//! De-risk for the reusable PROJECTILE-COMBAT system (shared by Paintball / BattleRoyale / Arena / Quest): does
//! rapier give reliable fast-projectile HIT detection on the physics foundation? Launches a small fast projectile
//! (CCD on, so it can't tunnel) at a player-sized target and reports when the collision event fires. The real
//! projectile speed / mass / lifetime / damage come from the dump later — this just proves the MECHANISM.
//!
//! `cargo run --profile fast -p metallic-rc-client --example projectile_hit`
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

#[derive(Component)]
struct Target;
#[derive(Component)]
struct Projectile;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_systems(Startup, setup)
        .add_systems(Update, detect);
    // ~2 s: plenty for a 40 m/s shot to cross 10 m
    for _ in 0..120 {
        app.update();
    }
    println!("projectile de-risk complete");
}

fn setup(mut commands: Commands) {
    // a player-sized target 10 m downrange, emitting collision events
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(0.3, 0.9, 0.3),
        ActiveEvents::COLLISION_EVENTS,
        Transform::from_xyz(0.0, 0.9, 10.0),
        Target,
    ));
    // a small fast projectile launched straight at the target: CCD on so it doesn't tunnel through at speed, no
    // gravity so it flies level (the real paintball has slight drop — a dump value — but this is the hit mechanism)
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(0.03),
        Ccd::enabled(),
        GravityScale(0.0),
        Velocity { linear: Vec3::new(0.0, 0.0, 40.0), angular: Vec3::ZERO },
        ActiveEvents::COLLISION_EVENTS,
        Transform::from_xyz(0.0, 0.9, 0.0),
        Projectile,
    ));
}

fn detect(mut ev: MessageReader<CollisionEvent>, proj: Query<(), With<Projectile>>, targ: Query<(), With<Target>>, mut hit: Local<bool>) {
    for e in ev.read() {
        if let CollisionEvent::Started(a, b, _) = e {
            let is_hit = (proj.get(*a).is_ok() && targ.get(*b).is_ok()) || (proj.get(*b).is_ok() && targ.get(*a).is_ok());
            if is_hit && !*hit {
                *hit = true;
                println!("HIT: projectile struck the target (collision event fired)");
            }
        }
    }
}
