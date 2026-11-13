//! Bowling in-client over rr-net, master-tracked — the vertical for game #5, reusing the DiscGolf per-player shape
//! with the REAL 10-pin scoreboard ([`gamemodes::BowlingMode`]). `--world bowling` loads the real level91
//! (BowlingAlley); the master reads the real `BowlingPin` placements from the scene, spawns the real `(BowlingBall)`,
//! tracks the authoritative scoreboard, and broadcasts each player's running total. First player to finish all 10
//! frames with the highest total wins.
//!
//! PHYSICS-FREE PARTS (here, to the 1:1 bar): real scene config (pin placements, hard-fail if missing), the real
//! `BowlingBall` roll-tuning constants, the master-tracked net/scoring, and a bot self-test. PINFALL is real rigidbody
//! PHYSICS — a bowl knocks pins over, a pin is STANDING while `AngleFromUpright <= 25.0°` (VERIFIED), and the bowl's
//! pin count is (standing before − standing after the pins settle). That toppling is dynamic rigidbody dynamics the
//! rebuild's custom physics doesn't do yet (the explicit dependency — a reusable Rapier-based dynamic-prop foundation
//! is the planned home for it). Until then the self-test injects bowls directly, exactly as the other games' bot
//! tests inject their inputs; the `pinfall_from_physics` helper below is the real detection path, ready to switch on
//! once pins topple. Keep the client muted (owner's standing request) — run self-tests with `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use gamemodes::bowling::{BowlingConfig, BowlingMode as BowlingRules};
use gamemodes::mode::{GameEvent, GameInput, GameMode};
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const BALL_ADDRESS: &str = "(BowlingBall)"; // VERIFIED (world_inventory): BowlingBall script + rigidbody, grabbable
const PIN_CLASS: &str = "BowlingPin"; // the real pins are read from level91 by MonoBehaviour class
const ROOM: &str = "bowlingalley"; // level91 — resolves the scene for the class read
const GAME_VIEW_INDEX: u32 = 47117; // distinct authority view
const RPC_BOWL: &str = "BwBowl"; // client → master: a settled bowl's pinfall (player, pins)
const RPC_SCORE: &str = "BwScore"; // master → others: a player's running total
const RPC_WIN: &str = "BwWin"; // master → others: match over (winner)

/// VERIFIED (BowlingPin, const TZUDUOCEUSU / IsUpright DAT = 25.0): a pin is STANDING while its tilt from vertical
/// (`AngleFromUpright`, degrees) is `<= 25.0`, and KNOCKED (counts as down) when the tilt exceeds it.
const PIN_UPRIGHT_MAX_DEG: f32 = 25.0;

/// VERIFIED (BowlingBall prefab) roll-tuning model — the only numeric serialized fields on BowlingBall : Ball. Wire
/// these into the ball's rolling physics when the dynamic-prop solver lands; recorded here so no value is invented.
const BALL_ANGULAR_VELOCITY_THROW_MULTIPLIER: f32 = 1.5;
const BALL_LERP_FRICTION_SPEED: f32 = 0.75;
const BALL_INERTIA_MULTIPLIER: f32 = 2.0;
const BALL_Y_UP_VELOCITY_SCALAR: f32 = 0.5;
const BALL_MAX_DEPENETRATION_VELOCITY: f32 = 1.0;

#[derive(Resource, Clone, Copy)]
pub struct BowlingActive(pub bool);

#[derive(Resource, Default)]
pub struct Bowling {
    rules: Option<BowlingRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    /// the real BowlingPin world positions from level91 (for the pinfall grid + hard-fail check)
    pins: Vec<Vec3>,
    scores: std::collections::HashMap<u32, i32>,
    bots: Vec<(f32, u32, u8)>, // scheduled (time, player, pins) bowls (self-test)
    bot_elapsed: f32,
}

pub struct BowlingPlugin;

