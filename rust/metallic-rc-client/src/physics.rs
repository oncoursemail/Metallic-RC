//! The dynamic-prop PHYSICS FOUNDATION (bevy_rapier3d) — the reusable rigidbody layer the physics-heavy games need.
//!
//! It does two things: (1) mirrors the room's static colliders ([`crate::world::FixedSolids`]) as rapier FIXED
//! colliders, so dynamic props rest / roll / collide against the real room geometry; and (2) offers a small API to
//! spawn DYNAMIC rigidbody props ([`spawn_dynamic`]) that the solver simulates — bowling pins that topple, projectiles
//! that fly, karts that drive. The existing custom player/prop collision (`collide.rs` / `interact.rs`) is left
//! unchanged; this adds a real solver for the props that genuinely need rigidbody dynamics (which the box-bounce
//! custom physics can't do). A real solver matches Unity's PhysX far better than hand-rolled toppling, and the layer
//! is reused by Bowling (pins), Paintball (projectiles), RecRally (karts), and the BattleRoyale zone.
//!
//! Smoke-tested headless (examples/rapier_smoke.rs): a dynamic box falls under gravity and rests on a fixed ground.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::collide::Collider as RoomCollider;
use crate::world::FixedSolids;

/// Set once the room's static colliders have been mirrored into rapier (so we do it a single time per room load).
#[derive(Resource, Default)]
struct RoomMirrored(bool);

/// The reusable rigidbody-physics plugin: the rapier solver + a real-time timestep, and it mirrors the room's static
/// geometry into the physics world. Add it once; gameplay then spawns dynamic props via [`spawn_dynamic`].
pub struct RapierFoundationPlugin;

impl Plugin for RapierFoundationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            // real-time stepping, capped so a slow frame can't explode the sim (gravity is rapier's default −9.81)
            .insert_resource(TimestepMode::Variable { max_dt: 1.0 / 60.0, time_scale: 1.0, substeps: 2 })
            .init_resource::<RoomMirrored>()
            .add_systems(Update, mirror_room_colliders);
    }
}

/// Once the room's static colliders are ready, spawn a rapier FIXED collider for each (box / sphere / capsule — mesh
/// colliders are a follow-up), so dynamic props have the real floor/walls to interact with.
fn mirror_room_colliders(mut commands: Commands, fixed: Res<FixedSolids>, mut done: ResMut<RoomMirrored>) {
    if done.0 || fixed.0.is_empty() {
        return;
    }
    let mut n = 0u32;
    for c in &fixed.0 {
        match c {
            RoomCollider::Box(obb) => {
                let rot = Quat::from_mat3(&Mat3::from_cols(obb.axes[0], obb.axes[1], obb.axes[2]));
                commands.spawn((
                    RigidBody::Fixed,
                    Collider::cuboid(obb.half.x, obb.half.y, obb.half.z),
                    Transform::from_translation(obb.center).with_rotation(rot),
                ));
                n += 1;
            }
            RoomCollider::Sphere { center, radius } => {
                commands.spawn((RigidBody::Fixed, Collider::ball(*radius), Transform::from_translation(*center)));
                n += 1;
            }
            RoomCollider::Capsule { a, b, r } => {
                commands.spawn((RigidBody::Fixed, Collider::capsule(*a, *b, *r), Transform::IDENTITY));
                n += 1;
            }
            RoomCollider::Mesh(_) => {} // mesh colliders → rapier trimesh: a follow-up
        }
    }
    done.0 = true;
    info!("rapier: mirrored {n} room colliders as fixed bodies");
}

/// Spawn a DYNAMIC rigidbody prop — the reusable API. Returns its entity so the caller can track it (ownership,
/// networking, the pin-upright check, …). `linvel`/`angvel` seed its motion (e.g. a bowled ball's release velocity).
pub fn spawn_dynamic(commands: &mut Commands, collider: Collider, mass: f32, transform: Transform, linvel: Vec3, angvel: Vec3) -> Entity {
    commands
        .spawn((
            RigidBody::Dynamic,
            collider,
            ColliderMassProperties::Mass(mass),
            Velocity { linear: linvel, angular: angvel },
            transform,
        ))
        .id()
}

/// A rigidbody's tilt from vertical, in degrees: the angle between its local up and world up. Used for "is it still
/// standing" checks — e.g. Bowling's real pin rule `AngleFromUpright <= 25°`.
pub fn tilt_from_upright_deg(rotation: Quat) -> f32 {
    (rotation * Vec3::Y).angle_between(Vec3::Y).to_degrees()
}
