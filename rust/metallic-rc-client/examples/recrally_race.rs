//! Verify the RecRally KART race headless: bot karts ([`metallic_rc_client::kart`] physics) drive a checkpoint loop
//! with pursuit AI; completing 3 laps raises [`GameInput::FinishRace`] into the real [`gamemodes::recrally::RecRallyMode`],
//! which assigns finish-order PLACES (first home = place 1 = winner). Proves drive→laps→finish-order→placement, the
//! wiring the in-client plugin runs over rr-net. Kart tuning is pending the GroundVehicle dump (placeholders). Karts are
//! staggered single-file at equal speed, so the finish order is deterministic (front kart wins). Deterministic timing
//! via TimeUpdateStrategy (fixed 1/60, matching the physics step).
//!
//! `cargo run --profile fast -p metallic-rc-client --example recrally_race`
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_rapier3d::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase};
use gamemodes::recrally::{RecRallyConfig, RecRallyMode};
use metallic_rc_client::kart::{spawn_kart, Kart, KartPlugin};

#[derive(Component)]
struct Racer {
    team: u8,
    next_cp: usize,
    lap: u32,
    finished: bool,
}

#[derive(Resource)]
struct Track {
    cps: Vec<Vec3>,
}

#[derive(Resource)]
struct Rules(RecRallyMode);

#[derive(Resource, Default)]
struct Outcome {
    order: Vec<(u8, usize)>,
    winner: Option<u8>,
    over: bool,
}

const KARTS: u8 = 3;
const LAPS: u32 = 3;
const CP_RADIUS: f32 = 6.0;
const Y: f32 = 0.3;

/// A large rectangular loop (x 0..40, z 0..60) with edge midpoints — 8 checkpoints so the pursuit AI always has a
/// nearby target and turns gradually.
fn track_loop() -> Vec<Vec3> {
    [(0.0, 0.0), (0.0, 30.0), (0.0, 60.0), (20.0, 60.0), (40.0, 60.0), (40.0, 30.0), (40.0, 0.0), (20.0, 0.0)]
        .iter()
        .map(|&(x, z)| Vec3::new(x, Y, z))
        .collect()
}

fn main() {
    let cps = track_loop();
    let mut rules = RecRallyMode::new(RecRallyConfig { total_laps: LAPS, game_duration: 300.0 });
    for _ in 0..KARTS {
        rules.add_player(0); // each kart is a team (join order → team 0..KARTS)
    }

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(KartPlugin)
        .insert_resource(Track { cps })
        .insert_resource(Rules(rules))
        .init_resource::<Outcome>()
        .add_systems(Startup, setup)
        .add_systems(Update, (kart_ai, lap_tracking).chain());

    for _ in 0..9000 {
        // up to 150 s
        app.update();
        if app.world().resource::<Outcome>().over {
            break;
        }
    }

    let o = app.world().resource::<Outcome>();
    let order: Vec<String> = o.order.iter().map(|(t, p)| format!("team{t}=P{p}")).collect();
    match o.winner {
        Some(w) => println!("recrally race: team {w} WINS (1st) — finish order [{}] — drive→laps→finish-order→placement pipeline OK", order.join(", ")),
        None => println!("recrally race: NO WINNER in 150 s — karts didn't finish 3 laps (track/AI needs tuning)"),
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((RigidBody::Fixed, Collider::cuboid(200.0, 0.1, 200.0), Transform::from_xyz(20.0, -0.1, 30.0))); // flat ground under the loop
    for i in 0..KARTS {
        // single-file just behind checkpoint 0 = (0,0), facing +Z (identity rotation → kart forward is +Z); staggered
        // in z so the front kart (team 0) leads and finishes first (deterministic order).
        let e = spawn_kart(&mut commands, Transform::from_xyz(0.0, Y, -3.0 - i as f32 * 4.0), 150.0);
        commands.entity(e).insert(Racer { team: i, next_cp: 0, lap: 0, finished: false });
    }
}

/// Pursuit AI: full throttle, steer toward the next checkpoint. steer sign matches the drive convention (steer>0 =
/// right): a left target (cross_y>0) needs steer<0.
fn kart_ai(track: Res<Track>, mut q: Query<(&Racer, &Transform, &mut Kart)>) {
    for (r, tf, mut k) in &mut q {
        if r.finished {
            k.throttle = 0.0;
            k.steer = 0.0;
            continue;
        }
        let target = track.cps[r.next_cp];
        let pos = tf.translation;
        let to = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z).normalize_or_zero();
        let f = tf.rotation * Vec3::Z;
        let fwd = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        if to == Vec3::ZERO || fwd == Vec3::ZERO {
            continue;
        }
        let cross_y = fwd.z * to.x - fwd.x * to.z; // (fwd × to).y > 0 → target is to the left
        k.steer = (-cross_y).clamp(-1.0, 1.0);
        k.throttle = 1.0;
    }
}

/// Count ordered checkpoints + laps; completing LAPS raises FinishRace into the real RecRallyMode (place + winner).
fn lap_tracking(track: Res<Track>, mut q: Query<(&mut Racer, &Transform)>, mut rules: ResMut<Rules>, mut out: ResMut<Outcome>) {
    if out.over {
        return;
    }
    for (mut r, tf) in &mut q {
        if r.finished {
            continue;
        }
        let cp = track.cps[r.next_cp];
        let d = Vec3::new(cp.x - tf.translation.x, 0.0, cp.z - tf.translation.z).length();
        if d < CP_RADIUS {
            r.next_cp = (r.next_cp + 1) % track.cps.len();
            if r.next_cp == 0 {
                r.lap += 1;
                if r.lap >= LAPS {
                    r.finished = true;
                    let mut events = Vec::new();
                    rules.0.input(GameInput::FinishRace { team: r.team }, &mut events);
                    if let Some(p) = rules.0.place_of(r.team) {
                        out.order.push((r.team, p));
                    }
                    for ev in events {
                        if let GameEvent::Round { phase: RoundPhase::GameOver, winner: Some(w), .. } = ev {
                            out.winner = Some(w);
                        }
                    }
                }
            }
        }
    }
    if out.order.len() >= KARTS as usize {
        out.over = true;
    }
}
