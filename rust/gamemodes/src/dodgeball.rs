//! Dodgeball, ported from the decompiled C# `DodgeballManager : GameRuleManager` (@dump.cs:5118) and
//! `Dodgeball : Ball : RRNetworkBehavior` (@5010). Built-in RR Original games are hardcoded C# rule managers, not
//! loadable circuits — so this is a faithful reimplementation of the manager's rules + the ball's event hooks, kept
//! engine-agnostic: the client/authoritative server feeds it ball events (thrown / hit / caught) and ticks it, and
//! implements [`DodgeballEffects`] (eliminate, respawn, score, SFX/VFX) over `rr-net`.
//!
//! RULES SOURCE (per the integrator's steer, since the IL2CPP dump is signatures-only and body recovery is noisy):
//! ported from the descriptive field/method NAMES + types, the authoritative CONFIG numbers from the prefab, and the
//! well-understood Dodgeball ruleset — a ROUND is last-team-standing: players are eliminated when a thrown ball hits
//! them *unless it's caught* (a catch puts the THROWER out); when only one team has live players the round ends and
//! that team scores a round-win; first team to `ScoreToWin` round-wins takes the match; players revive between
//! rounds. Decisions that a cleanly-recovered method body would pin down further are marked `// 1:1:`. Config values
//! are placeholders until the real `DodgeballGameRuleWrapper`/`DodgeballManager` prefab numbers are decoded.

use std::collections::HashMap;

pub type PlayerId = u32;
pub type Team = u8;

/// Game-rule config (`DodgeballGameRuleData` @29720 + `defaultScoreToWin`). Placeholder numbers until the real prefab
/// values are decoded.
#[derive(Clone, Debug)]
pub struct DodgeballConfig {
    /// round-wins needed to take the match (`ScoreToWin` / `defaultScoreToWin`)
    pub score_to_win: u32,
    pub pre_round_duration: f32,
    pub post_round_duration: f32,
}

impl Default for DodgeballConfig {
    fn default() -> Self {
        // 1:1 decoded from the prefabs + dump.cs (decompile lane, sweet-galileo_dodgeball-recon.md §EXACT CONFIG):
        //   ScoreToWin = DodgeballGameRuleWrapper.defaultScoreToWin = 3
        //   PreRoundDuration / PostRoundDuration = DodgeballManager consts 3f / 3f (the third const, 0.35f, is a
        //   separate threshold whose exact use needs the method body — see on_player_hit).
        DodgeballConfig { score_to_win: 3, pre_round_duration: 3.0, post_round_duration: 3.0 }
    }
}

/// The real `DodgeballBall_phys` PhysicsMaterial (decoded 1:1): the ball bounces and grips the floor the same on every
/// client. Used by the client/server when it simulates the ball (friction/bounce). The ball's Rigidbody mass/drag and
/// the release throw velocity live in `Dodgeball.OnPostRelease` (a method body not yet recovered), so they're not here.
pub struct BallPhysics {
    pub dynamic_friction: f32,
    pub static_friction: f32,
    pub bounciness: f32,
}
pub const BALL_PHYSICS: BallPhysics = BallPhysics { dynamic_friction: 0.6, static_friction: 0.6, bounciness: 0.6 };

/// The round/match lifecycle (`IsRoundRunning` + Pre/PostRoundDuration). The `f32` is seconds left in a timed phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    PreRound(f32),
    Running,
    PostRound(f32),
    GameOver(Team),
}

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub team: Team,
    pub alive: bool,
}

/// Side effects the manager emits; the client/authoritative server implements them (networked elimination/respawn,
/// scoreboard, the manager's SFX/VFX — roundStartSFX, playerOutSFX/VO, playerImpactVFX, …).
pub trait DodgeballEffects {
    fn round_started(&mut self, _round: u32) {}
    fn round_over(&mut self, _winner: Option<Team>) {}
    fn match_over(&mut self, _winner: Team) {}
    fn player_out(&mut self, _player: PlayerId) {}
    fn player_respawned(&mut self, _player: PlayerId) {}
    fn score_changed(&mut self, _team: Team, _round_wins: u32) {}
    fn sfx(&mut self, _name: &str) {}
}

