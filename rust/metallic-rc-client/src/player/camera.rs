//! Where the screen camera goes: the game's pivot-plus-offset rig (`ScreenPlayerCameraSettings`), first person at
//! the eyes, third person over the shoulder pulled in front of any wall, with the field of view multipliers and head
//! bob. Positions are in Bevy's axes (the presets are in Unity's, so their z is negated here).
use bevy::math::{Quat, Vec3};

use super::tuning::CameraPreset;
use crate::collide::{self, Collider};

/// The camera's world position for `preset`, the player standing at `feet` with normalized `height`, looking `view`.
/// INFERRED: the pivot scales with the player's height (the eye ends at 1.55 m for 1.7 m).
pub fn place(preset: &CameraPreset, feet: Vec3, yaw: f32, view: Quat, height: f32) -> (Vec3, Vec3) {
    let unity = |v: Vec3| Vec3::new(v.x, v.y, -v.z);
    let pivot = feet + Quat::from_rotation_y(yaw) * unity(preset.pivot) * Vec3::new(1.0, height, 1.0);
    (pivot, pivot + view * unity(preset.offset))
}

/// The furthest point from `pivot` toward `want` where a ball of `radius` touches no box (the camera does not go
/// into walls); `pivot` itself when even the first step is blocked.
pub fn pull_in(solids: &[Collider], pivot: Vec3, want: Vec3, radius: f32) -> Vec3 {
    let steps = 40;
    let clear = (1..=steps).map(|i| i as f32 / steps as f32).take_while(|f| collide::sphere_clear(solids, pivot.lerp(want, *f), radius)).last();
    pivot.lerp(want, clear.unwrap_or(0.0))
}

/// The highest pitch (radians, looking up) a preset allows: `MinPitchAngleFromUp` keeps the view that far from
/// straight up. First person allows 1.5 (INFERRED: the game's own clamp was not found).
pub fn max_pitch(preset: &CameraPreset) -> f32 {
    if preset.min_pitch_from_up > 0.0 { (90.0 - preset.min_pitch_from_up).to_radians() } else { 1.5 }
}

/// Head bob for the Head Bob setting (`amount` 0..1) at stride `phase` (radians, one turn per two steps) and
/// `speed` m/s: the eyes dip with each step and sway a little sideways. INFERRED amplitudes (the game's bob code was not
/// read): 3 cm down and 1.5 cm sideways at walking speed.
pub fn bob(amount: f32, phase: f32, speed: f32) -> Vec3 {
    let k = amount.clamp(0.0, 1.0) * (speed / 3.0).min(1.5);
    Vec3::new(0.015 * k * phase.sin(), -0.03 * k * (phase.sin()).abs(), 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::tuning::Tuning;

    #[test]
    fn first_person_is_at_the_eyes_and_third_behind_the_right_shoulder() {
        let t = Tuning::default();
        let (_, eye) = place(&t.first_person, Vec3::ZERO, 0.0, Quat::IDENTITY, 1.0);
        // 1.55 up, a centimetre forward (Unity -0.08 + 0.09 in z)
        assert!((eye - Vec3::new(0.0, 1.55, -0.01)).length() < 1e-5, "{eye}");
        let (pivot, cam) = place(&t.third_person, Vec3::ZERO, 0.0, Quat::IDENTITY, 1.0);
        assert!((pivot - Vec3::new(0.0, 1.6, 0.0)).length() < 1e-5);
        assert!((cam - Vec3::new(0.4875, 1.6, 1.8)).length() < 1e-5, "{cam}");
        // turned half round: behind is now -z
        let half = Quat::from_rotation_y(std::f32::consts::PI);
        let (_, cam) = place(&t.third_person, Vec3::ZERO, std::f32::consts::PI, half, 1.0);
        assert!((cam - Vec3::new(-0.4875, 1.6, -1.8)).length() < 1e-4, "{cam}");
        // crouched, the eyes come down with the height
        let (_, low) = place(&t.first_person, Vec3::ZERO, 0.0, Quat::IDENTITY, 0.725);
        assert!((low.y - (1.35 * 0.725 + 0.2)).abs() < 1e-5);
        assert!((max_pitch(&t.third_person) - 60f32.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn the_camera_stops_in_front_of_a_wall() {
        let wall = Collider::Box(crate::collide::Obb { center: Vec3::new(0.0, 1.5, 1.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::new(5.0, 3.0, 0.05) });
        let at = pull_in(&[wall.clone()], Vec3::new(0.0, 1.6, 0.0), Vec3::new(0.0, 1.6, 1.8), 0.15);
        assert!(at.z <= 0.95 - 0.15 + 1e-3 && at.z > 0.7, "{at}");
        let free = pull_in(&[], Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), 0.15);
        assert_eq!(free, Vec3::new(0.0, 0.0, 2.0));
        assert_eq!(bob(0.0, 1.0, 3.0), Vec3::ZERO);
        assert!(bob(1.0, std::f32::consts::FRAC_PI_2, 3.0).y < -0.029);
    }
}
