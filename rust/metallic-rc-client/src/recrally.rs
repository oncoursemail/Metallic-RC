//! RecRally kart race in-client over rr-net, master-tracked — the KART vertical (game #9) on the kart physics
//! foundation ([`crate::kart`]). `--world recrally` (level107 RecRally_Chaparral) runs the real
//! [`gamemodes::recrally::RecRallyMode`]: a finish-ORDER placement race, no points, no time score. Karts
//! ([`GroundVehicle`](crate::kart)) drive a circuit of ordered checkpoints; passing all checkpoints in order completes
//! a lap, and completing `totalLaps` = 3 raises [`GameInput::FinishRace`] → the master assigns the finish PLACE (first
//! team home = place 1 = winner, the rest ranked by finish order).
//!
//! WHAT'S REAL vs PENDING: the finish-order RULES + totalLaps=3 are body-verified (sweet-galileo_recrally-recon). The
//! kart PHYSICS tuning (mass/accel/max speed/steer/grip) is the only pending piece — the real `GroundVehicle`
//! serialized fields (requested from the decompile lane); it's isolated in [`crate::kart::KartTuning`], flagged
//! `// 1:1:`. The self-test (`METALLIC_RR_PHYSICS`) drives bot karts round a synthetic checkpoint loop to the finish
//! (the real level107 LapColliders are runtime-found, not statically readable — real-track checkpoint reads are a
//! follow-up). Keep the client muted (owner's standing request): run with `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase};
use gamemodes::recrally::{RecRallyConfig, RecRallyMode as RallyRules};
use rr_net::objects::RpcTarget;

use crate::kart::{spawn_kart, Kart};
use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const GAME_VIEW_INDEX: u32 = 47121; // distinct authority view (bowling 47117 … arena 47120)
const RPC_FINISH: &str = "RrFinish"; // guest → master: a team finished the race (team)
const RPC_PLACE: &str = "RrPlace"; // master → others: a team's finish place (team, place)
const RPC_WIN: &str = "RrWin"; // master → others: the winner team (place 1)

const KART_MASS: f32 = 150.0; // 1:1: pending GroundVehicle Rigidbody mass (requested)
const CP_RADIUS: f32 = 6.0; // a kart within this of the next checkpoint has passed it
const TOTAL_LAPS: u32 = 3; // VERIFIED (level107 RecRally_LapManager)
const TEST_FLOOR_Y: f32 = 50.0; // self-test loop runs on a flat ground in clear space above the room

#[derive(Resource, Clone, Copy)]
pub struct RecRallyActive(pub bool);

/// Per-kart race progress (master-tracked): its team, the next checkpoint it must reach, its lap count, and whether it
/// has finished.
#[derive(Component)]
struct Racer {
    team: u8,
    next_cp: usize,
    lap: u32,
    finished: bool,
}

#[derive(Resource, Default)]
pub struct RecRally {
    rules: Option<RallyRules>,
    game: Option<Entity>,
    /// ordered checkpoint positions (the lap circuit); crossing checkpoint 0 again completes a lap
    checkpoints: Vec<Vec3>,
    /// teams that have completed the race, awaiting placement through the scorer
    pending: Vec<u8>,
    /// number of karts in the race (so we end only once every kart has placed, not on the first finisher)
    kart_count: u32,
    placed: u32,
    self_test: bool,
    over: bool,
}

pub struct RecRallyPlugin;

impl Plugin for RecRallyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RecRally>().add_systems(Update, (join_game, recv_rpcs, kart_ai, lap_tracking, master_tick).chain());
    }
}

/// A large rectangular loop with edge midpoints (8 checkpoints) at `floor_y` — the synthetic self-test circuit.
fn test_loop(floor_y: f32) -> Vec<Vec3> {
    let y = floor_y + 0.3; // kart body centre height
    [(0.0, 0.0), (0.0, 30.0), (0.0, 60.0), (20.0, 60.0), (40.0, 60.0), (40.0, 30.0), (40.0, 0.0), (20.0, 0.0)]
        .iter()
        .map(|&(x, z)| Vec3::new(x, y, z))
        .collect()
}