/// Port of `DodgeballManager`: rounds, score-to-win, last-team-standing elimination, timing. Consumes ball events
/// (the ball gadget detects hits/catches and fires `DodgeballPlayerHitEvent` / `DodgeballCatchEvent`).
pub struct DodgeballManager {
    pub config: DodgeballConfig,
    pub phase: Phase,
    pub round: u32,
    pub players: HashMap<PlayerId, PlayerState>,
    /// round-wins per team
    pub team_score: Vec<u32>,
}

impl DodgeballManager {
    /// `DodgeballManager.Start`: begins in the pre-round countdown of round 1.
    pub fn new(config: DodgeballConfig) -> Self {
        let pre = config.pre_round_duration;
        DodgeballManager { config, phase: Phase::PreRound(pre), round: 1, players: HashMap::new(), team_score: vec![0, 0] }
    }

    pub fn add_player(&mut self, id: PlayerId, team: Team) {
        self.players.insert(id, PlayerState { team, alive: true });
        if team as usize >= self.team_score.len() {
            self.team_score.resize(team as usize + 1, 0);
        }
    }

    pub fn is_round_running(&self) -> bool {
        self.phase == Phase::Running
    }

    /// `DodgeballManager.Update`: advances the round timer; the timed phases are unambiguous.
    pub fn tick(&mut self, dt: f32, fx: &mut dyn DodgeballEffects) {
        match self.phase {
            Phase::PreRound(t) => {
                let t = t - dt;
                if t <= 0.0 {
                    self.begin_round(fx);
                } else {
                    self.phase = Phase::PreRound(t);
                }
            }
            Phase::PostRound(t) => {
                let t = t - dt;
                if t <= 0.0 {
                    self.next_round(fx);
                } else {
                    self.phase = Phase::PostRound(t);
                }
            }
            Phase::Running | Phase::GameOver(_) => {}
        }
    }

    /// `KTLYQVYAOGB` round-begin: everyone revives, round goes live.
    fn begin_round(&mut self, fx: &mut dyn DodgeballEffects) {
        for (id, p) in self.players.iter_mut() {
            if !p.alive {
                p.alive = true;
                fx.player_respawned(*id);
            }
        }
        self.phase = Phase::Running;
        fx.round_started(self.round);
        fx.sfx("roundStart");
    }

    fn next_round(&mut self, fx: &mut dyn DodgeballEffects) {
        self.round += 1;
        self.phase = Phase::PreRound(self.config.pre_round_duration);
        let _ = fx;
    }

    /// A ball thrown by `thrower` hit `victim` (ball's `DodgeballPlayerHitEvent`). The ball does the hit detection
    /// (`OnCollisionEnter`); a hit (not a catch) puts the victim out.
    pub fn on_player_hit(&mut self, victim: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        // 1:1: confirm friendly-fire / self-hit (ball OWEMYSVBCGK). For now a hit between live cross-team players counts.
        let (Some(&PlayerState { team: vt, alive: true }), Some(tt)) =
            (self.players.get(&victim), self.players.get(&thrower).map(|p| p.team))
        else {
            return;
        };
        if vt == tt {
            return;
        }
        fx.sfx("playerImpact");
        self.eliminate(victim, fx);
    }

    /// `catcher` caught a ball thrown by `thrower` (ball's `DodgeballCatchEvent`): the thrower is out.
    pub fn on_catch(&mut self, _catcher: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        // 1:1: confirm the catch rule (thrower out? a teammate also revived?) against the ball/manager body
        self.eliminate(thrower, fx);
    }

    fn eliminate(&mut self, player: PlayerId, fx: &mut dyn DodgeballEffects) {
        if self.players.get(&player).map(|p| p.alive) != Some(true) {
            return;
        }
        self.players.get_mut(&player).unwrap().alive = false;
        fx.player_out(player);
        fx.sfx("playerOut");
        self.check_round_end(fx);
    }

