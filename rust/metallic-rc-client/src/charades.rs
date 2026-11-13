//! Charades in-client over rr-net, master-authoritative — the vertical for game #6, reusing the team/round broadcast
//! shape with the real [`gamemodes::CharadesMode`]. `--world charades` loads level8; the master reads the real
//! Charades gadgets from the scene (the guess button + podium), runs the scoreboard + performer rotation + timers, and
//! broadcasts each score change, the round state, and who is performing. A guesser presses the real
//! `CharadesPhysicalButton` (guess button) to signal a correct guess → `RpcMasterRequestGuess`; the master validates
//! (guesser != performer), scores +1 (shared in CoOp / the guessing team in TeamLightning), and advances the performer.
//!
//! `METALLIC_CH_BOTS=1` self-tests the scoring over the net. Charades needs a performer AND a separate guesser, so the
//! self-test adds a couple of bot players to the mode (the lone real client would always be the performer and can't
//! guess its own word) and injects guesses from a non-performer. `METALLIC_CH_TEAM=1` runs TeamLightning instead of
//! CoOp. The pen DRAWING (3D networked strokes via the CharadesPen) is presentation depth, not a scoring dependency —
//! it is not built here. Keep the client muted (owner's standing request) — run self-tests with `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use gamemodes::charades::{CharadesConfig, CharadesMode as CharadesRules};
use gamemodes::mode::{GameEvent, GameInput, GameMode, PlayerId, RoundPhase};
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const GUESS_BUTTON_CLASS: &str = "CharadesPhysicalButton"; // the real guess button (press → correct-guess signal)
const PODIUM_CLASS: &str = "CharadesPodium"; // the performer stands (presentation; part of the real-config check)
const ROOM: &str = "charades"; // level8
const GAME_VIEW_INDEX: u32 = 47119; // distinct authority view
const RPC_GUESS: &str = "ChGuess"; // client → master: a guess-button press (guesser)
const RPC_SCORE: &str = "ChScore"; // master → others: a team/pool score
const RPC_ROUND: &str = "ChRound"; // master → others: round/match state
const RPC_PERFORMER: &str = "ChPerf"; // master → others: who is performing now

#[derive(Resource, Clone, Copy)]
pub struct CharadesActive(pub bool);

#[derive(Resource, Default)]
pub struct Charades {
    rules: Option<CharadesRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    score: [u32; 2],
    performer: Option<PlayerId>,
    guess_buttons: Vec<Vec3>, // real CharadesPhysicalButton placements (for the hard-fail check + real guess trigger)
    bots: Vec<f32>,           // scheduled guess times (self-test)
    bot_players: Vec<PlayerId>,
    bot_elapsed: f32,
}

pub struct CharadesPlugin;

