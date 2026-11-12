//! Paddleball in-client over rr-net, master-client authority — the full vertical for game #2, reusing the Dodgeball
//! net pattern (verified 1:1, decompile lane). `--world paddleball` loads the real arena (decrypted level13); the
//! master spawns the real ball + paddle props (`(Paddleball)` ball, `(Paddleball_Paddle)` — a held physics collider,
//! no scripted force, exactly like the Dodgeball ball/throw) and runs the [`gamemodes::PaddleballMode`] rules. When a
//! ball enters a goal, the goal-trigger client raises `RpcMasterRequestScore`; the master validates (round active),
//! counts the goal for the scoring team, and broadcasts `RpcOnScore`. First team to 5 goals wins.
//!
//! Only two things differ from Dodgeball's plugin: the request is a Goal (not a Hit), and detection is a ball-in-goal
//! trigger (not ball-vs-player contact). Everything else — scene load, net_spawn props, the master-validate RPC flow,
//! the scoreboard — is the same shape. `METALLIC_PB_BOTS=1` drives the score to a win with one client, as the
//! Dodgeball self-test does. Keep the client muted (owner's standing request).

use bevy::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase, Team};
use gamemodes::paddleball::{PaddleballConfig, PaddleballMode as PaddleballRules};
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObject, NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

// 1:1: the exact Paddleball ball Addressables key isn't obvious in the catalog (it's not "(Paddleball*)" like the
// paddle is) — confirm with the decompile lane; net_spawn fails gracefully if wrong, and the self-test injects goals
// directly so it doesn't depend on the ball spawning.
const BALL_ADDRESS: &str = "(Paddleball)";
const PADDLE_ADDRESS: &str = "(Paddleball_Paddle)"; // VERIFIED (world_inventory)
const GAME_VIEW_INDEX: u32 = 47111; // distinct from Dodgeball's authority view
const RPC_GOAL: &str = "PbGoal"; // client → master: a goal scored (scoring team)
const RPC_SCORE: &str = "PbScore"; // master → others: team score
const RPC_ROUND: &str = "PbRound"; // master → others: round/match state
/// 1:1: a goal trigger spans the end of the court; the real goal AABBs + teams are PaddleballGoal objects in the
/// scene (get_Team). Approximated here from the arena bounds (team 0 defends -z, team 1 defends +z) until the scene's
/// goal objects are parsed; the scoring-team resolution (ball in a team's goal → the other team scores) is the real
/// air-hockey rule.
const GOAL_DEPTH: f32 = 3.0;

#[derive(Resource, Clone, Copy)]
pub struct PaddleballActive(pub bool);

#[derive(Resource, Default)]
pub struct Paddleball {
    rules: Option<PaddleballRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    score: [u32; 2],
    /// the two goal lines (z of the -z and +z ends) and the court centre, from the arena bounds
    goal_z: [f32; 2],
    court_set: bool,
    bots: Vec<f32>, // scheduled goal times (self-test)
    bot_team: Team,
    bot_elapsed: f32,
}

pub struct PaddleballPlugin;