impl Plugin for BowlingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bowling>().add_systems(Update, (join_game, recv_rpcs, master_tick).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// A pin is standing while its tilt from vertical is within the real threshold. `tilt_deg` is the pin's
/// `AngleFromUpright`; feed it from the pin rigidbody's up-axis once pins topple. This is the real IsUpright rule.
#[inline]
fn pin_is_standing(tilt_deg: f32) -> bool {
    tilt_deg <= PIN_UPRIGHT_MAX_DEG
}

/// The real pinfall detection: a bowl's pin count = (standing before) − (standing after the pins settle). Ready for
/// when the dynamic-prop physics topples pins; the tilts come from each pin rigidbody's up-axis vs world up.
#[allow(dead_code)]
fn pinfall_from_physics(tilts_before: &[f32], tilts_after: &[f32]) -> u8 {
    let standing = |tilts: &[f32]| tilts.iter().filter(|&&t| pin_is_standing(t)).count();
    standing(tilts_before).saturating_sub(standing(tilts_after)).min(10) as u8
}

/// Once the lane is built and we're in a room, register the shared game-authority object; the master reads the real
/// pins, spawns the ball, and starts the scoreboard. Refuses to run if the real pins can't be read (no approximation).
fn join_game(
    mut commands: Commands,
    active: Option<Res<BowlingActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Bowling>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(active.as_deref(), Some(BowlingActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };

    // read the REAL BowlingPin placements from level91 (class-resolved). A standard lane is 10 pins; level91 has
    // several lanes. Refuse to run if we can't read a full lane's worth — never approximate the lane.
    db.pins = crate::levels::scene_class_positions(None, ROOM, PIN_CLASS);
    if db.pins.len() < 10 {
        error!("bowling: could not read the real BowlingPins from level91 (got {}) — refusing to run (no approximation)", db.pins.len());
        exit.write(AppExit::error());
        return;
    }
    info!("bowling: read {} real BowlingPins from level91 ({} lane(s))", db.pins.len(), db.pins.len() / 10);

    let e = commands.spawn((Name::new("bowling-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/BowlingGame", Transform::default());
    db.game = Some(e);

    if master {
        let mut rules = BowlingRules::new(BowlingConfig::default());
        rules.add_player(pid(actor));
        db.roster.push(actor);
        db.rules = Some(rules);

        // spawn the real ball at the near (bowler) end of lane 1, derived from the real pin layout: lane-1 pins are
        // the 10 nearest the first pin's lane; the headpin is the highest-z pin, the bowler stands beyond the lowest-z
        // end. (The exact ball-return object is a follow-up scene read; this is geometric placement from real data,
        // not a gameplay value — and the self-test injects bowls directly, so it doesn't depend on the ball spawn.)
        let lane: Vec<Vec3> = db.pins.iter().copied().take(10).collect();
        let cx = lane.iter().map(|p| p.x).sum::<f32>() / lane.len() as f32;
        let min_z = lane.iter().map(|p| p.z).fold(f32::INFINITY, f32::min);
        net.net_spawn(BALL_ADDRESS, Transform::from_xyz(cx, 0.2, min_z - 2.0));
        info!(
            "bowling: authority (actor {actor}); ball at lane-1 start; roll model throwMul {BALL_ANGULAR_VELOCITY_THROW_MULTIPLIER} \
             friction {BALL_LERP_FRICTION_SPEED} inertia {BALL_INERTIA_MULTIPLIER} yUp {BALL_Y_UP_VELOCITY_SCALAR} \
             maxDepen {BALL_MAX_DEPENETRATION_VELOCITY}; pins standing while tilt<= {PIN_UPRIGHT_MAX_DEG}°"
        );

        if std::env::var("METALLIC_BW_BOTS").is_ok() {
            // self-test: the master bowls a full real card (the 167 textbook game) to completion → MatchOver. Verifies
            // the 10-pin scoreboard + the match-complete/win broadcast over the net. One bowl every 0.8 s.
            let me = pid(actor);
            let card: [u8; 17] = [10, 7, 3, 9, 0, 10, 0, 8, 8, 2, 0, 6, 10, 10, 10, 8, 1];
            db.bots = card.iter().enumerate().map(|(i, &pins)| (1.5 + i as f32 * 0.8, me, pins)).collect();
            info!("bowling: self-test — master bowls the 167 card to a complete game");
        }
    } else {
        info!("bowling: guest (actor {actor}); the master tracks the scoreboard");
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Bowling>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_BOWL => {
                if rpc.bytes.len() >= 5 {
                    let player = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let pins = rpc.bytes[4];
                    db.pending.push(GameInput::Bowl { player, pins });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 8 {
                    let player = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let score = i32::from_le_bytes(rpc.bytes[4..8].try_into().unwrap_or_default());
                    db.scores.insert(player, score);
                    info!("bowling: player {player} total {score}");
                }
            }
            RPC_WIN => {
                if rpc.bytes.len() >= 4 {
                    let winner = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    info!("bowling: GAME OVER — player {winner} WINS");
                }
            }
            _ => {}
        }
    }
}

/// Master only: drain queued bowl requests + the self-test bots through the scoreboard, then broadcast running totals
/// and the win.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<Bowling>) {
    if db.rules.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    db.bot_elapsed += dt;

    // register real remote players who joined
    let remote_actors: Vec<i32> = match link.get() {
        Some(rt) => rt.room.lock().unwrap().remotes(rt.now(), dt as f64).into_iter().map(|s| s.actor).filter(|&a| a > 0 && a < 9000).collect(),
        None => Vec::new(),
    };
    for a in &remote_actors {
        if !db.roster.contains(a) {
            db.roster.push(*a);
            if let Some(r) = db.rules.as_mut() {
                r.add_player(pid(*a));
            }
            info!("bowling: player {a} joined");
        }
    }

    // self-test scripted bowls (time, player, pins)
    let elapsed = db.bot_elapsed;
    let mut fired = Vec::new();
    db.bots.retain(|&(at, player, pins)| {
        if elapsed >= at {
            fired.push(GameInput::Bowl { player, pins });
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
                _ => continue, // Cue etc. not networked here
            };
            net.send_rpc(g, method, bytes, RpcTarget::Others);
        }
        match ev {
            GameEvent::PlayerScore { player, score } => {
                db.scores.insert(player, score);
                info!("bowling: player {player} total {score}");
            }
            GameEvent::MatchOver { winner } => {
                info!("bowling: GAME OVER — player {winner} WINS (total {})", db.scores.get(&winner).copied().unwrap_or(0));
            }
            _ => {}
        }
    }
}
