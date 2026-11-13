//! Soccer in-client over rr-net, master-client authority — the full vertical for game #4, reusing the Paddleball
//! team-goals net pattern. `--world soccer` loads the real arena (decrypted level72); the master spawns the real
//! ball and runs the [`gamemodes::SoccerMode`] rules. When the ball enters a goal, the goal-trigger client raises
//! `RpcMasterRequestScore`; the master validates (round active), counts the goal for the opposing team, and
//! broadcasts `RpcOnScore`. First team to `GoalsToWin` wins.
//!
//! This is the Paddleball plugin with two differences: there is no paddle (Soccer's ball is driven by the player's
//! own body — thrown via the Ball base's release velocity, or kicked by running into it with the real fixed
//! locomotion-attack impulse `SOCCER_KICK_FORCE` = 20.0), and Soccer has powerups. `METALLIC_SOC_BOTS=1` drives the
//! score to a win with one client. Keep the client muted (owner's standing request) — run self-tests with
//! `METALLIC_NO_MIC=1`.
//!
//! SCAFFOLD STATUS (2026-11-12): structure is the proven Paddleball shape; values a clean `SoccerManager` body pins
//! down are marked `// 1:1:` (the ball Addressables key, the goal AABBs/geometry from level72, GoalsToWin, the
//! post-goal pause, and whether powerups affect scoring). Reconciled the moment the decompile lane returns bodies.

use bevy::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase, Team};
use gamemodes::soccer::{SoccerConfig, SoccerMode as SoccerRules};
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObject, NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

// VERIFIED (world_inventory): the real Soccer ball — (SoccerBall) carries the SoccerBall script + Tool/RigidbodyPickup
// (grabbable/throwable) + LocomotionSettings (the body-kick), a 1.4 m ball with box+sphere colliders.
const BALL_ADDRESS: &str = "(SoccerBall)";
/// VERIFIED (SoccerBall prefab): the body-kick impulse. The prefab's min/maxLocomotionAttackForce are BOTH 20.0, so
/// the shipped kick is a fixed 20.0, not a range. Wire the locomotion-attack (running into the ball) to this impulse.
#[allow(dead_code)]
const SOCCER_KICK_FORCE: f32 = 20.0;
const GAME_VIEW_INDEX: u32 = 47113; // distinct from Dodgeball/Paddleball/DiscGolf authority views
const RPC_GOAL: &str = "SocGoal"; // client → master: a goal scored (scoring team)
const RPC_SCORE: &str = "SocScore"; // master → others: team score
const RPC_ROUND: &str = "SocRound"; // master → others: round/match state
/// The goal teams + trigger AABBs are the real `SoccerGoal` objects in level72, read by class via MonoFields
/// (`goalCollider: BoxCollider` PPtr FIRST, THEN `team: int`; the AABB is that referenced BoxCollider's m_Size/
/// m_Center). The scoring-team resolution (ball in a team's goal → the OTHER team scores, so own-goals count) is the
/// real rule. There is NO geometry approximation: if the real goals can't be read, the game REFUSES TO RUN (a missing
/// real value is a loud bug to fix with real data, never a silent fallback).

#[derive(Resource, Clone, Copy)]
pub struct SoccerActive(pub bool);

#[derive(Resource, Default)]
pub struct Soccer {
    rules: Option<SoccerRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    score: [u32; 2],
    /// the REAL SoccerGoal trigger boxes from level72: (defending team, z-min, z-max, x-centre, x-half). A ball inside
    /// a goal scores for the OPPONENT of its defending team (so own-goals count). The game refuses to run if this
    /// can't be read from the real scene — never approximated.
    real_goals: Vec<(Team, f32, f32, f32, f32)>,
    court_set: bool,
    bots: Vec<(f32, Team)>, // scheduled (time, scoring team) goals (self-test)
    bot_elapsed: f32,
}

pub struct SoccerPlugin;

