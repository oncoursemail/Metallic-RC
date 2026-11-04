//! The face screen's layout and where the eyes look. The face is a dark screen on the front of the head; the eyes
//! and mouth are lights on it, placed by longitude and latitude on the head shell (`mesh::on_head`) so they stay on
//! its curve as they move. The eyes glide across the screen toward what the robot looks at.
use bevy::math::{Vec2, Vec3};

use super::mesh::HEAD_HALF;

/// The screen: half its width (longitude), its middle and half its height (latitude), radians on the head.
pub const SCREEN: (f32, f32, f32) = (0.92, -0.1, 0.5);
/// The rim round the screen, a little larger.
pub const BEZEL: (f32, f32, f32) = (1.0, -0.1, 0.58);
/// Where the eyes sit looking straight ahead (either side by `EYE_LON`), and the mouth.
pub const EYE_LON: f32 = 0.3;
pub const EYE_LAT: f32 = 0.06;
pub const MOUTH_LAT: f32 = -0.36;
/// How far the eyes travel across the screen at the edge of their look, radians.
pub const LOOK: Vec2 = Vec2::new(0.17, 0.11);
/// Beyond these angles off straight ahead (radians: sideways, up or down) the eyes stop following.
const REACH: Vec2 = Vec2::new(0.9, 0.7);

/// Where the eyes look (-1..1 across and up the screen) to see `target`, given in the head's own space (+y up,
/// facing -z, centred on the shell); None when it is behind or too far to the side to follow.
pub fn gaze(target: Vec3) -> Option<Vec2> {
    let d = target - Vec3::new(0.0, 0.0, -HEAD_HALF.z);
    let flat = Vec2::new(d.x, d.z).length();
    if d.length() < 1e-3 || -d.z < 0.25 * d.length() {
        return None;
    }
    let (yaw, pitch) = (d.x.atan2(-d.z), d.y.atan2(flat));
    Some((Vec2::new(yaw, pitch) / REACH).clamp(Vec2::NEG_ONE, Vec2::ONE))
}

/// An idle glance: where the eyes wander when nothing holds them, a new spot every couple of seconds (different per
/// avatar by `seed`), mostly near the middle.
pub fn glance(seed: u32, t: f32) -> Vec2 {
    let span = 1.6 + (seed % 5) as f32 * 0.3;
    let k = (t / span).floor() as i64 as u64;
    // splitmix64 of the spot number: stable, no state
    let mut h = k.wrapping_add(seed as u64).wrapping_mul(0x9e3779b97f4a7c15);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    let unit = |bits: u64| (bits & 0xffff) as f32 / 65535.0 * 2.0 - 1.0;
    // every third spot is straight ahead
    if h % 3 == 0 {
        Vec2::ZERO
    } else {
        Vec2::new(unit(h >> 8) * 0.55, unit(h >> 32) * 0.35)
    }
}

/// The longitude and latitude on the head of the left (0) or right (1) eye looking `look` (-1..1).
pub fn eye_at(side: usize, look: Vec2) -> (f32, f32) {
    let s = if side == 0 { -1.0 } else { 1.0 };
    (s * EYE_LON + look.x * LOOK.x, EYE_LAT + look.y * LOOK.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eyes_follow_what_is_in_front() {
        // straight ahead, a metre off
        assert_eq!(gaze(Vec3::new(0.0, 0.0, -1.0)), Some(Vec2::ZERO));
        // to the right (+x) and up: the eyes go right and up, not past the edge
        let g = gaze(Vec3::new(0.6, 0.3, -1.0)).unwrap();
        assert!(g.x > 0.5 && g.y > 0.2 && g.x <= 1.0, "{g}");
        let far = gaze(Vec3::new(3.0, -4.0, -2.5)).unwrap();
        assert_eq!(far, Vec2::new(1.0, -1.0));
        // behind the head, or right beside it: nothing to follow
        assert_eq!(gaze(Vec3::new(0.0, 0.0, 2.0)), None);
        assert_eq!(gaze(Vec3::new(2.0, 0.0, -0.3)), None);
        // the eyes stay on the screen: the outer edge of the look is inside the screen's half width
        let (lon, lat) = eye_at(1, Vec2::ONE);
        assert!(lon < SCREEN.0 - 0.2 && lat < SCREEN.1 + SCREEN.2 - 0.15, "{lon} {lat}");
        assert_eq!(eye_at(0, Vec2::ZERO).0, -eye_at(1, Vec2::ZERO).0);
    }

    #[test]
    fn idle_glances_hold_then_move_and_stay_small() {
        let spots: Vec<Vec2> = (0..200).map(|i| glance(7, i as f32 * 0.5)).collect();
        assert!(spots.iter().all(|g| g.x.abs() <= 0.55 && g.y.abs() <= 0.35));
        // held for a while (consecutive samples often equal), but not one spot for ever
        let held = spots.windows(2).filter(|w| w[0] == w[1]).count();
        let moved = spots.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(held > 100 && moved > 20, "{held} {moved}");
        // different avatars glance differently
        assert_ne!((0..20).map(|i| glance(1, i as f32)).collect::<Vec<_>>(), (0..20).map(|i| glance(2, i as f32)).collect::<Vec<_>>());
    }
}