impl Plugin for PaddleballPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Paddleball>().add_systems(Update, (join_game, goal_detect, recv_rpcs, master_tick).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Once the arena is built and we're in a room, register the shared game-authority object; the master sets up the
/// court (ball + paddles + goal lines) and starts the rules.
fn join_game(
    mut commands: Commands,
    active: Option<Res<PaddleballActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Paddleball>,
) {
    if !matches!(active.as_deref(), Some(PaddleballActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };
    let e = commands.spawn((Name::new("paddleball-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/PaddleballGame", Transform::default());
    db.game = Some(e);

    // goal lines at the court ends (the real PaddleballGoal AABBs; approximated from the arena z-extent ±18)
    db.goal_z = [-18.0 + GOAL_DEPTH, 18.0 - GOAL_DEPTH];

    if master {
        let mut rules = PaddleballRules::new(PaddleballConfig::default());
        rules.add_player(pid(actor));
        db.roster.push(actor);
        db.rules = Some(rules);
        // the real ball at centre, a paddle on each side (grab/throwable via interact — a held physics collider)
        net.net_spawn(BALL_ADDRESS, Transform::from_xyz(0.0, 1.0, 0.0));
        net.net_spawn(PADDLE_ADDRESS, Transform::from_xyz(0.0, 1.0, 6.0));
        net.net_spawn(PADDLE_ADDRESS, Transform::from_xyz(0.0, 1.0, -6.0));
        db.court_set = true;
        info!("paddleball: this client is the authority (actor {actor}); ball + paddles spawned, goals at z={:?}", db.goal_z);
        if std::env::var("METALLIC_PB_BOTS").is_ok() {
            // self-test: team 0 scores 5 goals to a win. Space them >2s apart so each lands while Playing, not during
            // the 2s serve pause (a goal during the pause doesn't count — matches the real rule).
            db.bots = vec![2.5, 5.3, 8.1, 10.9, 13.7];
            db.bot_team = 0;
            info!("paddleball: self-test — team 0 drives to 5 goals");
        }
    } else {
        info!("paddleball: this client is a guest (actor {actor}); the authority scores");
    }
}

/// The ball's owner detects a goal: for a live ball THIS client owns, if it crosses into a goal line, raise a score
/// request to the master. Team 0 defends -z, team 1 defends +z; a ball in a team's goal scores for the OTHER team.
fn goal_detect(
    active: Option<Res<PaddleballActive>>,
    link: Res<RoomLink>,
    mut net: NetObjects,
    db: Res<Paddleball>,
    balls: Query<(&NetObject, &Transform)>,
    mut cooldown: Local<f32>,
    time: Res<Time>,
) {
    if !matches!(active.as_deref(), Some(PaddleballActive(true))) {
        return;
    }
    *cooldown -= time.delta_secs();
    let (Some(game), Some(_rt)) = (db.game, link.get()) else { return };
    if *cooldown > 0.0 {
        return;
    }
    for (o, t) in &balls {
        if !o.mine || o.address != BALL_ADDRESS {
            continue;
        }
        let z = t.translation.z;
        let scoring = if z < db.goal_z[0] {
            Some(1) // ball in team 0's goal (-z) -> team 1 scores
        } else if z > db.goal_z[1] {
            Some(0) // ball in team 1's goal (+z) -> team 0 scores
        } else {
            None
        };
        if let Some(team) = scoring {
            *cooldown = 2.5; // one request per goal (matches the serve pause)
            net.send_rpc(game, RPC_GOAL, vec![team], RpcTarget::Master);
        }
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Paddleball>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_GOAL => {
                if let Some(&team) = rpc.bytes.first() {
                    db.pending.push(GameInput::Goal { team });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 5 {
                    let team = rpc.bytes[0];
                    let score = u32::from_le_bytes(rpc.bytes[1..5].try_into().unwrap_or_default());
                    if (team as usize) < 2 {
                        db.score[team as usize] = score;
                    }
                    info!("paddleball: team {team} GOAL — {}:{}", db.score[0], db.score[1]);
                }
            }
            RPC_ROUND => {
                if rpc.bytes.len() >= 6 {
                    let round = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let w = rpc.bytes[5];
                    info!("paddleball: round {round} state{}", if w != 0xFF { format!(" — team {w} WINS") } else { String::new() });
                }
            }
            _ => {}
        }
    }
}

/// Master only: apply queued goal requests + the serve-pause clock through the rules, then broadcast outcomes.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<Paddleball>) {
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
            info!("paddleball: player {a} joined");
        }
    }

    // self-test scripted goals
    let elapsed = db.bot_elapsed;
    let bot_team = db.bot_team;
    let mut fired = Vec::new();
    db.bots.retain(|&at| {
        if elapsed >= at {
            fired.push(GameInput::Goal { team: bot_team });
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
                GameEvent::Score { team, score } => {
                    let mut v = vec![*team];
                    v.extend_from_slice(&score.to_le_bytes());
                    (RPC_SCORE, v)
                }
                GameEvent::Round { round, phase, winner } => {
                    let mut v = round.to_le_bytes().to_vec();
                    v.push(match phase {
                        RoundPhase::PreRound => 0,
                        RoundPhase::Running => 1,
                        RoundPhase::PostRound => 2,
                        RoundPhase::GameOver => 3,
                    });
                    v.push(winner.unwrap_or(0xFF));
                    (RPC_ROUND, v)
                }
                _ => continue,
            };
            net.send_rpc(g, method, bytes, RpcTarget::Others);
        }
        match ev {
            GameEvent::Score { team, score } => {
                if (team as usize) < 2 {
                    db.score[team as usize] = score;
                }
                info!("paddleball: team {team} GOAL — {}:{}", db.score[0], db.score[1]);
            }
            GameEvent::Round { round, phase, winner } => {
                info!("paddleball: round {round} {phase:?}{}", winner.map_or(String::new(), |w| format!(" — team {w} WINS the match")));
            }
            _ => {}
        }
    }
}
