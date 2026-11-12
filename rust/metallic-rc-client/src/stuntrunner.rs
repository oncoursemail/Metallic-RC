//! StuntRunner in-client over rr-net, master-authoritative — the vertical for game #7, reusing the per-player
//! (DiscGolf/Bowling) shape with the real [`gamemodes::StuntRunnerMode`] (6-course time-trial). The master reads the
//! real course gates from the scene, runs the authoritative clock + scoreboard over the 6-course sequence, and
//! broadcasts each player's running total, the current course, and the winner. A player crossing the finish gate
//! sends their run time (ms) → `RpcMasterRequestCourseFinishTime` → [`GameInput::FinishCourse`]; the master scores it.
//!
//! SCORING-COMPLETE PARTS (here, to the bar): real scene config (the StuntRunnerEndingGate finish gate read by class,
//! HARD-FAIL if missing), the master-tracked net/scoring over the 6 courses, course-state sync, and a bot self-test.
//! Two WORLD-DEPTH follow-ups, NOT scoring dependencies: (1) the real gate-crossing TIMING — measuring each player's
//! wall-clock from the StuntRunnerStartingGate to the EndingGate — the self-test injects run times directly instead;
//! (2) the multi-course scene SWAP — the real game loads level93–98 one at a time on COURSE_TRANSITION, which the
//! world loader doesn't do yet; the self-test runs the whole sequence in one loaded course scene. Keep the client
//! muted (owner's standing request) — run self-tests with `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, PlayerId};
use gamemodes::stuntrunner::{StuntRunnerConfig, StuntRunnerMode as StuntRunnerRules};
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const FINISH_GATE_CLASS: &str = "StuntRunnerEndingGate"; // the real finish gate (crossing it posts a run time)
const START_GATE_CLASS: &str = "StuntRunnerStartingGate"; // the real start line (clock start; presentation here)
const ROOM: &str = "level93"; // Course1 — the self-test loads one course; multi-course scene-swap is a follow-up
const GAME_VIEW_INDEX: u32 = 47121; // distinct authority view
const RPC_FINISH: &str = "SrFinish"; // client → master: a course finish (player, time_ms)
const RPC_SCORE: &str = "SrScore"; // master → others: a player's running total
const RPC_COURSE: &str = "SrCourse"; // master → others: the current course index
const RPC_WIN: &str = "SrWin"; // master → others: match over (winner)

#[derive(Resource, Clone, Copy)]
pub struct StuntRunnerActive(pub bool);

#[derive(Resource, Default)]
pub struct StuntRunner {
    rules: Option<StuntRunnerRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    totals: std::collections::HashMap<PlayerId, i32>,
    course: usize,
    finish_gates: Vec<Vec3>, // real StuntRunnerEndingGate placements (hard-fail check + real finish trigger)
    bots: Vec<(f32, PlayerId, u32)>, // scheduled (time, player, run_time_ms) finishes (self-test)
    bot_players: Vec<PlayerId>,
    bot_elapsed: f32,
}

pub struct StuntRunnerPlugin;

