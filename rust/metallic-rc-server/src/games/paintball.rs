//! A Paintball match in one room: lobby -> countdown -> playing -> round end -> results -> lobby, Team Battle
//! (first team to `target` hits) or Capture the Flag (first to `target` captures). The server moves the paintballs
//! (speed and gravity) and decides hits against the players' last known positions and the bots; a hit player is out
//! and comes back at a team spawn after a moment.
//!
//! From the game's data: paint speed 33 m/s and at most 5 shots a second (the `(PaintballGun)` prefab's `Pistol`:
//! `bulletFireSpeed`, `maximumFiringRate`). INFERRED (ours): gravity 9.81 (Unity's default), the body as a capsule
//! 0.35 m round from 0.35 to 1.55 m above the feet, paintball radius 0.06, out for 3 s, countdown 5 s, round end 4 s,
//! results 10 s, Team Battle to 15 in 5 min, CTF to 3 in 10 min, flag reach 1.5 m, goal reach 2 m, a dropped flag
//! goes home after 20 s or when its own team touches it, no friendly fire, no lag compensation.
use std::collections::BTreeMap;

use rr_net::games::{BotPose, Down, Flag, MapInfo, Member, Mode, NoteKind, Phase, Spot, State, Up, BLUE, NOBODY, RED};
use rr_net::ngo::NgoDelivery;

use super::{bots::Brain, Send};

pub const GUN_SPEED: f32 = 33.0;
pub const FIRE_RATE: f32 = 5.0;
pub const GRAVITY: f32 = 9.81;
pub const OUT_FOR: f64 = 3.0;
pub const COUNTDOWN: f64 = 5.0;
pub const ROUND_END: f64 = 4.0;
pub const RESULTS: f64 = 10.0;
pub const BODY_RADIUS: f32 = 0.35;
pub const BODY_LOW: f32 = 0.35;
pub const BODY_HIGH: f32 = 1.55;
pub const BALL_RADIUS: f32 = 0.06;
pub const FLAG_REACH: f32 = 1.5;
pub const GOAL_REACH: f32 = 2.0;
pub const FLAG_RETURN: f64 = 20.0;
/// a paintball flies at most this long (the client says when it meets the world)
pub const MAX_LIFE: f32 = 3.0;
/// state messages a second
const STATE_HZ: f64 = 10.0;
const STEP: f64 = 1.0 / 60.0;
const MAX_BOTS: usize = 16;

pub type V = [f32; 3];

pub fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn len(a: V) -> f32 {
    dot(a, a).sqrt()
}
/// Distance on the ground (x, z).
pub fn flat(a: V, b: V) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The closest distance between segments p0-p1 and q0-q1.
pub fn segment_distance(p0: V, p1: V, q0: V, q1: V) -> f32 {
    let (d1, d2, r) = (sub(p1, p0), sub(q1, q0), sub(p0, q0));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (s, t) = if a <= 1e-9 && e <= 1e-9 {
        (0.0, 0.0)
    } else if a <= 1e-9 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = dot(d1, r);
        if e <= 1e-9 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s = if denom > 1e-9 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    len(sub(add(p0, scale(d1, s)), add(q0, scale(d2, t))))
}

/// Whether a paintball moving from `a` to `b` touches a body standing at `feet`.
pub fn hits_body(a: V, b: V, feet: V) -> bool {
    segment_distance(a, b, add(feet, [0.0, BODY_LOW, 0.0]), add(feet, [0.0, BODY_HIGH, 0.0])) <= BODY_RADIUS + BALL_RADIUS
}

struct Player {
    team: u8,
    name: String,
    /// a real player's connection; bots have none
    client: Option<u64>,
    feet: V,
    yaw: f32,
    vel: V,
    /// its position is known (a player has sent movement, a bot always)
    seen: bool,
    out_until: Option<f64>,
    kills: u16,
    outs: u16,
    caps: u16,
    last_fire: f64,
    bot: Option<Brain>,
}

