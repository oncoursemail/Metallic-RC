//! De-risk for the first physics-foundation application: does rapier give real BOWLING pin-toppling? Spawns the real
//! 4-3-2-1 ten-pin triangle (at level91's lane-1 layout) + a ball as dynamic rigidbodies on a fixed lane, bowls the
//! ball down the middle, lets it topple the pins, then counts how many are still STANDING by the real rule
//! (`AngleFromUpright <= 25°`) — so pinfall = 10 − standing. Headless, deterministic.
//!
//! `cargo run --profile fast -p metallic-rc-client --example bowling_physics`
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use metallic_rc_client::physics::tilt_from_upright_deg;

#[derive(Component)]
struct Pin;

const PIN_UPRIGHT_MAX_DEG: f32 = 25.0; // VERIFIED real rule
const FLOOR_TOP: f32 = 0.13; // level91 pins sit at y≈0.13

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_systems(Startup, setup);
    // ~4 s: enough for the ball to travel the lane and the pins to settle
    for _ in 0..240 {
        app.update();
    }
    // count standing pins by the real 25° rule
    let mut q = app.world_mut().query_filtered::<&Transform, With<Pin>>();
    let world = app.world();
    let (mut standing, mut total) = (0, 0);
    for t in q.iter(world) {
        total += 1;
        if tilt_from_upright_deg(t.rotation) <= PIN_UPRIGHT_MAX_DEG {
            standing += 1;
        }
    }
    println!("pins: {total} total, {standing} standing (tilt<= {PIN_UPRIGHT_MAX_DEG}°), PINFALL = {}", total - standing);
}

fn setup(mut commands: Commands) {
    // the lane: a fixed slab with its top at FLOOR_TOP
    commands.spawn((RigidBody::Fixed, Collider::cuboid(3.0, 0.1, 30.0), Transform::from_xyz(0.0, FLOOR_TOP - 0.1, 9.0)));

    // the real lane-1 ten-pin triangle (4-3-2-1), z rows from level91's BowlingPin layout. EXACT dump pin: compound
    // body (base box + neck/belly capsules), mass 1, root at the base (sweet-galileo_bowling-stunt-charades).
    let pin = || {
        Collider::compound(vec![
            (Vec3::new(0.0, 0.05, 0.0), Quat::IDENTITY, Collider::cuboid(0.0275, 0.05, 0.0275)),
            (Vec3::new(0.0, 0.19, 0.0), Quat::IDENTITY, Collider::capsule_y(0.15, 0.04)),
            (Vec3::new(0.0, 0.12, 0.0), Quat::IDENTITY, Collider::capsule_y(0.024, 0.076)),
        ])
    };
    let rows = [(4usize, 17.67_f32), (3, 17.28), (2, 16.89), (1, 16.50)];
    for (count, z) in rows {
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * 0.45; // ~0.45 m pin spacing
            commands.spawn((
                RigidBody::Dynamic,
                pin(),
                ColliderMassProperties::Mass(1.0),
                Damping { linear_damping: 0.0, angular_damping: 1.0 },
                Friction { coefficient: 0.6, combine_rule: CoefficientCombineRule::Average },
                Restitution { coefficient: 0.6, combine_rule: CoefficientCombineRule::Max },
                Transform::from_xyz(x, FLOOR_TOP, z), // compound root = pin base
                Pin,
            ));
        }
    }

    // the ball bowled into the 1-3 POCKET (a dead-centre ball splits; the pocket line strikes). Aimed from slightly
    // left of the lane centre toward x≈0.15 at the pins, ~9 m/s. EXACT dump ball: sphere r0.125, mass 8 (8:1 vs pin).
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(0.125),
        ColliderMassProperties::Mass(8.0),
        Friction { coefficient: 0.14, combine_rule: CoefficientCombineRule::Multiply },
        Restitution { coefficient: 0.25, combine_rule: CoefficientCombineRule::Multiply },
        Velocity { linear: Vec3::new(0.09, 0.0, 9.0), angular: Vec3::ZERO },
        Transform::from_xyz(-0.1, FLOOR_TOP + 0.125, 0.0),
    ));
}
