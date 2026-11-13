//! Smoke test for the dynamic-physics foundation: does `bevy_rapier3d` simulate a dynamic prop headless (no window,
//! no render) — a box dropped above a fixed ground falls under gravity and comes to rest ON the ground?
//!
//! `cargo run --profile fast -p metallic-rc-client --example rapier_smoke`
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

#[derive(Component)]
struct TheBox;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        // deterministic headless stepping: a fixed dt per Bevy tick, independent of real frame time
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_systems(Startup, setup)
        .add_systems(Update, report);
    // ~3 s of simulation
    for _ in 0..180 {
        app.update();
    }
    println!("smoke test complete");
}

fn setup(mut commands: Commands) {
    // a fixed ground: a thin wide slab with its top at y = 0.1
    commands.spawn((RigidBody::Fixed, Collider::cuboid(10.0, 0.1, 10.0), Transform::from_xyz(0.0, 0.0, 0.0)));
    // a dynamic 1 m box dropped from y = 5 (offset in x so it can't balance perfectly — it should settle flat)
    commands.spawn((RigidBody::Dynamic, Collider::cuboid(0.5, 0.5, 0.5), Transform::from_xyz(0.2, 5.0, 0.0), TheBox));
}

fn report(q: Query<&Transform, With<TheBox>>, mut n: Local<u32>) {
    *n += 1;
    if *n % 30 == 0 {
        if let Ok(t) = q.single() {
            println!("step {:3}: box y = {:.3}", *n, t.translation.y);
        }
    }
}
