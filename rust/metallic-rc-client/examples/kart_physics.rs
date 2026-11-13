//! De-risk for the KART (GroundVehicle) physics ([`metallic_rc_client::kart`]): does the arcade drive model give a
//! kart that accelerates to its max speed, STEERS (yaws) when turning, and GRIPS (its velocity follows its facing
//! instead of sliding straight on)? Drives a single kart on a flat ground: full throttle straight for 2 s, then full
//! throttle + full right steer, and reports speed / heading / slip + that it stayed grounded + upright. Headless,
//! deterministic (Time advanced a fixed 1/60 per update via TimeUpdateStrategy, matching the fixed physics step).
//!
//! `cargo run --profile fast -p metallic-rc-client --example kart_physics`
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_rapier3d::prelude::*;
use metallic_rc_client::kart::{spawn_kart, Kart, KartPlugin, KartTuning};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(KartPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, control);

    for _ in 0..300 {
        // 5 s
        app.update();
    }

    let tuning = *app.world().resource::<KartTuning>();
    let mut q = app.world_mut().query::<(&Transform, &Velocity)>();
    let world = app.world();
    for (tf, vel) in q.iter(world) {
        let v_h = Vec3::new(vel.linear.x, 0.0, vel.linear.z);
        let speed = v_h.length();
        let fwd = tf.rotation * Vec3::Z;
        let heading = fwd.x.atan2(fwd.z).to_degrees(); // 0° = +Z (start), +90° = +X
        let slip = if speed > 0.5 { v_h.normalize().dot(Vec3::new(fwd.x, 0.0, fwd.z).normalize()).clamp(-1.0, 1.0).acos().to_degrees() } else { 0.0 };
        println!(
            "kart physics: speed {speed:.1}/{:.0} m/s, pos ({:.1},{:.1}), ground y {:.2}, heading {heading:.0}° (turned from 0°), slip {slip:.1}° (grip → small)",
            tuning.max_speed, tf.translation.x, tf.translation.z, tf.translation.y
        );
        let ok_speed = speed > tuning.max_speed * 0.8;
        let ok_turn = heading.abs() > 20.0;
        let ok_grip = slip < 20.0;
        let ok_ground = (tf.translation.y - 0.3).abs() < 0.2;
        println!("kart physics: {} — accel {ok_speed}, steer {ok_turn}, grip {ok_grip}, grounded/upright {ok_ground}", if ok_speed && ok_turn && ok_grip && ok_ground { "PASS" } else { "CHECK" });
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((RigidBody::Fixed, Collider::cuboid(200.0, 0.1, 200.0), Transform::from_xyz(0.0, -0.1, 0.0))); // flat ground, top at y=0
    spawn_kart(&mut commands, Transform::from_xyz(0.0, 0.3, 0.0), 150.0); // kart body resting on the ground, facing +Z
}

/// Full throttle throughout; straight for the first 2 s, then full right steer.
fn control(time: Res<Time>, mut q: Query<&mut Kart>, mut elapsed: Local<f32>) {
    *elapsed += time.delta_secs();
    for mut k in &mut q {
        k.throttle = 1.0;
        k.steer = if *elapsed > 2.0 { 1.0 } else { 0.0 };
    }
}