impl Plugin for SoccerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Soccer>().add_systems(Update, (join_game, goal_detect, recv_rpcs, master_tick).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Once the arena is built and we're in a room, register the shared game-authority object; the master sets up the
/// pitch (ball + goal lines) and starts the rules.
fn join_game(
    mut commands: Commands,
    active: Option<Res<SoccerActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Soccer>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(active.as_deref(), Some(SoccerActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };

    // Read the REAL SoccerGoal trigger boxes + teams from level72 (class-resolved: goalCollider PPtr → BoxCollider
    // AABB, then team) BEFORE setting anything up. A soccer pitch has two opposing goals; if we can't read both from
    // the real scene we REFUSE TO RUN rather than approximate — a missing real value is a bug to fix with real data.
    let goals = crate::levels::scene_goals(None, "soccer", "SoccerGoal", true);
    db.real_goals = goals
        .iter()
        .map(|g| (g.team as Team, g.center.z - g.half.z, g.center.z + g.half.z, g.center.x, g.half.x.abs().max(1.0)))
        .collect();
    if db.real_goals.len() < 2 {
        error!(
            "soccer: could not read the two real SoccerGoals from level72 (got {}) — refusing to run (no approximation)",
            db.real_goals.len()
        );
        exit.write(AppExit::error());
        return;
    }
    info!("soccer: read {} real SoccerGoal(s) from level72: {:?}", db.real_goals.len(), db.real_goals);

    let e = commands.spawn((Name::new("soccer-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/SoccerGame", Transform::default());
    db.game = Some(e);

    if master {
        // Soccer is TIMED: a 180 s match (SoccerConfig::default), most goals at the final whistle wins. For the
        // self-test, shorten the match so it reaches the whistle quickly.
        let mut cfg = SoccerConfig::default();
        let self_test = std::env::var("METALLIC_SOC_BOTS").is_ok();
        if self_test {
            cfg.match_duration = 12.0;
        }
        let mut rules = SoccerRules::new(cfg);
        rules.add_player(pid(actor));
        db.roster.push(actor);
        db.rules = Some(rules);
        // the real ball at centre (SoccerBall : Ball — grabbable/throwable physics prop, also kicked by running into
        // it). The body-kick impulse is the real SoccerBall locomotion-attack force = SOCCER_KICK_FORCE (20.0, a
        // fixed value — min == max == 20.0 on the prefab), to wire into the locomotion-attack/ball-contact system.
        net.net_spawn(BALL_ADDRESS, Transform::from_xyz(0.0, 1.0, 0.0));
        db.court_set = true;
        info!("soccer: this client is the authority (actor {actor}); 180s timed match, ball spawned, {} real goals", db.real_goals.len());
        if self_test {
            // self-test: team 0 scores twice, team 1 once; the clock expires at 12 s and team 0 wins 2–1. Goals are
            // spaced seconds apart so each lands while Playing (the event-driven kickoff clears on the next tick).
            db.bots = vec![(2.5, 0), (5.3, 0), (8.1, 1)];
            info!("soccer: self-test — team 0 to 2, team 1 to 1, whistle at 12s → team 0 wins");
        }
    } else {
        info!("soccer: this client is a guest (actor {actor}); the authority scores");
    }
}

/// The ball's owner detects a goal: for a live ball THIS client owns, if it enters one of the real level72 goal
/// trigger boxes, raise a score request to the master for the OPPONENT of that goal's defending team.
fn goal_detect(
    active: Option<Res<SoccerActive>>,
    link: Res<RoomLink>,
    mut net: NetObjects,
    db: Res<Soccer>,
    balls: Query<(&NetObject, &Transform)>,
    mut cooldown: Local<f32>,
    time: Res<Time>,
) {
    if !matches!(active.as_deref(), Some(SoccerActive(true))) {
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
        let p = t.translation;
        // a ball inside a real goal's trigger box scores for the OPPONENT of that goal's defending team (own-goals
        // count). The real goals are guaranteed present — join_game refuses to run without them.
        let scoring = db.real_goals.iter().find_map(|&(defender, zmin, zmax, xc, xhalf)| {
            ((zmin..=zmax).contains(&p.z) && (p.x - xc).abs() <= xhalf).then_some(1 - defender)
        });
        if let Some(team) = scoring {
            *cooldown = 2.5; // one request per goal (matches the post-goal pause)
            net.send_rpc(game, RPC_GOAL, vec![team], RpcTarget::Master);
        }
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Soccer>) {
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
                    info!("soccer: team {team} GOAL — {}:{}", db.score[0], db.score[1]);
                }
            }
            RPC_ROUND => {
                if rpc.bytes.len() >= 6 {
                    let round = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let phase = rpc.bytes[4];
                    let w = rpc.bytes[5];
                    let tail = if phase == 3 {
                        if w != 0xFF { format!(" — FULL TIME, team {w} WINS") } else { " — FULL TIME, DRAW".into() }
                    } else {
                        String::new()
                    };
                    info!("soccer: round {round} state{tail}");
                }
            }
            _ => {}
        }
    }
}

/// Master only: apply queued goal requests + the post-goal-pause clock through the rules, then broadcast outcomes.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<Soccer>) {
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
            info!("soccer: player {a} joined");
        }
    }

    // self-test scripted goals (time, scoring team)
    let elapsed = db.bot_elapsed;
    let mut fired = Vec::new();
    db.bots.retain(|&(at, team)| {
        if elapsed >= at {
            fired.push(GameInput::Goal { team });
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
                info!("soccer: team {team} GOAL — {}:{}", db.score[0], db.score[1]);
            }
            GameEvent::Round { round, phase, winner } => {
                let tail = if matches!(phase, RoundPhase::GameOver) {
                    winner.map_or(" — FULL TIME, DRAW".to_string(), |w| format!(" — FULL TIME, team {w} WINS the match"))
                } else {
                    String::new()
                };
                info!("soccer: round {round} {phase:?}{tail}");
            }
            _ => {}
        }
    }
}
