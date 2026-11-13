//! Reusable KART (GroundVehicle) physics on bevy_rapier — the arcade vehicle model for RecRally (and any kart game).
//! A kart is a dynamic rigidbody that drives forward on throttle, yaws on steer, and GRIPS (its sideways velocity is
//! damped so it corners instead of sliding), capped at a max speed, kept upright (pitch/roll locked so it can't tip).
//! Inputs (`throttle`, `steer`) are set each frame from player input or AI; the `drive` system turns them into motion
//! with the [`KartTuning`]. The tuning is the REAL GroundVehicle dump values; this module is the value-independent
//! mechanism, de-risked in examples/kart_physics.rs.
//!
//! Forward is the kart's local +Z. The model is velocity-driven (sets the rigidbody's Velocity each frame): collisions
//! still resolve through rapier, but the drive is arcade-stable (no wheel-collider tuning), which matches how a
//! GroundVehicle feels and is cheap at many karts.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// A drivable kart. `throttle` ∈ [-1,1] (forward / reverse), `steer` ∈ [-1,1] (right / left). Set these each frame.
#[derive(Component, Default)]
pub struct Kart {
    pub throttle: f32,
    pub steer: f32,
}

/// GroundVehicle tuning (shared). 1:1: PENDING the real GroundVehicle dump (requested from the decompile lane) — the
/// values below are arcade-reasonable PLACEHOLDERS so the drive model + lap detection work; reconcile when it lands.
#[derive(Resource, Clone, Copy)]
pub struct KartTuning {
    /// forward acceleration (m/s²) at full throttle
    pub accel: f32,
    /// forward speed cap (m/s)
    pub max_speed: f32,
    /// reverse speed cap (m/s)
    pub max_reverse: f32,
    /// yaw rate (rad/s) at full steer and full speed
    pub steer_rate: f32,
    /// lateral-slip damping (per second): higher = grippier (corners instead of sliding)
    pub grip: f32,
}

impl Default for KartTuning {
    fn default() -> Self {
        KartTuning { accel: 12.0, max_speed: 18.0, max_reverse: 6.0, steer_rate: 2.2, grip: 8.0 } // 1:1: placeholder
    }
}

pub struct KartPlugin;

impl Plugin for KartPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KartTuning>().add_systems(Update, drive);
    }
}

/// Spawn a kart rigidbody at `transform` (facing its local +Z), with body `mass`. Pitch/roll are locked so it stays
/// upright; only yaw (steering) and planar motion are free. Collider ~a kart body (1.0 × 0.6 × 1.8 m) — pending the
/// real GroundVehicle collider.
pub fn spawn_kart(commands: &mut Commands, transform: Transform, mass: f32) -> Entity {
    commands
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(0.5, 0.3, 0.9),
            ColliderMassProperties::Mass(mass),
            LockedAxes::ROTATION_LOCKED_X | LockedAxes::ROTATION_LOCKED_Z,
            Velocity::zero(),
            transform,
            Kart::default(),
        ))
        .id()
}

/// The arcade drive model: throttle accelerates along the kart's forward, steer yaws it (scaled by speed, so no
/// turning in place), grip damps sideways slip, speed is capped, and the vertical (gravity) component is preserved.
fn drive(time: Res<Time>, tuning: Res<KartTuning>, mut q: Query<(&Kart, &Transform, &mut Velocity)>) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    for (kart, tf, mut vel) in &mut q {
        // forward in the horizontal plane (kart faces local +Z)
        let fwd = tf.rotation * Vec3::Z;
        let fwd_h = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
        if fwd_h == Vec3::ZERO {
            continue;
        }
        let v = vel.linear;
        let v_h = Vec3::new(v.x, 0.0, v.z);
        // forward speed (signed), advanced by throttle and capped
        let mut fwd_speed = v_h.dot(fwd_h);
        fwd_speed = (fwd_speed + kart.throttle * tuning.accel * dt).clamp(-tuning.max_reverse, tuning.max_speed);
        // lateral slip, damped by grip
        let lateral = (v_h - fwd_h * v_h.dot(fwd_h)) * (1.0 - (tuning.grip * dt).min(1.0));
        // recompose horizontal, keep the vertical component (gravity / ground)
        vel.linear = fwd_h * fwd_speed + lateral + Vec3::Y * v.y;
        // steering: yaw about +Y, scaled by speed (no turning in place) and reversed when driving backward. steer>0
        // = right → negative yaw (clockwise looking down).
        let speed_factor = (fwd_speed.abs() / tuning.max_speed).clamp(0.0, 1.0);
        let reverse = if fwd_speed >= 0.0 { 1.0 } else { -1.0 };
        vel.angular = Vec3::new(0.0, -kart.steer * tuning.steer_rate * speed_factor * reverse, 0.0);
    }
}