    /// When one team (or none) still has live players, the round is over: the surviving team scores a round-win.
    fn check_round_end(&mut self, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        let mut live: Vec<Team> = self.players.values().filter(|p| p.alive).map(|p| p.team).collect();
        live.sort_unstable();
        live.dedup();
        if live.len() <= 1 {
            // 1:1: a round timeout (most-players-left wins) may also exist; not modeled until confirmed
            self.round_won(live.first().copied(), fx);
        }
    }

    fn round_won(&mut self, winner: Option<Team>, fx: &mut dyn DodgeballEffects) {
        if let Some(t) = winner {
            self.team_score[t as usize] += 1;
            let score = self.team_score[t as usize];
            fx.score_changed(t, score);
            fx.sfx("playerScore");
            if score >= self.config.score_to_win {
                self.phase = Phase::GameOver(t);
                fx.round_over(Some(t));
                fx.match_over(t);
                return;
            }
        }
        self.phase = Phase::PostRound(self.config.post_round_duration);
        fx.round_over(winner);
        fx.sfx("roundOver");
    }
}

// ---- the authority GameMode: the manager plus the ball simulation it owns ----

use crate::mode::{GameEvent, GameInput, GameMode, ObjectId, PlayerPose, RoundPhase};

const BALL_RADIUS: f32 = 0.11; // the real DodgeballBall SphereCollider radius
const HIT_RADIUS: f32 = 0.5; // a ball within this of a player's torso is a hit (the ball gadget's OnCollisionEnter)
const CATCH_RADIUS: f32 = 0.7; // a grab of an in-flight ball within this of the grabber is a catch
const TORSO_UP: f32 = 1.0; // torso height above the player's root
const BALL_GRAVITY: f32 = 9.81;

/// An authority-owned ball in flight.
struct FlightBall {
    id: ObjectId,
    pos: [f32; 3],
    vel: [f32; 3],
    thrower: PlayerId,
}

/// Dodgeball as a [`GameMode`] on the room authority: wraps [`DodgeballManager`] (the rules) and owns the thrown balls
/// (ballistic sim + the hit/catch decision against the authoritative player poses). Clients send throws and catch
/// grabs; this decides out/score and emits [`GameEvent`]s for the authority to broadcast. Config is the decompile
/// lane's 1:1 numbers via [`DodgeballConfig::default`].
pub struct DodgeballMode {
    mgr: DodgeballManager,
    balls: Vec<FlightBall>,
    next_object: ObjectId,
    joined: u32,
}

impl DodgeballMode {
    pub fn new(config: DodgeballConfig) -> Self {
        DodgeballMode { mgr: DodgeballManager::new(config), balls: Vec::new(), next_object: 1, joined: 0 }
    }

    /// the court floor, from the lowest player (balls below it are dead); never above 0
    fn floor(players: &[PlayerPose]) -> f32 {
        players.iter().map(|p| p.pos[1]).fold(f32::INFINITY, f32::min).min(0.0)
    }
}

/// Collects the manager's effects into broadcastable [`GameEvent`]s.
struct Collect<'a> {
    out: &'a mut Vec<GameEvent>,
    round: u32,
}
impl DodgeballEffects for Collect<'_> {
    fn round_started(&mut self, r: u32) {
        self.round = r;
        self.out.push(GameEvent::Round { round: r, phase: RoundPhase::Running, winner: None });
    }
    fn round_over(&mut self, w: Option<Team>) {
        self.out.push(GameEvent::Round { round: self.round, phase: RoundPhase::PostRound, winner: w });
    }
    fn match_over(&mut self, w: Team) {
        self.out.push(GameEvent::Round { round: self.round, phase: RoundPhase::GameOver, winner: Some(w) });
    }
    fn player_out(&mut self, p: PlayerId) {
        self.out.push(GameEvent::PlayerOut(p));
    }
    fn player_respawned(&mut self, p: PlayerId) {
        self.out.push(GameEvent::PlayerRespawned(p));
    }
    fn score_changed(&mut self, t: Team, s: u32) {
        self.out.push(GameEvent::Score { team: t, score: s });
    }
    fn sfx(&mut self, name: &str) {
        self.out.push(GameEvent::Cue(name.to_string()));
    }
}

