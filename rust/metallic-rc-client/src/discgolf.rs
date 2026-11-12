//! DiscGolf in-client over rr-net, master-client authority — the full vertical for game #3. `--world discgolf` loads
//! the real course (decrypted level9); the master spawns the real disc ((DiscGolfDisc)) and runs
//! [`gamemodes::DiscGolfMode`] (per-player stroke play). Reuses the Dodgeball/Paddleball net pattern: a client raises
//! a request (throw / basket / hazard), the master validates + scores and broadcasts. First difference vs the two ball
//! games: the disc is NOT plain ballistic — it has a FLIGHT MODEL (lift + throw-assist + spin), ported here as a
//! client-side prop behaviour.
//!
//! Scoring is per-player (strokes relative to par); the course's per-hole par comes from the scene's DiscGolfGoal
//! objects. `METALLIC_DG_BOTS=1` plays the master through a short course to a win, as the other self-tests do. Muted;
//! every run uses METALLIC_NO_MIC=1.

use std::collections::HashMap;

use bevy::prelude::*;
use gamemodes::discgolf::{DiscGolfConfig, DiscGolfMode as DiscGolfRules};
use gamemodes::mode::{GameEvent, GameInput, GameMode};
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObject, NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const DISC_ADDRESS: &str = "(DiscGolfDisc)";
const GAME_VIEW_INDEX: u32 = 47112;
const RPC_THROW: &str = "DgThrow";
const RPC_BASKET: &str = "DgBasket";
const RPC_HAZARD: &str = "DgHazard";
const RPC_SCORE: &str = "DgScore"; // master → others: a player's running total (i32)
const RPC_WIN: &str = "DgWin"; // master → others: the winning player

/// The disc's FLIGHT MODEL (VERIFIED from DiscGolfDisc config, decompile lane): it glides. Per-frame while a thrown
/// disc is live, add lift against gravity scaled by the throw-assist curve (1.0 → 0.75 over ~1s), and carry spin.
/// Throw speed itself is the hand release velocity (physics-authentic), same as the balls.
const AIR_LIFT: f32 = 0.8; // airLiftModifier: fraction of gravity countered by lift at full assist
const ASSIST_TIME: f32 = 1.0; // seconds over which ThrowAssistCurve falls 1.0 → 0.75
const ASSIST_END: f32 = 0.75; // ThrowAssistCurve end value
const GRAVITY: f32 = 9.81;

#[derive(Resource, Clone, Copy)]
pub struct DiscGolfActive(pub bool);

/// A thrown disc in flight (the owner augments its physics with the flight model).
#[derive(Component)]
struct Disc {
    age: f32,
}

#[derive(Resource, Default)]
pub struct DiscGolf {
    rules: Option<DiscGolfRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    /// each player's running total (strokes relative to par), as broadcast
    scores: HashMap<u32, i32>,
    winner: Option<u32>,
    court_set: bool,
    /// self-test: scripted (time, input) for the master to play a short course
    bots: Vec<(f32, GameInput)>,
    bot_elapsed: f32,
}

pub struct DiscGolfPlugin;

