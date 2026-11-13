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
use bevy_rapier3d::prelude::*;
use rr_net::objects::RpcTarget;

use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::physics::tilt_from_upright_deg;
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

/// A real bowling pin as a rapier rigidbody (it topples; whether it still counts as standing is judged by its tilt).
#[derive(Component)]
struct BwPin;

/// The real bowling ball as a rapier rigidbody (bowled with a release velocity).
#[derive(Component)]
struct BwBall;

#[derive(Clone, Copy, PartialEq)]
enum BowlPhase {
    NeedRack,
    Aiming(f32),
    Settling(f32),
}

/// The physics bowling run (master only, `METALLIC_BW_PHYSICS=1`): the real pins + ball as rapier bodies bowled on a
/// physical lane. A bowl's pinfall is (standing before) − (standing after the pins settle) by the real 25° rule, fed
/// to the same `BowlingMode` scoreboard over the net — the real physics replacing the bot-injected pin counts.
#[derive(Resource)]
struct BowlingPhysics {
    active: bool,
    player: u32,
    /// the 10 lane-1 pin spawn poses (re-racked between frames)
    pin_homes: Vec<Transform>,
    ball_home: Transform,
    /// the current rack's pin entities
    pins: Vec<Entity>,
    ball: Option<Entity>,
    phase: BowlPhase,
    standing_before: u8,
    bowls_this_rack: u8,
    /// total bowls so far (a safety cap on the self-test)
    bowl_index: u8,
}

impl Default for BowlingPhysics {
    fn default() -> Self {
        BowlingPhysics {
            active: false,
            player: 0,
            pin_homes: Vec::new(),
            ball_home: Transform::IDENTITY,
            pins: Vec::new(),
            ball: None,
            phase: BowlPhase::NeedRack,
            standing_before: 0,
            bowls_this_rack: 0,
            bowl_index: 0,
        }
    }
}

// EXACT dump physics (prefabs (BowlingBall) + (SandboxBowlingPin), sweet-galileo_bowling-stunt-charades): a heavy
// sphere ball + a light compound-body pin — ball:pin mass = 8:1 (the key strike-realism correction).
const BALL_RADIUS: f32 = 0.125; // SphereCollider radius (0.25 m dia)
const BALL_MASS: f32 = 8.0; // Rigidbody mass
const BALL_FRICTION: f32 = 0.14; // PhysicMaterial "BowlingBall" dynamicFriction (frictionCombine Multiply)
const BALL_BOUNCE: f32 = 0.25; // bounciness (bounceCombine Multiply)
const PIN_MASS: f32 = 1.0; // Rigidbody mass
const PIN_ANGULAR_DAMPING: f32 = 1.0; // Rigidbody angularDrag
const PIN_FRICTION: f32 = 0.6; // PhysicMaterial "BowlingPin" dynamicFriction (frictionCombine Average)
const PIN_BOUNCE: f32 = 0.6; // bounciness (bounceCombine Maximum)
const BOWL_SPEED: f32 = 9.0; // m/s down the lane
const SETTLE_SECS: f32 = 3.5; // time for the ball to travel + the pins to settle

/// The real SandboxBowlingPin compound collider, root at the pin base (exact dump dimensions). Unity CapsuleCollider
/// height is the total cap-to-cap height, so the bevy cylinder half-height = height/2 − radius.
fn pin_collider() -> Collider {
    Collider::compound(vec![
        (Vec3::new(0.0, 0.05, 0.0), Quat::IDENTITY, Collider::cuboid(0.0275, 0.05, 0.0275)), // BoxCollider size (0.055,0.10,0.055)
        (Vec3::new(0.0, 0.19, 0.0), Quat::IDENTITY, Collider::capsule_y(0.15, 0.04)), // CapsuleCollider r0.04 h0.38 (neck)
        (Vec3::new(0.0, 0.12, 0.0), Quat::IDENTITY, Collider::capsule_y(0.024, 0.076)), // CapsuleCollider r0.076 h0.20 (belly)
    ])
}

pub struct BowlingPlugin;