fn join_game(
    mut commands: Commands,
    active: Option<Res<RecRallyActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<RecRally>,
) {
    if !matches!(active.as_deref(), Some(RecRallyActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(_actor) = actor else { return };

    let e = commands.spawn((Name::new("recrally-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/RecRallyGame", Transform::default());
    db.game = Some(e);

    if master {
        db.rules = Some(RallyRules::new(RecRallyConfig { total_laps: TOTAL_LAPS, game_duration: 300.0 }));
        info!("recrally: authority — finish-order race, {TOTAL_LAPS} laps (first team home = place 1)");

        if std::env::var("METALLIC_RR_PHYSICS").is_ok() {
            // self-test: a flat ground + a synthetic checkpoint loop in clear space above the room, with bot karts the
            // AI drives to the finish (real level107 checkpoints are runtime-found — a follow-up).
            db.checkpoints = test_loop(TEST_FLOOR_Y);
            commands.spawn((RigidBody::Fixed, Collider::cuboid(200.0, 0.1, 200.0), Transform::from_xyz(20.0, TEST_FLOOR_Y - 0.1, 30.0)));
            let karts = 3u32;
            for i in 0..karts {
                if let Some(r) = db.rules.as_mut() {
                    r.add_player(i); // each kart is a team (join order → team)
                }
                // single-file behind checkpoint 0, facing +Z (identity → kart forward is +Z), staggered so the front
                // kart leads and finishes first (deterministic order).
                let e = spawn_kart(&mut commands, Transform::from_xyz(0.0, TEST_FLOOR_Y + 0.3, -3.0 - i as f32 * 4.0), KART_MASS);
                commands.entity(e).insert(Racer { team: i as u8, next_cp: 0, lap: 0, finished: false });
            }
            db.kart_count = karts;
            db.self_test = true;
            info!("recrally: self-test — {karts} bot karts racing the synthetic loop to the finish-order podium");
        }
    } else {
        info!("recrally: guest; the master tracks lap progress + finish order");
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<RecRally>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_FINISH => {
                if !rpc.bytes.is_empty() {
                    db.pending.push(rpc.bytes[0]);
                }
            }
            RPC_PLACE => {
                if rpc.bytes.len() >= 5 {
                    let team = rpc.bytes[0];
                    let place = u32::from_le_bytes(rpc.bytes[1..5].try_into().unwrap_or_default());
                    info!("recrally: team {team} finished — place {place}");
                }
            }
            RPC_WIN => {
                if !rpc.bytes.is_empty() {
                    info!("recrally: RACE OVER — team {} WINS (1st)", rpc.bytes[0]);
                }
            }
            _ => {}
        }
    }
}

/// Pursuit AI (self-test bot karts): full throttle, steer toward the next checkpoint (steer>0 = right, so a left
/// target needs steer<0).
fn kart_ai(db: Res<RecRally>, mut q: Query<(&Racer, &Transform, &mut Kart)>) {
    if !db.self_test || db.checkpoints.is_empty() {
        return;
    }
    for (r, tf, mut k) in &mut q {
        if r.finished {
            k.throttle = 0.0;
            k.steer = 0.0;
            continue;
        }
        let target = db.checkpoints[r.next_cp];
        let pos = tf.translation;
        let to = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z).normalize_or_zero();
        let f = tf.rotation * Vec3::Z;
        let fwd = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        if to == Vec3::ZERO || fwd == Vec3::ZERO {
            continue;
        }
        let cross_y = fwd.z * to.x - fwd.x * to.z; // > 0 → target to the left
        k.steer = (-cross_y).clamp(-1.0, 1.0);
        k.throttle = 1.0;
    }
}

/// Master tracks each kart's ordered-checkpoint progress; completing `TOTAL_LAPS` queues the team's finish.
fn lap_tracking(mut db: ResMut<RecRally>, mut q: Query<(&mut Racer, &Transform)>) {
    if db.rules.is_none() || db.checkpoints.is_empty() || db.over {
        return;
    }
    let cps = db.checkpoints.clone();
    let mut finished: Vec<u8> = Vec::new();
    for (mut r, tf) in &mut q {
        if r.finished {
            continue;
        }
        let cp = cps[r.next_cp];
        let d = Vec3::new(cp.x - tf.translation.x, 0.0, cp.z - tf.translation.z).length();
        if d < CP_RADIUS {
            r.next_cp = (r.next_cp + 1) % cps.len();
            if r.next_cp == 0 {
                r.lap += 1;
                if r.lap >= TOTAL_LAPS {
                    r.finished = true;
                    finished.push(r.team);
                }
            }
        }
    }
    db.pending.extend(finished);
}

/// Master only: drain finishers through the real scorer (finish-order places), broadcast each place + the win; tick
/// the fallback clock.
fn master_tick(time: Res<Time>, mut net: NetObjects, mut db: ResMut<RecRally>) {
    if db.rules.is_none() || db.over {
        return;
    }
    let dt = time.delta_secs().min(0.1);

    let finishers: Vec<u8> = std::mem::take(&mut db.pending);
    let mut events: Vec<GameEvent> = Vec::new();
    {
        let rules = db.rules.as_mut().unwrap();
        for team in &finishers {
            rules.input(GameInput::FinishRace { team: *team }, &mut events);
        }
        rules.tick(dt, &mut events); // fallback race clock
    }

    let game = db.game;
    // broadcast the place for each team that just finished
    let places: Vec<(u8, u32)> = {
        let rules = db.rules.as_ref().unwrap();
        finishers.iter().filter_map(|t| rules.place_of(*t).map(|p| (*t, p as u32))).collect()
    };
    let newly_placed = places.len() as u32;
    for (team, place) in places {
        if let Some(g) = game {
            let mut v = vec![team];
            v.extend_from_slice(&place.to_le_bytes());
            net.send_rpc(g, RPC_PLACE, v, RpcTarget::Others);
        }
        info!("recrally: team {team} finished — place {place}");
    }
    db.placed += newly_placed;
    // the winner (place 1) is broadcast on the first finisher, but the race continues ranking the rest
    for ev in events {
        if let GameEvent::Round { phase: RoundPhase::GameOver, winner: Some(w), .. } = ev {
            if let Some(g) = game {
                net.send_rpc(g, RPC_WIN, vec![w], RpcTarget::Others);
            }
            info!("recrally: team {w} WINS (1st place)");
        }
    }
    // the race is fully over once every kart has placed (or the fallback clock ended it — winner None path)
    if db.kart_count > 0 && db.placed >= db.kart_count {
        db.over = true;
        info!("recrally: RACE OVER — all {} karts placed", db.kart_count);
    }
}
