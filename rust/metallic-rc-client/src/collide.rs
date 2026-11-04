//! The screen-mode player's body against the room's box colliders: a capsule 1.7 m tall, radius 0.25, that steps
//! up ledges of 0.25 m (the 2026 client's values, docs/agents/rc-spec/player.md in the private notes).
use bevy::math::{Mat4, Vec3};

pub const HEIGHT: f32 = 1.7;
pub const RADIUS: f32 = 0.25;
pub const STEP: f32 = 0.25;

/// An oriented box in the client's space.
#[derive(Clone, Debug)]
pub struct Obb {
    pub center: Vec3,
    /// unit axes
    pub axes: [Vec3; 3],
    pub half: Vec3,
}

impl Obb {
    /// A Unity BoxCollider (`center`, `size` in its own Unity space) under the object's world matrix (Bevy space).
    pub fn from_box_collider(world: Mat4, center: [f32; 3], size: [f32; 3]) -> Self {
        let (scale, rot, _) = world.to_scale_rotation_translation();
        Obb {
            center: world.transform_point3(Vec3::new(center[0], center[1], -center[2])),
            axes: [rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z],
            half: (Vec3::from(size) * scale).abs() * 0.5,
        }
    }

    fn closest(&self, p: Vec3) -> Vec3 {
        let d = p - self.center;
        let mut q = self.center;
        for i in 0..3 {
            q += self.axes[i] * d.dot(self.axes[i]).clamp(-self.half[i], self.half[i]);
        }
        q
    }

    /// The highest point of the box.
    pub fn top(&self) -> f32 {
        self.center.y + (0..3).map(|i| (self.axes[i] * self.half[i]).y.abs()).sum::<f32>()
    }

    /// How far to move a sphere so it no longer overlaps, if it does.
    fn push(&self, c: Vec3, r: f32) -> Option<Vec3> {
        let q = self.closest(c);
        let d = c - q;
        let dist = d.length();
        if dist >= r {
            return None;
        }
        if dist > 1e-5 {
            return Some(d / dist * (r - dist));
        }
        // the centre is inside: leave by the nearest face
        let local = c - self.center;
        let (i, depth) = (0..3)
            .map(|i| (i, self.half[i] - local.dot(self.axes[i]).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let sign = if local.dot(self.axes[i]) < 0.0 { -1.0 } else { 1.0 };
        Some(self.axes[i] * sign * (depth + r))
    }
}

/// Moves feet by `motion` through the boxes. Returns the new feet position and whether it stands on something.
/// A long move (a slow frame, a fast fall) goes in steps of at most half the radius, so it cannot pass through a
/// thin box.
pub fn move_capsule(boxes: &[Obb], feet: Vec3, motion: Vec3) -> (Vec3, bool) {
    let steps = (motion.length() / (RADIUS * 0.5)).ceil().max(1.0) as usize;
    let (mut p, mut grounded) = (feet, false);
    for _ in 0..steps {
        let (q, g) = move_step(boxes, p, motion / steps as f32);
        (p, grounded) = (q, grounded || g);
    }
    (p, grounded)
}

fn move_step(boxes: &[Obb], feet: Vec3, motion: Vec3) -> (Vec3, bool) {
    let heights = [RADIUS, HEIGHT * 0.5, HEIGHT - RADIUS];
    let mut p = feet + motion;
    let mut grounded = false;
    for _ in 0..4 {
        let mut moved = false;
        for b in boxes {
            // a ledge no higher than a step is climbed instead of pushing back, if the body fits on top
            let top = b.top();
            if motion.y <= 0.0 && top > p.y && top - p.y <= STEP && b.push(p + Vec3::Y * RADIUS, RADIUS).is_some()
                && heights.iter().all(|&h| b.push(Vec3::new(p.x, top + h, p.z), RADIUS * 0.99).is_none())
            {
                p.y = top;
                grounded = true;
                moved = true;
                continue;
            }
            for &h in &heights {
                if let Some(d) = b.push(p + Vec3::Y * h, RADIUS) {
                    p += d;
                    moved = true;
                    if h == RADIUS && d.normalize_or_zero().y > 0.7 {
                        grounded = true;
                    }
                }
            }
        }
        if !moved {
            break;
        }
    }
    (p, grounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(center: Vec3, size: Vec3) -> Obb {
        Obb { center, axes: [Vec3::X, Vec3::Y, Vec3::Z], half: size * 0.5 }
    }

    #[test]
    fn stands_on_the_floor() {
        let floor = [cube(Vec3::new(0.0, -0.5, 0.0), Vec3::new(10.0, 1.0, 10.0))];
        let (p, grounded) = move_capsule(&floor, Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.0, -0.1, 0.0));
        assert!(grounded);
        assert!((p.y - 0.0).abs() < 1e-3, "{p}");
    }

    #[test]
    fn a_long_fall_lands_on_a_thin_floor() {
        // one slow frame at full falling speed: 40 m in one move onto a 5 cm floor
        let floor = [cube(Vec3::new(0.0, -0.025, 0.0), Vec3::new(10.0, 0.05, 10.0))];
        let (p, grounded) = move_capsule(&floor, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, -40.0, 0.0));
        assert!(grounded && p.y.abs() < 1e-3, "{p}");
    }

    #[test]
    fn stops_at_a_wall() {
        let room = [cube(Vec3::new(0.0, -0.5, 0.0), Vec3::new(10.0, 1.0, 10.0)), cube(Vec3::new(2.0, 1.5, 0.0), Vec3::new(0.2, 3.0, 10.0))];
        let mut p = Vec3::ZERO;
        for _ in 0..60 {
            p = move_capsule(&room, p, Vec3::new(0.05, 0.0, 0.0)).0;
        }
        assert!((p.x - (1.9 - RADIUS)).abs() < 1e-3, "{p}");
        assert!(p.y.abs() < 1e-3, "{p}");
    }

    #[test]
    fn steps_up_a_ledge_but_not_a_table() {
        let floor = cube(Vec3::new(0.0, -0.5, 0.0), Vec3::new(10.0, 1.0, 10.0));
        let ledge = cube(Vec3::new(1.5, 0.1, 0.0), Vec3::new(1.0, 0.2, 2.0));
        let (p, grounded) = move_capsule(&[floor.clone(), ledge], Vec3::new(0.9, 0.0, 0.0), Vec3::new(0.2, 0.0, 0.0));
        assert!(grounded && (p.y - 0.2).abs() < 1e-3, "{p}");
        let table = cube(Vec3::new(1.5, 0.4, 0.0), Vec3::new(1.0, 0.8, 2.0));
        let (p, _) = move_capsule(&[floor, table], Vec3::new(0.7, 0.0, 0.0), Vec3::new(0.2, 0.0, 0.0));
        assert!(p.y.abs() < 1e-3 && p.x <= 1.0 - RADIUS + 1e-3, "{p}");
    }
}