impl Plugin for BowlingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bowling>()
            .init_resource::<BowlingPhysics>()
            .add_systems(Update, (join_game, recv_rpcs, master_tick, physics_bowl).chain());
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
    mut phys: ResMut<BowlingPhysics>,
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

        if std::env::var("METALLIC_BW_PHYSICS").is_ok() {
            // REAL physics: spawn the 10 lane-1 pins + ball as rapier rigidbodies on a physical lane; the solver
            // topples the pins and pinfall is counted by the 25° rule (physics_bowl), feeding the same scoreboard.
            let lane: Vec<Vec3> = db.pins.iter().copied().take(10).collect();
            // NOTE: the foundation mirrors the scene's OWN BowlingPin colliders as FIXED bodies, so spawning our
            // DYNAMIC pins at the real floor height would overlap them and eject the rack. Run the physics lane in
            // clear space ABOVE the room (same real x/z layout, lifted) so only our dynamic pins + ball + lane exist.
            // (Excluding the scene pins from the fixed mirror, to bowl on the real floor, is the follow-up.)
            let floor_y = 50.0_f32;
            let cx = lane.iter().map(|p| p.x).sum::<f32>() / lane.len() as f32;
            let min_z = lane.iter().map(|p| p.z).fold(f32::INFINITY, f32::min);
            let max_z = lane.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max);
            phys.pin_homes = lane.iter().map(|p| Transform::from_xyz(p.x, floor_y, p.z)).collect(); // compound root = pin base
            phys.ball_home = Transform::from_xyz(cx, floor_y + BALL_RADIUS, min_z - 14.0); // ~14 m run-up
            phys.player = pid(actor);
            phys.active = true;
            // a flat fixed lane under the pins + run-up — a bowling lane IS flat, so this matches the real geometry
            // and guarantees a surface even if the room floor is a mesh collider the foundation skips.
            let start_z = min_z - 14.0;
            let (lane_half_len, lane_mid_z) = ((max_z - start_z) * 0.5 + 1.0, (max_z + start_z) * 0.5);
            commands.spawn((RigidBody::Fixed, Collider::cuboid(1.5, 0.1, lane_half_len), Transform::from_xyz(cx, floor_y - 0.1, lane_mid_z)));
            info!("bowling: PHYSICS mode — real pins + ball as rapier bodies, bowling a full game on a physical lane");
        } else if std::env::var("METALLIC_BW_BOTS").is_ok() {
            // non-physics self-test: the master bowls a scripted card (the 167 textbook game) to completion → MatchOver.
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

/// Spawn the real ball as a rapier rigidbody at `home`, moving at `vel` (zero = resting at the start).
fn spawn_ball(commands: &mut Commands, home: Transform, vel: Vec3) -> Entity {
    commands
        .spawn((
            RigidBody::Dynamic,
            Collider::ball(BALL_RADIUS),
            ColliderMassProperties::Mass(BALL_MASS),
            Friction { coefficient: BALL_FRICTION, combine_rule: CoefficientCombineRule::Multiply },
            Restitution { coefficient: BALL_BOUNCE, combine_rule: CoefficientCombineRule::Multiply },
            Velocity { linear: vel, angular: Vec3::ZERO },
            home,
            BwBall,
        ))
        .id()
}

/// Master + physics mode: the bowl→settle→count state machine. Racks 10 real pins + a ball as rapier bodies, bowls
/// the ball down the lane (the solver topples pins), then counts the pins still STANDING by the 25° rule and feeds the
/// real pinfall (standing-before − standing-after) to the scoreboard — the physics replacing the bot-injected counts.
fn physics_bowl(
    mut commands: Commands,
    time: Res<Time>,
    mut phys: ResMut<BowlingPhysics>,
    mut db: ResMut<Bowling>,
    pins_q: Query<&Transform, With<BwPin>>,
) {
    if !phys.active || db.rules.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let standing = pins_q.iter().filter(|t| tilt_from_upright_deg(t.rotation) <= PIN_UPRIGHT_MAX_DEG).count() as u8;

    match phys.phase {
        BowlPhase::NeedRack => {
            // re-rack: despawn the old pins + ball, spawn a fresh upright 10-pin rack + a resting ball
            for e in std::mem::take(&mut phys.pins) {
                commands.entity(e).despawn();
            }
            if let Some(b) = phys.ball.take() {
                commands.entity(b).despawn();
            }
            let homes = phys.pin_homes.clone();
            let mut pins = Vec::with_capacity(homes.len());
            for home in homes {
                let e = commands
                    .spawn((
                        RigidBody::Dynamic,
                        pin_collider(),
                        ColliderMassProperties::Mass(PIN_MASS),
                        Damping { linear_damping: 0.0, angular_damping: PIN_ANGULAR_DAMPING },
                        Friction { coefficient: PIN_FRICTION, combine_rule: CoefficientCombineRule::Average },
                        Restitution { coefficient: PIN_BOUNCE, combine_rule: CoefficientCombineRule::Max },
                        home,
                        BwPin,
                    ))
                    .id();
                pins.push(e);
            }
            phys.pins = pins;
            let home = phys.ball_home;
            phys.ball = Some(spawn_ball(&mut commands, home, Vec3::ZERO));
            phys.bowls_this_rack = 0;
            phys.phase = BowlPhase::Aiming(1.0); // let the fresh rack settle before bowling
        }
        BowlPhase::Aiming(t) => {
            let t = t - dt;
            if t > 0.0 {
                phys.phase = BowlPhase::Aiming(t);
                return;
            }
            phys.standing_before = standing;
            // bowl: respawn the ball moving down the lane, with the line varied a little per bowl (strikes + splits)
            if let Some(b) = phys.ball.take() {
                commands.entity(b).despawn();
            }
            let line_x = 0.09 + ((phys.bowl_index % 3) as f32 - 1.0) * 0.06;
            let home = phys.ball_home;
            phys.ball = Some(spawn_ball(&mut commands, home, Vec3::new(line_x, 0.0, BOWL_SPEED)));
            phys.bowls_this_rack += 1;
            phys.bowl_index += 1;
            phys.phase = BowlPhase::Settling(SETTLE_SECS);
        }
        BowlPhase::Settling(t) => {
            let t = t - dt;
            if t > 0.0 {
                phys.phase = BowlPhase::Settling(t);
                return;
            }
            let pinfall = phys.standing_before.saturating_sub(standing);
            let player = phys.player;
            db.pending.push(GameInput::Bowl { player, pins: pinfall });
            info!("bowling: PHYSICS bowl {} — {} standing → {standing} standing, PINFALL {pinfall}", phys.bowl_index, phys.standing_before);
            if standing == 0 || phys.bowls_this_rack >= 2 {
                // frame done (strike, or both balls bowled) → re-rack, unless we've hit the safety cap
                if phys.bowl_index >= 21 {
                    phys.active = false;
                } else {
                    phys.phase = BowlPhase::NeedRack;
                }
            } else {
                // a second ball at the pins left standing: reset only the ball
                if let Some(b) = phys.ball.take() {
                    commands.entity(b).despawn();
                }
                let home = phys.ball_home;
                phys.ball = Some(spawn_ball(&mut commands, home, Vec3::ZERO));
                phys.phase = BowlPhase::Aiming(0.5);
            }
        }
    }
}
