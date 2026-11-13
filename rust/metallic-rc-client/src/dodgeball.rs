//! Dodgeball over rr-net, master-client authority — mirroring Rec Room's real flow (verified against the decompiled
//! bodies). The thrown ball is the thrower's own networked prop: it keeps the hand's release velocity and flies on
//! that client, which detects the collision with a player body and sends the master a hit *request*
//! (`RpcMasterRequestPlayerOut`). The master (room authority) runs the verified `IsValidHit` rule via
//! [`gamemodes::DodgeballMode`] and, if the hit counts, broadcasts the outcome (`RpcOnPlayerOut`) for every client to
//! apply (topple + respawn). A catch is a grab of a live ball → `RpcMasterRequestPlayerCatch`. No outcome is
//! client-trusted; the master never simulates the ball.
//!
//! `--world dodgeball` sets up the court: the master spawns the real `(DodgeballBall)` props (grab/throwable through
//! the client's own `interact` grab/throw, which already applies the real release velocity). `METALLIC_DB_BOTS=1`
//! adds host-side bots that raise scripted hit requests, so scoring can be proven with a single client as authority.
//! Keep the client muted (owner's standing request).

use std::collections::HashMap;

use bevy::prelude::*;
use gamemodes::dodgeball::{DodgeballConfig, DodgeballMode as DodgeballRules};
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase};
use rr_net::objects::RpcTarget;

