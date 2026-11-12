//! Soccer in-client over rr-net, master-client authority — the full vertical for game #4, reusing the Paddleball
//! team-goals net pattern. `--world soccer` loads the real arena (decrypted level72); the master spawns the real
//! ball and runs the [`gamemodes::SoccerMode`] rules. When the ball enters a goal, the goal-trigger client raises
//! `RpcMasterRequestScore`; the master validates (round active), counts the goal for the opposing team, and
//! broadcasts `RpcOnScore`. First team to `GoalsToWin` wins.
//!
//! This is the Paddleball plugin with two differences: there is no paddle (Soccer's ball is driven by the player's
//! own body/feet collider — physics-authentic, no scripted kick force, same principle as the Dodgeball throw), and
//! Soccer has powerups. `METALLIC_SOC_BOTS=1` drives the score to a win with one client. Keep the client muted
//! (owner's standing request) — run self-tests with `METALLIC_NO_MIC=1`.
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
const GAME_VIEW_INDEX: u32 = 47113; // distinct from Dodgeball/Paddleball/DiscGolf authority views
const RPC_GOAL: &str = "SocGoal"; // client → master: a goal scored (scoring team)
const RPC_SCORE: &str = "SocScore"; // master → others: team score
const RPC_ROUND: &str = "SocRound"; // master → others: round/match state
/// 1:1: the real goal AABBs + teams are `SoccerGoal` objects in level72 — read them by class via MonoFields
/// (SoccerGoal fields IN ORDER: `goalCollider: BoxCollider` PPtr FIRST @0x40, THEN `team: int` @0x48 — note team is
/// the SECOND field, unlike PaddleballGoal; the goal AABB is that referenced BoxCollider's m_Size/m_Center). Here the
/// goal lines are approximated from the arena bounds (team 0 defends -z, team 1 defends +z) until level72's goals are
/// parsed. The scoring-team resolution (ball in a team's goal → the OTHER team scores, so own-goals count) is the
/// real rule regardless of geometry.
const GOAL_DEPTH: f32 = 3.0;
/// 1:1: the level72 arena z-extent — approximated until the real bounds are read from the scene; the demo cam frames
/// from the loaded room's own bounds regardless, so this only affects where the goal lines sit for detection.
const ARENA_HALF_Z: f32 = 18.0;

#[derive(Resource, Clone, Copy)]
pub struct SoccerActive(pub bool);

#[derive(Resource, Default)]
pub struct Soccer {
    rules: Option<SoccerRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    score: [u32; 2],
    /// the two goal lines (z of the -z and +z ends), from the arena bounds — fallback when the real goals aren't read
    goal_z: [f32; 2],
    /// the REAL SoccerGoal trigger boxes from level72: (defending team, z-min, z-max, x-centre, x-half). A ball inside
    /// a goal scores for the OPPONENT of its defending team (so own-goals count). Empty → use `goal_z`.
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
    let e = commands.spawn((Name::new("soccer-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/SoccerGame", Transform::default());
    db.game = Some(e);

    // goal lines at the pitch ends — fallback approximation from the arena z-extent ±18
    db.goal_z = [-ARENA_HALF_Z + GOAL_DEPTH, ARENA_HALF_Z - GOAL_DEPTH];
    // the REAL SoccerGoal trigger boxes + teams from level72 (class-resolved; goalCollider PPtr → BoxCollider AABB,
    // then team). The scene file + collider live in the corpus, so no game-data dir is needed.
    let goals = crate::levels::scene_goals(None, "soccer", "SoccerGoal", true);
    db.real_goals = goals
        .iter()
        .map(|g| (g.team as Team, g.center.z - g.half.z, g.center.z + g.half.z, g.center.x, g.half.x.abs().max(1.0)))
        .collect();
    if !db.real_goals.is_empty() {
        info!("soccer: read {} real SoccerGoal(s) from level72: {:?}", db.real_goals.len(), db.real_goals);
    } else {
        info!("soccer: no real goals read — using the ±{ARENA_HALF_Z} arena approximation");
    }

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
        // it; 1:1: the locomotion-attack force range [minLocomotionAttackForce, maxLocomotionAttackForce] is on the
        // SoccerBall prefab — pull from level72/the ball prefab when wiring the kick force)
        net.net_spawn(BALL_ADDRESS, Transform::from_xyz(0.0, 1.0, 0.0));
        db.court_set = true;
        info!("soccer: this client is the authority (actor {actor}); 180s timed match, ball spawned, goals at z={:?}", db.goal_z);
        if self_test {
            // self-test: team 0 scores twice, team 1 once; the clock expires at 12 s and team 0 wins 2–1. Space goals
            // >kickoff_pause apart so each lands while Playing (a goal during the kickoff pause doesn't count).
            db.bots = vec![(2.5, 0), (5.3, 0), (8.1, 1)];
            info!("soccer: self-test — team 0 to 2, team 1 to 1, whistle at 12s → team 0 wins");
        }
    } else {
        info!("soccer: this client is a guest (actor {actor}); the authority scores");
    }
}

/// The ball's owner detects a goal: for a live ball THIS client owns, if it crosses into a goal line, raise a score
/// request to the master. Team 0 defends -z, team 1 defends +z; a ball in a team's goal scores for the OTHER team.
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
        let scoring = if !db.real_goals.is_empty() {
            // real goals: a ball inside a goal's trigger box scores for the OPPONENT of its defending team
            db.real_goals.iter().find_map(|&(defender, zmin, zmax, xc, xhalf)| {
                ((zmin..=zmax).contains(&p.z) && (p.x - xc).abs() <= xhalf).then_some(1 - defender)
            })
        } else if p.z < db.goal_z[0] {
            Some(1) // ball in team 0's goal (-z) -> team 1 scores
        } else if p.z > db.goal_z[1] {
            Some(0) // ball in team 1's goal (+z) -> team 0 scores
        } else {
            None
        };
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