impl Plugin for CharadesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Charades>().add_systems(Update, (join_game, recv_rpcs, master_tick).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Once level8 is built and we're in a room, register the authority object; the master reads the real Charades
/// gadgets (refusing to run if the guess buttons can't be read — no approximation) and starts the scoreboard.
fn join_game(
    mut commands: Commands,
    active: Option<Res<CharadesActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Charades>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(active.as_deref(), Some(CharadesActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };

    // read the REAL Charades gadgets from level8 by class. The guess button is the gameplay gadget; refuse to run if
    // it can't be read from the real scene (no approximation).
    db.guess_buttons = crate::levels::scene_class_positions(None, ROOM, GUESS_BUTTON_CLASS);
    let podiums = crate::levels::scene_class_positions(None, ROOM, PODIUM_CLASS);
    if db.guess_buttons.is_empty() {
        error!("charades: could not read any real {GUESS_BUTTON_CLASS} from level8 — refusing to run (no approximation)");
        exit.write(AppExit::error());
        return;
    }
    info!("charades: read {} real guess button(s) + {} podium(s) from level8", db.guess_buttons.len(), podiums.len());

    let e = commands.spawn((Name::new("charades-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/CharadesGame", Transform::default());
    db.game = Some(e);

    if master {
        let team_mode = std::env::var("METALLIC_CH_TEAM").is_ok();
        let self_test = std::env::var("METALLIC_CH_BOTS").is_ok();
        let mut cfg = if team_mode { CharadesConfig::team() } else { CharadesConfig::coop() };
        if self_test {
            // shorten the clocks so the self-test reaches a result quickly (real: CoOp 480 s, Team 4×90 s)
            cfg.game_duration = 14.0; // CoOp
            cfg.team_round_duration = 4.0; // Team
            cfg.performer_turn = 3.0;
        }
        let mut rules = CharadesRules::new(cfg);
        rules.add_player(pid(actor));
        db.roster.push(actor);
        if self_test {
            // Charades scores only when a non-performer guesses, so add bot players to the mode (one real client is
            // always the performer and can't guess its own word). Two bots → a performer + a guesser always exist.
            db.bot_players = vec![90001, 90002];
            for &b in &db.bot_players {
                rules.add_player(b);
            }
            // a guess every ~1.2 s; each is injected in master_tick from a current non-performer
            db.bots = (0..12).map(|i| 1.0 + i as f32 * 1.2).collect();
            info!("charades: self-test — {} mode; bots guess to a result", if team_mode { "TeamLightning" } else { "CoOp" });
        }
        db.performer = rules.performer();
        db.rules = Some(rules);
        info!("charades: authority (actor {actor}); {} mode", if team_mode { "TeamLightning" } else { "CoOp" });
    } else {
        info!("charades: guest (actor {actor}); the master tracks the scoreboard");
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Charades>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_GUESS => {
                if rpc.bytes.len() >= 4 {
                    let guesser = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    db.pending.push(GameInput::Guess { guesser });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 5 {
                    let team = rpc.bytes[0];
                    let score = u32::from_le_bytes(rpc.bytes[1..5].try_into().unwrap_or_default());
                    if (team as usize) < 2 {
                        db.score[team as usize] = score;
                    }
                    info!("charades: score — team/pool {team}: {score}");
                }
            }
            RPC_PERFORMER => {
                if rpc.bytes.len() >= 4 {
                    let p = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    info!("charades: now performing — player {p}");
                }
            }
            RPC_ROUND => {
                if rpc.bytes.len() >= 6 {
                    let round = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let phase = rpc.bytes[4];
                    let w = rpc.bytes[5];
                    if phase == 3 {
                        info!("charades: GAME OVER{}", if w != 0xFF { format!(" — team {w} WINS") } else { " (co-op result)".into() });
                    } else {
                        info!("charades: round {round} running");
                    }
                }
            }
            _ => {}
        }
    }
}

/// Master only: drain queued guesses + the self-test bots, tick the timers, and broadcast scores / round state / the
/// performer.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<Charades>) {
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
            info!("charades: player {a} joined");
        }
    }

    // self-test: fire a guess from a current non-performer each scheduled time
    let elapsed = db.bot_elapsed;
    let performer = db.rules.as_ref().and_then(|r| r.performer());
    let mut fired = Vec::new();
    let candidates: Vec<PlayerId> = db.roster.iter().map(|&a| pid(a)).chain(db.bot_players.iter().copied()).collect();
    db.bots.retain(|&at| {
        if elapsed >= at {
            if let Some(&g) = candidates.iter().find(|&&p| Some(p) != performer) {
                fired.push(GameInput::Guess { guesser: g });
            }
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

    // broadcast a performer change
    let now_performer = db.rules.as_ref().and_then(|r| r.performer());
    if now_performer != db.performer {
        db.performer = now_performer;
        if let (Some(g), Some(p)) = (db.game, now_performer) {
            net.send_rpc(g, RPC_PERFORMER, p.to_le_bytes().to_vec(), RpcTarget::Others);
        }
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
                _ => continue, // Cue not networked here
            };
            net.send_rpc(g, method, bytes, RpcTarget::Others);
        }
        match ev {
            GameEvent::Score { team, score } => {
                if (team as usize) < 2 {
                    db.score[team as usize] = score;
                }
                info!("charades: score — team/pool {team}: {score}");
            }
            GameEvent::Round { round, phase, winner } => {
                if matches!(phase, RoundPhase::GameOver) {
                    info!(
                        "charades: GAME OVER{} (scores {}:{})",
                        winner.map_or(" — co-op result".to_string(), |w| format!(" — team {w} WINS")),
                        db.score[0],
                        db.score[1]
                    );
                } else {
                    info!("charades: round {round} running");
                }
            }
            _ => {}
        }
    }
}