impl Plugin for DiscGolfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiscGolf>().add_systems(Update, (join_game, disc_flight, recv_rpcs, master_tick).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

fn join_game(
    mut commands: Commands,
    active: Option<Res<DiscGolfActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<DiscGolf>,
) {
    if !matches!(active.as_deref(), Some(DiscGolfActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };
    let e = commands.spawn((Name::new("discgolf-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/DiscGolfGame", Transform::default());
    db.game = Some(e);

    if master {
        // The REAL level9 course: 9 holes, pars [3,2,3,4,2,4,3,2,5] (total 28), read from the scene's DiscGolfGoal
        // objects via `unity_assets::level::MonoFields` and now the default (see `gamemodes::DiscGolfConfig`).
        let cfg = DiscGolfConfig::default();
        let holes = cfg.course.len();
        let mut rules = DiscGolfRules::new(cfg);
        rules.add_player(pid(actor));
        db.roster.push(actor);
        db.rules = Some(rules);
        net.net_spawn(DISC_ADDRESS, Transform::from_xyz(0.0, 1.2, 0.0)); // a disc at the first tee (grab/throwable)
        db.court_set = true;
        info!("discgolf: this client is the authority (actor {actor}); disc spawned, real {holes}-hole level9 course");
        if std::env::var("METALLIC_DG_BOTS").is_ok() {
            let me = pid(actor);
            // play the master through the full real course: each hole in one throw (an ace), so each scores par−1 and
            // the course completes after the last basket. Throw then basket, spaced per hole.
            let mut bots = Vec::new();
            for i in 0..holes {
                let t = 2.0 + i as f32 * 0.6;
                bots.push((t, GameInput::Throw { player: me }));
                bots.push((t + 0.3, GameInput::Basket { player: me }));
            }
            db.bots = bots;
            info!("discgolf: self-test — master plays the real {holes}-hole course");
        }
    } else {
        info!("discgolf: this client is a guest (actor {actor}); the authority scores");
    }
}

/// The disc's flight model: for a thrown disc THIS client owns, counter part of gravity (lift) scaled by the
/// throw-assist curve, so it glides like a frisbee instead of dropping ballistically. (Spin is visual; the lift is
/// what changes the path.) Clients that don't own the disc get its synced transform as usual.
fn disc_flight(
    time: Res<Time>,
    active: Option<Res<DiscGolfActive>>,
    mut commands: Commands,
    mut discs: Query<(Entity, &NetObject, &mut Transform, Option<&mut Disc>)>,
) {
    if !matches!(active.as_deref(), Some(DiscGolfActive(true))) {
        return;
    }
    let dt = time.delta_secs();
    for (e, o, mut t, disc) in &mut discs {
        if !o.mine || o.address != DISC_ADDRESS {
            continue;
        }
        // tag a disc as in-flight once (for the assist timer)
        let age = match disc {
            Some(mut d) => {
                d.age += dt;
                d.age
            }
            None => {
                commands.entity(e).insert(Disc { age: 0.0 });
                0.0
            }
        };
        // ThrowAssistCurve: full lift early (1.0), easing to ASSIST_END over ASSIST_TIME
        let assist = 1.0 - (1.0 - ASSIST_END) * (age / ASSIST_TIME).min(1.0);
        // lift = assist * AIR_LIFT * gravity, applied up over the frame (nudges the transform; the prop sim adds the
        // ballistic motion). This is the glide.
        t.translation.y += AIR_LIFT * assist * GRAVITY * dt * dt * 0.5;
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<DiscGolf>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_THROW => {
                if let Some(p) = dec_u32(&rpc.bytes) {
                    db.pending.push(GameInput::Throw { player: p });
                }
            }
            RPC_BASKET => {
                if let Some(p) = dec_u32(&rpc.bytes) {
                    db.pending.push(GameInput::Basket { player: p });
                }
            }
            RPC_HAZARD => {
                if let Some(p) = dec_u32(&rpc.bytes) {
                    db.pending.push(GameInput::Hazard { player: p });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 8 {
                    let player = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let score = i32::from_le_bytes(rpc.bytes[4..8].try_into().unwrap_or_default());
                    db.scores.insert(player, score);
                    info!("discgolf: player {player} now {score:+} (relative to par)");
                }
            }
            RPC_WIN => {
                if let Some(w) = dec_u32(&rpc.bytes) {
                    db.winner = Some(w);
                    info!("discgolf: course complete — player {w} WINS (best relative to par)");
                }
            }
            _ => {}
        }
    }
}

fn master_tick(time: Res<Time>, mut net: NetObjects, mut db: ResMut<DiscGolf>) {
    if db.rules.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    db.bot_elapsed += dt;

    // self-test scripted inputs
    let elapsed = db.bot_elapsed;
    let mut fired = Vec::new();
    db.bots.retain(|(at, input)| {
        if elapsed >= *at {
            fired.push(*input);
            false
        } else {
            true
        }
    });
    let mut inputs: Vec<GameInput> = std::mem::take(&mut db.pending);
    inputs.extend(fired);

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
        if let Some(g) = game {
            let (method, bytes) = match &ev {
                GameEvent::PlayerScore { player, score } => {
                    let mut v = player.to_le_bytes().to_vec();
                    v.extend_from_slice(&score.to_le_bytes());
                    (RPC_SCORE, v)
                }
                GameEvent::MatchOver { winner } => (RPC_WIN, winner.to_le_bytes().to_vec()),
                _ => continue,
            };
            net.send_rpc(g, method, bytes, RpcTarget::Others);
        }
        match ev {
            GameEvent::PlayerScore { player, score } => {
                db.scores.insert(player, score);
                info!("discgolf: player {player} now {score:+} (relative to par)");
            }
            GameEvent::MatchOver { winner } => {
                db.winner = Some(winner);
                info!("discgolf: course complete — player {winner} WINS (best relative to par)");
            }
            _ => {}
        }
    }
}

fn dec_u32(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(0..4)?.try_into().ok()?))
}