struct Shot {
    shooter: i32,
    team: u8,
    pos: V,
    vel: V,
    left: f32,
}

#[derive(Clone, Copy)]
struct FlagState {
    flag: Flag,
    dropped_at: f64,
}

pub struct Match {
    mode: Mode,
    phase: Phase,
    phase_end: f64,
    target: u16,
    seconds: u16,
    scores: [u16; 2],
    winner: u8,
    map: Option<MapInfo>,
    players: BTreeMap<i32, Player>,
    shots: Vec<Shot>,
    flags: [FlagState; 2],
    next_spawn: [usize; 2],
    next_bot: i32,
    last: f64,
    carry: f64,
    next_state: f64,
    rng: u64,
    outbox: Vec<(Option<u64>, Down, NgoDelivery)>,
}

impl Match {
    pub fn new(now: f64) -> Match {
        Match {
            mode: Mode::TeamBattle,
            phase: Phase::Lobby,
            phase_end: now,
            target: 15,
            seconds: 300,
            scores: [0; 2],
            winner: NOBODY,
            map: None,
            players: BTreeMap::new(),
            shots: vec![],
            flags: [FlagState { flag: Flag { home: true, ..Default::default() }, dropped_at: 0.0 }; 2],
            next_spawn: [0; 2],
            next_bot: -1,
            last: now,
            carry: 0.0,
            next_state: now,
            rng: 0x9E37_79B9_7F4A_7C15,
            outbox: vec![],
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn scores(&self) -> [u16; 2] {
        self.scores
    }

    pub fn team_of(&self, id: i32) -> Option<u8> {
        self.players.get(&id).map(|p| p.team)
    }

    pub fn is_out(&self, id: i32) -> bool {
        self.players.get(&id).is_some_and(|p| p.out_until.is_some())
    }

    pub fn bots(&self) -> usize {
        self.players.values().filter(|p| p.bot.is_some()).count()
    }

    /// Where `id` stands now.
    pub fn position(&self, id: i32) -> Option<V> {
        self.players.get(&id).map(|p| p.feet)
    }

    pub fn flag(&self, team: u8) -> Flag {
        self.flags[team as usize].flag
    }

    fn random(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn send(&mut self, to: Option<u64>, d: Down, delivery: NgoDelivery) {
        self.outbox.push((to, d, delivery));
    }

    fn all(&mut self, d: Down) {
        self.send(None, d, NgoDelivery::ReliableFragmentedSequenced);
    }

    /// The smaller team (red on a tie).
    fn smaller_team(&self) -> u8 {
        let red = self.players.values().filter(|p| p.team == RED).count();
        let blue = self.players.len() - red;
        if blue < red { BLUE } else { RED }
    }

    pub fn join(&mut self, _now: f64, client: u64, name: &str) {
        let team = self.smaller_team();
        self.players.insert(client as i32, Player {
            team,
            name: name.to_string(),
            client: Some(client),
            feet: [0.0; 3],
            yaw: 0.0,
            vel: [0.0; 3],
            seen: false,
            out_until: None,
            kills: 0,
            outs: 0,
            caps: 0,
            last_fire: f64::NEG_INFINITY,
            bot: None,
        });
        if let Some(map) = self.map.clone() {
            self.send(Some(client), Down::Map(map), NgoDelivery::ReliableFragmentedSequenced);
        }
        // the next state message tells everyone
        self.next_state = f64::NEG_INFINITY;
    }

    pub fn leave(&mut self, client: u64) {
        let id = client as i32;
        self.drop_flag_of(id, 0.0);
        self.players.remove(&id);
        self.next_state = f64::NEG_INFINITY;
    }

    pub fn moved(&mut self, client: u64, feet: V, yaw: f32) {
        if let Some(p) = self.players.get_mut(&(client as i32)) {
            (p.feet, p.yaw, p.seen) = (feet, yaw, true);
        }
    }

    pub fn handle(&mut self, now: f64, from: u64, up: Up) {
        let id = from as i32;
        if !self.players.contains_key(&id) {
            return;
        }
        match up {
            Up::Start { mode, target, seconds, map } if matches!(self.phase, Phase::Lobby | Phase::Results) => {
                self.start(now, mode, target, seconds, map);
            }
            Up::Start { .. } => {}
            Up::Stop => self.to_lobby(now),
            Up::Bots { count: 0 } => {
                self.players.retain(|_, p| p.bot.is_none());
                self.next_state = f64::NEG_INFINITY;
            }
            Up::Bots { count } => {
                for _ in 0..count {
                    if self.bots() >= MAX_BOTS {
                        break;
                    }
                    self.add_bot(now);
                }
            }
            Up::Team { team } if team < 2 && self.phase == Phase::Lobby => {
                if let Some(p) = self.players.get_mut(&id) {
                    p.team = team;
                }
                self.next_state = f64::NEG_INFINITY;
            }
            Up::Team { .. } => {}
            Up::Fire { origin, velocity, life } => self.fire(now, id, origin, velocity, life),
        }
    }

    fn add_bot(&mut self, now: f64) {
        let id = self.next_bot;
        self.next_bot -= 1;
        let team = self.smaller_team();
        let name = format!("Bot {}", -id);
        self.players.insert(id, Player {
            team,
            name,
            client: None,
            feet: [0.0; 3],
            yaw: 0.0,
            vel: [0.0; 3],
            seen: true,
            out_until: None,
            kills: 0,
            outs: 0,
            caps: 0,
            last_fire: f64::NEG_INFINITY,
            bot: Some(Brain::new(now, id)),
        });
        let spot = self.spawn_spot(team, self.phase != Phase::Lobby);
        self.respawn(id, spot);
        self.next_state = f64::NEG_INFINITY;
    }

    /// A team spawn (round robin), or a lobby spot.
    fn spawn_spot(&mut self, team: u8, in_match: bool) -> Spot {
        let Some(map) = &self.map else { return Spot::default() };
        let list = if in_match && !map.spawns[team as usize].is_empty() { &map.spawns[team as usize] } else if !map.lobby.is_empty() { &map.lobby } else { &map.spawns[team as usize] };
        if list.is_empty() {
            return Spot::default();
        }
        let i = self.next_spawn[team as usize] % list.len();
        self.next_spawn[team as usize] += 1;
        list[i]
    }

    fn respawn(&mut self, id: i32, spot: Spot) {
        if let Some(p) = self.players.get_mut(&id) {
            (p.feet, p.yaw, p.vel, p.out_until) = (spot.pos, spot.yaw, [0.0; 3], None);
            if p.client.is_some() {
                self.all(Down::Respawn { id, spot });
            }
        }
    }

    fn start(&mut self, now: f64, mode: Mode, target: u16, seconds: u16, map: MapInfo) {
        self.mode = mode;
        self.target = target.clamp(1, 999);
        self.seconds = seconds.clamp(10, 3600);
        self.scores = [0; 2];
        self.winner = NOBODY;
        self.shots.clear();
        self.flags = [0, 1].map(|t| FlagState { flag: Flag { at: map.flags[t], carrier: 0, home: true }, dropped_at: 0.0 });
        self.map = Some(map.clone());
        self.all(Down::Map(map));
        let ids: Vec<i32> = self.players.keys().copied().collect();
        for id in ids {
            let p = self.players.get_mut(&id).unwrap();
            (p.kills, p.outs, p.caps) = (0, 0, 0);
            let team = p.team;
            let spot = self.spawn_spot(team, true);
            self.respawn(id, spot);
        }
        self.phase = Phase::Countdown;
        self.phase_end = now + COUNTDOWN;
        self.next_state = f64::NEG_INFINITY;
    }

    fn to_lobby(&mut self, now: f64) {
        self.phase = Phase::Lobby;
        self.phase_end = now;
        self.shots.clear();
        if let Some(map) = &self.map {
            self.flags = [0, 1].map(|t| FlagState { flag: Flag { at: map.flags[t], carrier: 0, home: true }, dropped_at: 0.0 });
        }
        let ids: Vec<i32> = self.players.keys().copied().collect();
        for id in ids {
            let team = self.players[&id].team;
            let spot = self.spawn_spot(team, false);
            self.respawn(id, spot);
        }
        self.next_state = f64::NEG_INFINITY;
    }

    /// A paintball from `id`: checked against the gun's rate and the shooter's place, then flown by the server.
    fn fire(&mut self, now: f64, id: i32, origin: V, velocity: V, life: f32) {
        let Some(p) = self.players.get(&id) else { return };
        let speed = len(velocity);
        if p.out_until.is_some() || now - p.last_fire < 1.0 / FIRE_RATE as f64 - 0.05 || !(speed > 1.0) || !life.is_finite() {
            return;
        }
        if p.seen && len(sub(origin, add(p.feet, [0.0, 1.4, 0.0]))) > 3.0 {
            return;
        }
        if matches!(self.phase, Phase::Countdown | Phase::RoundEnd | Phase::Results) {
            return;
        }
        let velocity = scale(velocity, GUN_SPEED / speed);
        let life = life.clamp(0.0, MAX_LIFE);
        let (team, client) = (p.team, p.client);
        self.players.get_mut(&id).unwrap().last_fire = now;
        self.launch(id, team, client, origin, velocity, life);
    }

    fn launch(&mut self, shooter: i32, team: u8, client: Option<u64>, origin: V, velocity: V, life: f32) {
        // practice in the lobby: drawn for everyone, nobody gets hit
        if self.phase == Phase::Playing {
            self.shots.push(Shot { shooter, team, pos: origin, vel: velocity, left: life });
        }
        let d = Down::Shot { shooter, origin, velocity, life };
        let to: Vec<u64> = self.players.values().filter_map(|p| p.client).filter(|&c| Some(c) != client).collect();
        for c in to {
            self.send(Some(c), d.clone(), NgoDelivery::ReliableFragmentedSequenced);
        }
    }

    pub fn update(&mut self, now: f64) {
        let mut dt = (now - self.last).clamp(0.0, 0.25);
        self.last = now;
        self.carry += dt;
        while self.carry >= STEP {
            self.carry -= STEP;
            dt = STEP;
            self.step(now, dt as f32);
        }
        if now >= self.next_state {
            self.next_state = now + 1.0 / STATE_HZ;
            let s = Down::State(self.state(now));
            let to: Vec<u64> = self.players.values().filter_map(|p| p.client).collect();
            for c in to {
                self.send(Some(c), s.clone(), NgoDelivery::UnreliableSequenced);
            }
        }
    }

    fn step(&mut self, now: f64, dt: f32) {
        // phases
        if now >= self.phase_end {
            match self.phase {
                Phase::Countdown => {
                    self.phase = Phase::Playing;
                    self.phase_end = now + self.seconds as f64;
                    self.next_state = f64::NEG_INFINITY;
                }
                Phase::Playing => self.end_round(now),
                Phase::RoundEnd => {
                    self.phase = Phase::Results;
                    self.phase_end = now + RESULTS;
                    self.next_state = f64::NEG_INFINITY;
                }
                Phase::Results => self.to_lobby(now),
                Phase::Lobby => {}
            }
        }
        // the out come back
        let back: Vec<(i32, u8)> = self.players.iter().filter(|(_, p)| p.out_until.is_some_and(|t| now >= t)).map(|(&id, p)| (id, p.team)).collect();
        for (id, team) in back {
            let spot = self.spawn_spot(team, self.phase != Phase::Lobby);
            self.respawn(id, spot);
            self.next_state = f64::NEG_INFINITY;
        }
        if self.phase != Phase::Playing && self.phase != Phase::Lobby {
            return;
        }
        self.think_bots(now, dt);
        if self.phase == Phase::Playing {
            self.fly(now, dt);
            if self.mode == Mode::CaptureTheFlag {
                self.flags(now);
            }
            if self.scores.iter().any(|&s| s >= self.target) {
                self.end_round(now);
            }
        }
    }

    fn end_round(&mut self, now: f64) {
        self.winner = match self.scores {
            [r, b] if r > b => RED,
            [r, b] if b > r => BLUE,
            _ => NOBODY,
        };
        self.phase = Phase::RoundEnd;
        self.phase_end = now + ROUND_END;
        self.shots.clear();
        self.next_state = f64::NEG_INFINITY;
    }

    /// Moves the paintballs and hits whoever they meet.
    fn fly(&mut self, now: f64, dt: f32) {
        let mut hits = vec![];
        let mut shots = std::mem::take(&mut self.shots);
        shots.retain_mut(|s| {
            let a = s.pos;
            let step = dt.min(s.left);
            s.vel[1] -= GRAVITY * step;
            s.pos = add(s.pos, scale(s.vel, step));
            s.left -= step;
            let victim = self.players.iter()
                .filter(|(&id, p)| id != s.shooter && p.team != s.team && p.seen && p.out_until.is_none())
                .filter(|(_, p)| hits_body(a, s.pos, p.feet))
                .min_by(|x, y| len(sub(x.1.feet, a)).total_cmp(&len(sub(y.1.feet, a))))
                .map(|(&id, _)| id);
            if let Some(v) = victim {
                hits.push((s.shooter, v, s.pos));
                return false;
            }
            s.left > 0.0
        });
        self.shots = shots;
        for (shooter, victim, at) in hits {
            self.hit(now, shooter, victim, at);
        }
    }

    fn hit(&mut self, now: f64, shooter: i32, victim: i32, at: V) {
        let Some(v) = self.players.get_mut(&victim) else { return };
        if v.out_until.is_some() {
            return;
        }
        v.out_until = Some(now + OUT_FOR);
        v.outs += 1;
        if let Some(s) = self.players.get_mut(&shooter) {
            s.kills += 1;
            if self.mode == Mode::TeamBattle {
                self.scores[s.team as usize] += 1;
            }
        }
        self.drop_flag_of(victim, now);
        self.all(Down::Hit { shooter, victim, at });
        self.next_state = f64::NEG_INFINITY;
    }

    /// The flag `id` carries falls where it stands.
    fn drop_flag_of(&mut self, id: i32, now: f64) {
        let feet = self.players.get(&id).map(|p| p.feet);
        for t in 0..2 {
            if self.flags[t].flag.carrier == id && id != 0 {
                let f = &mut self.flags[t];
                f.flag.carrier = 0;
                f.flag.at = feet.map_or(f.flag.at, |p| add(p, [0.0, 1.0, 0.0]));
                f.dropped_at = now;
                self.all(Down::Note { kind: NoteKind::FlagDropped, actor: id, team: t as u8 });
            }
        }
    }

    /// Capture the Flag: taking, carrying, returning and capturing.
    fn flags(&mut self, now: f64) {
        let Some(map) = self.map.clone() else { return };
        let mut notes = vec![];
        let ids: Vec<i32> = self.players.keys().copied().collect();
        for id in ids {
            let p = &self.players[&id];
            if p.out_until.is_some() || !p.seen {
                continue;
            }
            let (team, chest) = (p.team, add(p.feet, [0.0, 1.0, 0.0]));
            for t in 0..2 {
                let f = &mut self.flags[t];
                if f.flag.carrier == 0 {
                    let near = flat(chest, f.flag.at) <= FLAG_REACH && (chest[1] - f.flag.at[1]).abs() <= 2.0;
                    if near && team as usize != t {
                        (f.flag.carrier, f.flag.home) = (id, false);
                        notes.push((NoteKind::FlagTaken, id, t as u8));
                    } else if near && !f.flag.home {
                        (f.flag.at, f.flag.home) = (map.flags[t], true);
                        notes.push((NoteKind::FlagReturned, id, t as u8));
                    }
                }
                if f.flag.carrier == id {
                    f.flag.at = add(chest, [0.0, 0.5, 0.0]);
                    if flat(chest, map.goals[team as usize]) <= GOAL_REACH {
                        f.flag = Flag { at: map.flags[t], carrier: 0, home: true };
                        self.scores[team as usize] += 1;
                        notes.push((NoteKind::Captured, id, t as u8));
                    }
                }
            }
            if notes.iter().any(|n| n.0 == NoteKind::Captured && n.1 == id) {
                self.players.get_mut(&id).unwrap().caps += 1;
            }
        }
        for t in 0..2 {
            let f = &mut self.flags[t];
            if f.flag.carrier == 0 && !f.flag.home && now - f.dropped_at >= FLAG_RETURN {
                (f.flag.at, f.flag.home) = (map.flags[t], true);
                notes.push((NoteKind::FlagReturned, 0, t as u8));
            }
        }
        if !notes.is_empty() {
            self.next_state = f64::NEG_INFINITY;
        }
        for (kind, actor, team) in notes {
            self.all(Down::Note { kind, actor, team });
        }
    }

    /// The bots move and shoot (see `bots`).
    fn think_bots(&mut self, now: f64, dt: f32) {
        let ids: Vec<i32> = self.players.iter().filter(|(_, p)| p.bot.is_some() && p.out_until.is_none()).map(|(&id, _)| id).collect();
        for id in ids {
            let view = super::bots::View::of(self, id);
            let Some(view) = view else { continue };
            let r1 = self.random();
            let r2 = self.random();
            let p = self.players.get_mut(&id).unwrap();
            let brain = p.bot.as_mut().unwrap();
            let act = brain.think(now, dt, &view, [r1, r2]);
            p.vel = act.velocity;
            p.feet = act.feet;
            p.yaw = act.yaw;
            if let (Some((origin, velocity)), Phase::Playing) = (act.fire, self.phase) {
                let team = p.team;
                p.last_fire = now;
                self.launch(id, team, None, origin, velocity, super::bots::SHOT_LIFE);
            }
        }
    }

    /// What every player is told, ten times a second.
    pub fn state(&self, now: f64) -> State {
        let members = self.players.iter().map(|(&id, p)| Member {
            id,
            team: p.team,
            out: p.out_until.is_some(),
            kills: p.kills,
            outs: p.outs,
            caps: p.caps,
            bot: p.bot.as_ref().map(|_| BotPose { name: p.name.clone(), pos: p.feet, yaw: p.yaw, vel: p.vel }),
        }).collect();
        State {
            mode: self.mode,
            phase: self.phase,
            time_left: (self.phase_end - now).max(0.0) as f32,
            scores: self.scores,
            target: self.target,
            winner: self.winner,
            members,
            flags: [self.flags[0].flag, self.flags[1].flag],
        }
    }

    // ---- read by the bots ----

    pub(super) fn map(&self) -> Option<&MapInfo> {
        self.map.as_ref()
    }

    pub(super) fn mode(&self) -> Mode {
        self.mode
    }

    /// Everyone standing (not out, position known): id, team, feet.
    pub(super) fn standing(&self) -> Vec<(i32, u8, V)> {
        self.players.iter().filter(|(_, p)| p.seen && p.out_until.is_none()).map(|(&id, p)| (id, p.team, p.feet)).collect()
    }

    pub(super) fn last_fire(&self, id: i32) -> f64 {
        self.players.get(&id).map_or(0.0, |p| p.last_fire)
    }

    pub fn take_outbox(&mut self) -> Vec<Send> {
        let mut out = vec![];
        let clients: Vec<u64> = self.players.values().filter_map(|p| p.client).collect();
        for (to, d, delivery) in self.outbox.drain(..) {
            let payload = d.encode();
            match to {
                Some(c) => out.push(Send { to: c, payload, delivery }),
                None => out.extend(clients.iter().map(|&c| Send { to: c, payload: payload.clone(), delivery })),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> MapInfo {
        MapInfo {
            name: "test".into(),
            lobby: vec![Spot { pos: [0.0, 0.0, 0.0], yaw: 0.0 }],
            spawns: [vec![Spot { pos: [0.0, 0.0, -20.0], yaw: 0.0 }], vec![Spot { pos: [0.0, 0.0, 20.0], yaw: std::f32::consts::PI }]],
            flags: [[0.0, 1.0, -22.0], [0.0, 1.0, 22.0]],
            goals: [[0.0, 0.8, -22.0], [0.0, 0.8, 22.0]],
            bounds: None,
        }
    }

    fn run(m: &mut Match, from: f64, to: f64) {
        let mut t = from;
        while t < to {
            t += 1.0 / 120.0;
            m.update(t);
        }
    }

    #[test]
    fn segments() {
        assert!((segment_distance([0.0; 3], [1.0, 0.0, 0.0], [0.5, 1.0, -1.0], [0.5, 1.0, 1.0]) - 1.0).abs() < 1e-5);
        assert!(hits_body([-1.0, 1.0, 0.0], [1.0, 1.0, 0.0], [0.0; 3]));
        assert!(!hits_body([-1.0, 2.5, 0.0], [1.0, 2.5, 0.0], [0.0; 3]));
    }

    #[test]
    fn team_battle_hit_scores_and_respawns() {
        let mut m = Match::new(0.0);
        m.join(0.0, 1, "Ada");
        m.join(0.0, 2, "Bo");
        assert_eq!((m.team_of(1), m.team_of(2)), (Some(RED), Some(BLUE)));
        m.handle(0.0, 1, Up::Start { mode: Mode::TeamBattle, target: 2, seconds: 60, map: map() });
        assert_eq!(m.phase(), Phase::Countdown);
        run(&mut m, 0.0, COUNTDOWN + 0.1);
        assert_eq!(m.phase(), Phase::Playing);
        // Ada at red spawn, Bo walked up to 10 m in front of her
        m.moved(1, [0.0, 0.0, -20.0], 0.0);
        m.moved(2, [0.0, 0.0, -10.0], 0.0);
        let t = COUNTDOWN + 0.1;
        m.handle(t, 1, Up::Fire { origin: [0.0, 1.4, -19.5], velocity: [0.0, 0.0, GUN_SPEED], life: 2.0 });
        run(&mut m, t, t + 0.6);
        assert!(m.is_out(2));
        assert_eq!(m.scores(), [1, 0]);
        // too soon to fire again is refused; a shot at someone out does nothing
        assert!(m.take_outbox().iter().any(|s| matches!(Down::decode(&s.payload), Ok(Down::Hit { shooter: 1, victim: 2, .. }))));
        run(&mut m, t + 0.6, t + 0.6 + OUT_FOR);
        assert!(!m.is_out(2));
        assert_eq!(m.position(2), Some([0.0, 0.0, 20.0]));
        assert!(m.take_outbox().iter().any(|s| s.to == 2 && matches!(Down::decode(&s.payload), Ok(Down::Respawn { id: 2, .. }))));
    }

    #[test]
    fn no_friendly_fire_and_rate_limit() {
        let mut m = Match::new(0.0);
        m.join(0.0, 1, "Ada");
        m.join(0.0, 2, "Bo");
        m.join(0.0, 3, "Cy");
        m.handle(0.0, 1, Up::Start { mode: Mode::TeamBattle, target: 5, seconds: 60, map: map() });
        run(&mut m, 0.0, COUNTDOWN + 0.1);
        let t = COUNTDOWN + 0.1;
        // Cy is red like Ada
        m.moved(1, [0.0, 0.0, 0.0], 0.0);
        m.moved(3, [0.0, 0.0, 5.0], 0.0);
        m.moved(2, [3.0, 0.0, 30.0], 0.0);
        m.handle(t, 1, Up::Fire { origin: [0.0, 1.4, 0.5], velocity: [0.0, 0.0, 5.0], life: 1.0 });
        m.handle(t + 0.01, 1, Up::Fire { origin: [0.0, 1.4, 0.5], velocity: [0.0, 0.0, 5.0], life: 1.0 });
        assert_eq!(m.shots.len(), 1);
        run(&mut m, t, t + 1.0);
        assert!(!m.is_out(3));
        assert_eq!(m.scores(), [0, 0]);
    }

    #[test]
    fn capture_the_flag() {
        let mut m = Match::new(0.0);
        m.join(0.0, 1, "Ada");
        m.join(0.0, 2, "Bo");
        m.handle(0.0, 1, Up::Start { mode: Mode::CaptureTheFlag, target: 1, seconds: 60, map: map() });
        run(&mut m, 0.0, COUNTDOWN + 0.1);
        let mut t = COUNTDOWN + 0.1;
        // Ada (red) takes blue's flag at z 22, gets hit, it drops; Bo touches it: home again
        m.moved(1, [0.0, 0.0, 22.0], 0.0);
        run(&mut m, t, t + 0.1);
        t += 0.1;
        assert_eq!(m.flag(BLUE).carrier, 1);
        m.moved(2, [0.0, 0.0, 15.0], 0.0);
        m.handle(t, 2, Up::Fire { origin: [0.0, 1.4, 15.5], velocity: [0.0, 0.0, 5.0], life: 2.0 });
        run(&mut m, t, t + 0.5);
        t += 0.5;
        assert!(m.is_out(1));
        assert_eq!((m.flag(BLUE).carrier, m.flag(BLUE).home), (0, false));
        m.moved(2, [0.0, 0.0, 22.0], 0.0);
        run(&mut m, t, t + 0.1);
        t += 0.1;
        assert!(m.flag(BLUE).home);
        // after coming back Ada takes it again and brings it to red's goal: red wins
        run(&mut m, t, t + OUT_FOR + 0.1);
        t += OUT_FOR + 0.1;
        m.moved(2, [10.0, 0.0, 0.0], 0.0);
        m.moved(1, [0.0, 0.0, 22.0], 0.0);
        run(&mut m, t, t + 0.1);
        t += 0.1;
        assert_eq!(m.flag(BLUE).carrier, 1);
        m.moved(1, [0.5, 0.0, -21.0], 0.0);
        run(&mut m, t, t + 0.1);
        assert_eq!(m.scores(), [1, 0]);
        assert!(m.flag(BLUE).home);
        assert_eq!(m.phase(), Phase::RoundEnd);
        assert_eq!(m.state(t).winner, RED);
    }

    #[test]
    fn bots_play_a_round() {
        let mut m = Match::new(0.0);
        m.join(0.0, 1, "Ada");
        m.handle(0.0, 1, Up::Bots { count: 3 });
        assert_eq!(m.bots(), 3);
        m.handle(0.0, 1, Up::Start { mode: Mode::TeamBattle, target: 3, seconds: 120, map: map() });
        // Ada never moves (not seen), so the bots only meet each other: 2 v 1 bots
        run(&mut m, 0.0, 120.0);
        assert!(m.scores().iter().sum::<u16>() > 0, "bots scored nothing: {:?}", m.scores());
        assert!(matches!(m.phase(), Phase::RoundEnd | Phase::Results | Phase::Lobby));
    }
}