use crate::avatar::Rig;
use crate::netobj::{NetObject, NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

/// The game's own ball prop (the parentheses are part of the Addressables key).
const BALL_ADDRESS: &str = "(DodgeballBall)";
/// The balls on the centre line at the start of a round.
const BALL_COUNT: usize = 6;
/// A fixed scene index every client registers, so the game's request/outcome RPCs share one networked view.
const GAME_VIEW_INDEX: u32 = 47110;
const RPC_HIT: &str = "DbHit"; // client → master: a hit request (thrower, victim)
const RPC_CATCH: &str = "DbCatch"; // client → master: a catch request (catcher, thrower)
const RPC_OUT: &str = "DbOut"; // master → others: a player is out
const RPC_SCORE: &str = "DbScore"; // master → others: team score
const RPC_ROUND: &str = "DbRound"; // master → others: round state
/// 1:1: ball↔player contact distance for the owner-side collision — real = the ball SphereCollider (0.11) vs the
/// player body collider (Unity OnCollisionEnter). Approximated here until the exact body-collider size is decoded.
const CONTACT: f32 = 0.45;

/// Set when the client launched into the Dodgeball arena (`--world dodgeball`).
#[derive(Resource, Clone, Copy)]
pub struct DodgeballActive(pub bool);

/// Per-client Dodgeball state. The rules (`Some`) live only on the master; everyone keeps the shared game-authority
/// entity for RPCs and the scoreboard the authority broadcasts.
#[derive(Resource, Default)]
pub struct Dodgeball {
    rules: Option<DodgeballRules>,
    game: Option<Entity>,
    /// actors told to the rules, in join order (master only; also the team assignment)
    roster: Vec<i32>,
    /// requests the master received this frame, applied in `master_tick`
    pending: Vec<GameInput>,
    /// players currently out, and the scoreboard, as last broadcast
    out: Vec<u32>,
    score: [u32; 2],
    round: u32,
    court_set: bool,
    bots: Vec<Bot>,
    bot_elapsed: f32,
    /// the master should eliminate the first real guest once, so an over-the-net hit can be seen (2-client demo)
    demo_mode: bool,
    demo_hit: bool,
}

/// A scripted bot for the host-side self-test: it raises a hit request against a target at a set time.
struct Bot {
    thrower: i32,
    victim: i32,
    at: f32,
    fired: bool,
}

pub struct DodgeballPlugin;

impl Plugin for DodgeballPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dodgeball>().add_systems(Update, (join_game, owner_hit_detect, recv_rpcs, master_tick, apply_out_visual).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Once the arena is built and we're in a room, register the shared game-authority object and set up the court; the
/// master also starts the rules (and, for the self-test, its bots).
fn join_game(
    mut commands: Commands,
    active: Option<Res<DodgeballActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Dodgeball>,
) {
    if !matches!(active.as_deref(), Some(DodgeballActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return }; // not in a room yet
    let e = commands.spawn((Name::new("dodgeball-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/DodgeballGame", Transform::default());
    db.game = Some(e);

    if master {
        let mut rules = DodgeballRules::new(DodgeballConfig::default());
        rules.add_player(pid(actor));
        db.roster.push(actor);
        db.rules = Some(rules);
        // the real balls on the centre line (master owns them until a player grabs one; grab/throw via interact)
        for i in 0..BALL_COUNT {
            let x = (i as f32 - (BALL_COUNT as f32 - 1.0) * 0.5) * 1.1;
            net.net_spawn(BALL_ADDRESS, Transform::from_xyz(x, 0.6, 0.0));
        }
        db.court_set = true;
        info!("dodgeball: this client is the authority (actor {actor}); {BALL_COUNT} balls on the centre line");
        match std::env::var("METALLIC_DB_BOTS").ok().as_deref() {
            Some("1") => {
                // single-client self-test: roster becomes [actor(t0), 9001(t1), 9101(t0)]; 9101 (t0) hits 9001 (t1's
                // only player) -> team 1 wiped -> team 0 scores. The request is raised after the 3s pre-round.
                db.roster.push(9001);
                db.roster.push(9101);
                if let Some(r) = db.rules.as_mut() {
                    r.add_player(9001);
                    r.add_player(9101);
                }
                db.bots = vec![Bot { thrower: 9101, victim: 9001, at: 4.5, fired: false }];
                info!("dodgeball: self-test bots enabled");
            }
            Some("guest") => {
                db.demo_mode = true; // 2-client demo: hit the first real guest once
                info!("dodgeball: demo mode — will eliminate the first guest over the net");
            }
            _ => {}
        }
    } else {
        info!("dodgeball: this client is a guest (actor {actor}); the authority scores");
    }
}

/// The ball's owner detects the collision: for a live ball THIS client owns (grabbed/thrown), if it contacts a remote
/// player's body, raise a hit request to the master (Rec Room's `RpcMasterRequestPlayerOut`). The thrower is this
/// player; the master validates team/round/out state.
fn owner_hit_detect(
    active: Option<Res<DodgeballActive>>,
    link: Res<RoomLink>,
    mut net: NetObjects,
    db: Res<Dodgeball>,
    balls: Query<(&NetObject, &Transform)>,
    mut last_hit: Local<HashMap<i32, f32>>,
    time: Res<Time>,
) {
    if !matches!(active.as_deref(), Some(DodgeballActive(true))) {
        return;
    }
    let (Some(game), Some(rt)) = (db.game, link.get()) else { return };
    let (actor, remotes) = {
        let mut room = rt.room.lock().unwrap();
        (room.actor().unwrap_or(0), room.remotes(rt.now(), time.delta_secs_f64()))
    };
    let now = time.elapsed_secs();
    for (o, t) in &balls {
        if !o.mine || o.address != BALL_ADDRESS {
            continue; // only a ball this client owns (threw)
        }
        for s in &remotes {
            if s.actor == actor {
                continue;
            }
            let bp = t.translation;
            let pp = Vec3::from(s.body.position) + Vec3::Y; // torso
            if bp.distance(pp) < CONTACT && last_hit.get(&s.actor).is_none_or(|&t| now - t > 1.0) {
                last_hit.insert(s.actor, now);
                let bytes = enc_pair(pid(actor), pid(s.actor));
                net.send_rpc(game, RPC_HIT, bytes, RpcTarget::Master);
            }
        }
    }
}

/// Receive request RPCs (the master queues them) and outcome RPCs (guests apply them).
fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Dodgeball>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_HIT => {
                if let Some((thrower, victim)) = dec_pair(&rpc.bytes) {
                    db.pending.push(GameInput::Hit { thrower, victim });
                }
            }
            RPC_CATCH => {
                if let Some((catcher, thrower)) = dec_pair(&rpc.bytes) {
                    db.pending.push(GameInput::Catch { catcher, thrower });
                }
            }
            RPC_OUT => {
                if let Some(p) = dec_u32(&rpc.bytes) {
                    apply_out(&mut db, p);
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 5 {
                    let team = rpc.bytes[0];
                    let score = dec_u32(&rpc.bytes[1..]).unwrap_or(0);
                    if (team as usize) < 2 {
                        db.score[team as usize] = score;
                    }
                    info!("dodgeball: team {team} scores — {}:{}", db.score[0], db.score[1]);
                }
            }
            RPC_ROUND => {
                if let Some((round, phase, winner)) = dec_round(&rpc.bytes) {
                    db.round = round;
                    info!("dodgeball: round {round} {phase:?}{}", winner.map_or(String::new(), |w| format!(" (team {w})")));
                }
            }
            _ => {}
        }
    }
}

/// Master only: register joined players, apply queued requests + the round clock through the rules, then broadcast
/// the outcomes.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<Dodgeball>) {
    if db.rules.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    db.bot_elapsed += dt;

    // register real remote players who have joined (the local master + bots were added at join_game)
    let (local_actor, remote_actors): (i32, Vec<i32>) = {
        match link.get() {
            Some(rt) => {
                let mut room = rt.room.lock().unwrap();
                let local = room.actor().unwrap_or(0);
                let remotes = room.remotes(rt.now(), dt as f64).into_iter().map(|s| s.actor).filter(|&a| a > 0 && a < 9000).collect();
                (local, remotes)
            }
            None => (0, Vec::new()),
        }
    };
    for a in &remote_actors {
        if !db.roster.contains(a) {
            db.roster.push(*a);
            if let Some(r) = db.rules.as_mut() {
                r.add_player(pid(*a));
            }
            info!("dodgeball: player {a} joined (team {})", (db.roster.len() - 1) % 2);
        }
    }

    // self-test bots raise scripted hit requests
    let elapsed = db.bot_elapsed;
    let mut bot_inputs = Vec::new();
    for b in db.bots.iter_mut() {
        if !b.fired && elapsed >= b.at {
            b.fired = true;
            bot_inputs.push(GameInput::Hit { thrower: pid(b.thrower), victim: pid(b.victim) });
        }
    }
    // 2-client demo: once a real guest has joined and the round is going, the master's ball "hits" them once, so the
    // over-the-net PlayerOut + topple can be seen (METALLIC_DB_BOTS). Harmless with no guest present.
    if !db.demo_hit && db.demo_mode && db.bot_elapsed > 6.0 {
        if let Some(&guest) = remote_actors.iter().find(|&&a| a != local_actor) {
            bot_inputs.push(GameInput::Hit { thrower: pid(local_actor), victim: pid(guest) });
            db.demo_hit = true;
            info!("dodgeball: demo hit — master {local_actor} eliminates guest {guest}");
        }
    }

    let mut inputs: Vec<GameInput> = std::mem::take(&mut db.pending);
    inputs.extend(bot_inputs);

    let mut events: Vec<GameEvent> = Vec::new();
    {
        let rules = db.rules.as_mut().unwrap();
        for input in inputs {
            rules.input(input, &mut events);
        }
        rules.tick(dt, &mut events);
    }

    let game = db.game;
    for ev in events {
        // broadcast to the other clients, and apply locally (the master shows it too)
        if let Some(g) = game {
            let (method, bytes) = match &ev {
                GameEvent::PlayerOut(p) => (RPC_OUT, enc_u32(*p)),
                GameEvent::Score { team, score } => (RPC_SCORE, enc_score(*team, *score)),
                GameEvent::Round { round, phase, winner } => (RPC_ROUND, enc_round(*round, *phase, *winner)),
                _ => continue,
            };
            net.send_rpc(g, method, bytes, RpcTarget::Others);
        }
        match ev {
            GameEvent::PlayerOut(p) => apply_out(&mut db, p),
            GameEvent::PlayerRespawned(p) => db.out.retain(|&x| x != p),
            GameEvent::Score { team, score } => {
                if (team as usize) < 2 {
                    db.score[team as usize] = score;
                }
                info!("dodgeball: team {team} scores — {}:{}", db.score[0], db.score[1]);
            }
            GameEvent::Round { round, phase, winner } => {
                db.round = round;
                info!("dodgeball: round {round} {phase:?}{}", winner.map_or(String::new(), |w| format!(" (team {w})")));
                if phase == RoundPhase::GameOver {
                    info!("dodgeball: MATCH OVER — team {winner:?} wins");
                }
            }
            _ => {}
        }
    }
}

/// Every client applies the authority's PlayerOut: topple the out player's avatar (matched by its network actor), and
/// stand it back up on respawn. This is the visible half of `RpcOnPlayerOut`.
fn apply_out_visual(db: Res<Dodgeball>, mut avatars: Query<(&Rig, &mut Transform)>, mut toppled: Local<std::collections::HashSet<u32>>) {
    use std::f32::consts::FRAC_PI_2;
    for &p in &db.out {
        if toppled.insert(p) {
            for (rig, mut t) in &mut avatars {
                if rig.seed() == p {
                    t.rotation = Quat::from_rotation_x(-FRAC_PI_2) * t.rotation; // fall backward
                }
            }
        }
    }
    let out_now: std::collections::HashSet<u32> = db.out.iter().copied().collect();
    let restored: Vec<u32> = toppled.iter().copied().filter(|p| !out_now.contains(p)).collect();
    for p in restored {
        toppled.remove(&p);
        for (rig, mut t) in &mut avatars {
            if rig.seed() == p {
                t.rotation = Quat::from_rotation_x(FRAC_PI_2) * t.rotation; // stand back up
            }
        }
    }
}

fn apply_out(db: &mut Dodgeball, p: u32) {
    if !db.out.contains(&p) {
        db.out.push(p);
        info!("dodgeball: player {p} is OUT");
    }
}

// ---- compact RPC codec (no serde dep): little-endian fields ----

fn enc_u32(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}
fn dec_u32(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(0..4)?.try_into().ok()?))
}
fn enc_pair(a: u32, b: u32) -> Vec<u8> {
    let mut v = a.to_le_bytes().to_vec();
    v.extend_from_slice(&b.to_le_bytes());
    v
}
fn dec_pair(b: &[u8]) -> Option<(u32, u32)> {
    Some((u32::from_le_bytes(b.get(0..4)?.try_into().ok()?), u32::from_le_bytes(b.get(4..8)?.try_into().ok()?)))
}
fn enc_score(team: u8, score: u32) -> Vec<u8> {
    let mut v = vec![team];
    v.extend_from_slice(&score.to_le_bytes());
    v
}
fn enc_round(round: u32, phase: RoundPhase, winner: Option<u8>) -> Vec<u8> {
    let mut v = round.to_le_bytes().to_vec();
    v.push(match phase {
        RoundPhase::PreRound => 0,
        RoundPhase::Running => 1,
        RoundPhase::PostRound => 2,
        RoundPhase::GameOver => 3,
    });
    v.push(winner.unwrap_or(0xFF));
    v
}
fn dec_round(b: &[u8]) -> Option<(u32, RoundPhase, Option<u8>)> {
    let round = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?);
    let phase = match b.get(4)? {
        0 => RoundPhase::PreRound,
        1 => RoundPhase::Running,
        2 => RoundPhase::PostRound,
        _ => RoundPhase::GameOver,
    };
    let w = *b.get(5)?;
    Some((round, phase, (w != 0xFF).then_some(w)))
}