fn torso(root: [f32; 3]) -> [f32; 3] {
    [root[0], root[1] + TORSO_UP, root[2]]
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

impl GameMode for DodgeballMode {
    fn name(&self) -> &'static str {
        "Dodgeball"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        let team = (self.joined % 2) as Team; // alternate so the teams stay balanced
        self.joined += 1;
        self.mgr.add_player(id, team);
        team
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.mgr.players.remove(&id);
        self.balls.retain(|b| b.thrower != id);
    }

    fn input(&mut self, input: GameInput, players: &[PlayerPose], out: &mut Vec<GameEvent>) {
        match input {
            GameInput::Throw { player, origin, velocity } => {
                let id = self.next_object;
                self.next_object += 1;
                self.balls.push(FlightBall { id, pos: origin, vel: velocity, thrower: player });
                out.push(GameEvent::ObjectState { object: id, pos: origin, vel: velocity });
            }
            GameInput::Grab { player, object } => {
                // grabbing an IN-FLIGHT ball within catch range is a catch: the thrower is out, the ball leaves play
                if let Some(i) = self.balls.iter().position(|b| b.id == object) {
                    let near = players.iter().find(|p| p.id == player).is_some_and(|g| dist(self.balls[i].pos, torso(g.pos)) < CATCH_RADIUS);
                    if near {
                        let thrower = self.balls[i].thrower;
                        self.balls.swap_remove(i);
                        out.push(GameEvent::ObjectGone { object });
                        let mut c = Collect { out: &mut *out, round: self.mgr.round };
                        self.mgr.on_catch(player, thrower, &mut c);
                    }
                }
            }
            GameInput::Drop { object, pos, .. } => {
                if let Some(b) = self.balls.iter_mut().find(|b| b.id == object) {
                    b.pos = pos;
                    b.vel = [0.0; 3];
                    out.push(GameEvent::ObjectState { object, pos, vel: [0.0; 3] });
                }
            }
        }
    }

    fn tick(&mut self, dt: f32, players: &[PlayerPose], out: &mut Vec<GameEvent>) {
        // round / match timer
        {
            let mut c = Collect { out: &mut *out, round: self.mgr.round };
            self.mgr.tick(dt, &mut c);
        }
        let floor = Self::floor(players);
        let mut i = 0;
        while i < self.balls.len() {
            self.balls[i].vel[1] -= BALL_GRAVITY * dt;
            for k in 0..3 {
                self.balls[i].pos[k] += self.balls[i].vel[k] * dt;
            }
            let pos = self.balls[i].pos;
            // hit the floor -> dead
            if pos[1] <= floor + BALL_RADIUS {
                let id = self.balls[i].id;
                self.balls.swap_remove(i);
                out.push(GameEvent::ObjectGone { object: id });
                continue;
            }
            // hit a cross-team, alive player -> they are out (unless they catch it first, handled in `input`)
            let thrower = self.balls[i].thrower;
            let thrower_team = self.mgr.players.get(&thrower).map(|s| s.team);
            let victim = players.iter().find(|p| {
                p.id != thrower
                    && self.mgr.players.get(&p.id).map(|s| s.alive) == Some(true)
                    && thrower_team != Some(p.team)
                    && dist(pos, torso(p.pos)) < HIT_RADIUS
            });
            if let Some(v) = victim {
                let (vid, id) = (v.id, self.balls[i].id);
                self.balls.swap_remove(i);
                out.push(GameEvent::ObjectGone { object: id });
                let mut c = Collect { out: &mut *out, round: self.mgr.round };
                self.mgr.on_player_hit(vid, thrower, &mut c);
                continue;
            }
            // still flying -> broadcast its state so every client renders the same arc
            out.push(GameEvent::ObjectState { object: self.balls[i].id, pos, vel: self.balls[i].vel });
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Rec {
        log: Vec<String>,
    }
    impl DodgeballEffects for Rec {
        fn round_started(&mut self, r: u32) {
            self.log.push(format!("start {r}"));
        }
        fn round_over(&mut self, w: Option<Team>) {
            self.log.push(format!("round_over {w:?}"));
        }
        fn match_over(&mut self, w: Team) {
            self.log.push(format!("match_over {w}"));
        }
        fn player_out(&mut self, p: PlayerId) {
            self.log.push(format!("out {p}"));
        }
        fn score_changed(&mut self, t: Team, s: u32) {
            self.log.push(format!("score t{t}={s}"));
        }
    }

    #[test]
    fn round_is_last_team_standing() {
        let cfg = DodgeballConfig { score_to_win: 2, pre_round_duration: 1.0, post_round_duration: 1.0 };
        let mut m = DodgeballManager::new(cfg);
        m.add_player(1, 0);
        m.add_player(2, 0);
        m.add_player(3, 1);
        let mut fx = Rec::default();

        m.tick(1.0, &mut fx); // pre-round elapses -> running
        assert!(m.is_round_running());

        // team 0 eliminates team 1's only player -> team 0 wins the round (score 1), not a win yet
        m.on_player_hit(3, 1, &mut fx);
        assert_eq!(m.team_score[0], 1);
        assert_eq!(m.phase, Phase::PostRound(1.0));

        // next round: everyone revives
        m.tick(1.0, &mut fx); // post-round -> pre-round(next)
        assert_eq!(m.round, 2);
        m.tick(1.0, &mut fx); // pre-round -> running
        assert!(m.players[&3].alive);

        // team 0 wins round 2 -> reaches score_to_win=2 -> match over
        m.on_player_hit(3, 1, &mut fx);
        assert_eq!(m.team_score[0], 2);
        assert_eq!(m.phase, Phase::GameOver(0));
        assert!(fx.log.contains(&"match_over 0".to_string()));
    }

    #[test]
    fn a_catch_eliminates_the_thrower_and_can_end_the_round() {
        let mut m = DodgeballManager::new(DodgeballConfig { pre_round_duration: 0.0, ..Default::default() });
        m.add_player(1, 0);
        m.add_player(2, 1);
        let mut fx = Rec::default();
        m.tick(0.1, &mut fx); // running
        m.on_catch(2, 1, &mut fx); // p2 catches p1's throw -> p1 (team 0) out -> team 1 last standing -> round win
        assert_eq!(m.players[&1].alive, false);
        assert_eq!(m.team_score[1], 1);
    }

    #[test]
    fn authority_simulates_the_ball_decides_the_hit_and_scores() {
        use crate::mode::{GameEvent, GameInput, GameMode, PlayerPose, RoundPhase};
        // two players on opposite teams; score_to_win 1 so one wiped team ends the match
        let mut m = DodgeballMode::new(DodgeballConfig { score_to_win: 1, pre_round_duration: 0.0, post_round_duration: 1.0 });
        let t0 = m.add_player(1);
        let t1 = m.add_player(2);
        assert_ne!(t0, t1, "teams alternate");
        let poses = vec![PlayerPose { id: 1, team: t0, pos: [0.0, 0.0, 0.0] }, PlayerPose { id: 2, team: t1, pos: [0.0, 0.0, 5.0] }];

        let mut out = Vec::new();
        m.tick(0.1, &poses, &mut out); // pre-round(0) elapses -> Running
        assert!(out.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::Running, .. })));

        // player 1 (team 0) throws at player 2's (team 1) torso [0,1,5]; a ballistic arc that lands on it
        m.input(GameInput::Throw { player: 1, origin: [0.0, 1.3, 0.0], velocity: [0.0, 1.86, 10.0] }, &poses, &mut out);

        let (mut out_player, mut scoring_team, mut saw_flight) = (None, None, false);
        for _ in 0..600 {
            let mut ev = Vec::new();
            m.tick(1.0 / 60.0, &poses, &mut ev);
            for e in &ev {
                match e {
                    GameEvent::ObjectState { .. } => saw_flight = true,
                    GameEvent::PlayerOut(p) => out_player = Some(*p),
                    GameEvent::Score { team, .. } => scoring_team = Some(*team),
                    _ => {}
                }
            }
            if scoring_team.is_some() {
                break;
            }
        }
        assert!(saw_flight, "the authority broadcasts the ball's flight");
        assert_eq!(out_player, Some(2), "the authority decides player 2 is hit out");
        assert_eq!(scoring_team, Some(t0), "team 0 scores the round win");
    }
}