impl Plugin for StuntRunnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StuntRunner>().add_systems(Update, (join_game, recv_rpcs, master_tick).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Once the course is built and we're in a room, register the authority object; the master reads the real course
/// gates (refusing to run if the finish gate can't be read — no approximation) and starts the scoreboard.
fn join_game(
    mut commands: Commands,
    active: Option<Res<StuntRunnerActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<StuntRunner>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(active.as_deref(), Some(StuntRunnerActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };

    // read the REAL course gates by class. The finish gate is the gameplay gadget; refuse to run if it can't be read.
    db.finish_gates = crate::levels::scene_class_positions(None, ROOM, FINISH_GATE_CLASS);
    let start_gates = crate::levels::scene_class_positions(None, ROOM, START_GATE_CLASS);
    if db.finish_gates.is_empty() {
        error!("stuntrunner: could not read any real {FINISH_GATE_CLASS} from the course — refusing to run (no approximation)");
        exit.write(AppExit::error());
        return;
    }
    info!("stuntrunner: read {} real finish gate(s) + {} start gate(s) from the course", db.finish_gates.len(), start_gates.len());

    let e = commands.spawn((Name::new("stuntrunner-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/StuntRunnerGame", Transform::default());
    db.game = Some(e);

    if master {
        // the real sequence + formula + base score 180000 (StuntRunnerConfig::default); the time cap is a flagged
        // //1:1-pending placeholder (runtime-assigned, live-capture item) — it doesn't affect the winner.
        let mut rules = StuntRunnerRules::new(StuntRunnerConfig::default());
        rules.add_player(pid(actor));
        db.roster.push(actor);
        db.course = rules.course();
        if std::env::var("METALLIC_SR_BOTS").is_ok() {
            // StuntRunner needs ≥2 players to show a winner; add one bot. Over the 6 courses the real client finishes
            // faster (10 s runs) than the bot (30 s) → the real client wins. Both finish each course → sequence
            // advances. Run time is the wire value (ms); the self-test injects it directly (no gate-crossing timing).
            db.bot_players = vec![90001];
            for &b in &db.bot_players {
                rules.add_player(b);
            }
            let me = pid(actor);
            let bot = db.bot_players[0];
            let mut bots = Vec::new();
            for c in 0..6u32 {
                let t = 1.0 + c as f32 * 1.4;
                bots.push((t, me, 10_000)); // fast
                bots.push((t + 0.4, bot, 30_000)); // slow
            }
            db.bots = bots;
            info!("stuntrunner: self-test — real client (fast) vs 1 bot (slow) over the 6-course sequence");
        }
        db.rules = Some(rules);
        info!("stuntrunner: authority (actor {actor}); 6-course sequence, base score 180000");
    } else {
        info!("stuntrunner: guest (actor {actor}); the master tracks the scoreboard");
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<StuntRunner>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_FINISH => {
                if rpc.bytes.len() >= 8 {
                    let player = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let time_ms = u32::from_le_bytes(rpc.bytes[4..8].try_into().unwrap_or_default());
                    db.pending.push(GameInput::FinishCourse { player, time_ms });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 8 {
                    let player = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let score = i32::from_le_bytes(rpc.bytes[4..8].try_into().unwrap_or_default());
                    db.totals.insert(player, score);
                    info!("stuntrunner: player {player} total {score}");
                }
            }
            RPC_COURSE => {
                if rpc.bytes.len() >= 4 {
                    let course = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    info!("stuntrunner: now on course {}", course + 1);
                }
            }
            RPC_WIN => {
                if rpc.bytes.len() >= 4 {
                    let winner = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    info!("stuntrunner: FINISH — player {winner} WINS");
                }
            }
            _ => {}
        }
    }
}

/// Master only: drain queued finishes + the self-test bots through the scoreboard, tick the course clock, and
/// broadcast running totals, course changes, and the winner.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<StuntRunner>) {
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
            info!("stuntrunner: player {a} joined");
        }
    }

    // self-test scripted finishes (time, player, run_time_ms)
    let elapsed = db.bot_elapsed;
    let mut fired = Vec::new();
    db.bots.retain(|&(at, player, time_ms)| {
        if elapsed >= at {
            fired.push(GameInput::FinishCourse { player, time_ms });
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

    // broadcast a course change
    let now_course = db.rules.as_ref().map(|r| r.course()).unwrap_or(0);
    if now_course != db.course {
        db.course = now_course;
        if let Some(g) = db.game {
            net.send_rpc(g, RPC_COURSE, (now_course as u32).to_le_bytes().to_vec(), RpcTarget::Others);
        }
        info!("stuntrunner: now on course {}", now_course + 1);
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
                db.totals.insert(player, score);
                info!("stuntrunner: player {player} total {score}");
            }
            GameEvent::MatchOver { winner } => {
                info!("stuntrunner: FINISH — player {winner} WINS (total {})", db.totals.get(&winner).copied().unwrap_or(0));
            }
            _ => {}
        }
    }
}
