//! The screen-mode player's body against the room's box colliders: a capsule 1.7 m tall, radius 0.25, that steps
//! up ledges of 0.25 m (the 2026 client's values, docs/agents/rc-spec/player.md in the private notes).
use bevy::math::{Mat4, Vec3};

pub const HEIGHT: f32 = 1.7;
pub const RADIUS: f32 = 0.25;
pub const STEP: f32 = 0.25;

/// The colliders the player walks against: the room's fixed colliders plus the props' where they are now (kept by
/// `world::WorldPlugin`).
#[derive(bevy::prelude::Resource, Default)]
pub struct Solids(pub Vec<Collider>);

/// A convex or mesh collider the player's capsule collides with. `push`/`top` give it the same interface as `Obb`,
/// so `move_capsule` treats every shape uniformly.
#[derive(Clone, Debug)]
pub enum Collider {
    Box(Obb),
    Sphere { center: Vec3, radius: f32 },
    /// a capsule along the segment `a`..`b`, radius `r`
    Capsule { a: Vec3, b: Vec3, r: f32 },
    Mesh(MeshShape),
}

/// A mesh collider as world-space triangles with a bounding box, for closest-point queries.
#[derive(Clone, Debug)]
pub struct MeshShape {
    pub tris: Vec<[Vec3; 3]>,
    pub min: Vec3,
    pub max: Vec3,
}

impl MeshShape {
    pub fn new(tris: Vec<[Vec3; 3]>) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for t in &tris {
            for v in t {
                min = min.min(*v);
                max = max.max(*v);
            }
        }
        MeshShape { tris, min, max }
    }
}

/// The closest point to `p` on triangle `t`.
fn closest_on_tri(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Push a sphere at `c` radius `r` out of a sphere at `q` radius `rad`, if they overlap.
fn push_from_point(c: Vec3, r: f32, q: Vec3, rad: f32) -> Option<Vec3> {
    let d = c - q;
    let dist = d.length();
    let reach = r + rad;
    if dist >= reach {
        return None;
    }
    if dist > 1e-5 {
        Some(d / dist * (reach - dist))
    } else {
        Some(Vec3::Y * reach)
    }
}

impl Collider {
    /// The inner box when this collider is a `Box`, for the box-only ray helpers (walls, maker picks, paintball
    /// flight); mesh/sphere/capsule shapes are skipped by those (ray-vs-mesh is a separate path).
    pub fn as_box(&self) -> Option<&Obb> {
        if let Collider::Box(o) = self { Some(o) } else { None }
    }

    pub fn top(&self) -> f32 {
        match self {
            Collider::Box(b) => b.top(),
            Collider::Sphere { center, radius } => center.y + radius,
            Collider::Capsule { a, b, r } => a.y.max(b.y) + r,
            Collider::Mesh(m) => m.max.y,
        }
    }

    /// How far to move a sphere at `c` radius `r` so it no longer overlaps this collider, if it does.
    fn push(&self, c: Vec3, r: f32) -> Option<Vec3> {
        match self {
            Collider::Box(b) => b.push(c, r),
            Collider::Sphere { center, radius } => push_from_point(c, r, *center, *radius),
            Collider::Capsule { a, b, r: rad } => {
                let ab = *b - *a;
                let t = if ab.length_squared() > 1e-9 { (c - *a).dot(ab) / ab.length_squared() } else { 0.0 };
                push_from_point(c, r, *a + ab * t.clamp(0.0, 1.0), *rad)
            }
            Collider::Mesh(m) => {
                if c.x + r < m.min.x || c.x - r > m.max.x || c.y + r < m.min.y || c.y - r > m.max.y || c.z + r < m.min.z || c.z - r > m.max.z {
                    return None;
                }
                let mut best = None::<(f32, Vec3)>;
                for t in &m.tris {
                    let tmin = t[0].min(t[1]).min(t[2]);
                    let tmax = t[0].max(t[1]).max(t[2]);
                    if c.x + r < tmin.x || c.x - r > tmax.x || c.y + r < tmin.y || c.y - r > tmax.y || c.z + r < tmin.z || c.z - r > tmax.z {
                        continue;
                    }
                    let q = closest_on_tri(c, t);
                    let dist = c.distance_squared(q);
                    if dist < r * r && best.is_none_or(|(d, _)| dist < d) {
                        best = Some((dist, q));
                    }
                }
                best.and_then(|(_, q)| push_from_point(c, r, q, 0.0))
            }
        }
    }

    /// The collider under a further transform (a prop moving it). Only boxes move as props today.
    pub fn transformed(&self, m: Mat4) -> Self {
        match self {
            Collider::Box(b) => Collider::Box(b.transformed(m)),
            Collider::Sphere { center, radius } => Collider::Sphere { center: m.transform_point3(*center), radius: *radius },
            Collider::Capsule { a, b, r } => Collider::Capsule { a: m.transform_point3(*a), b: m.transform_point3(*b), r: *r },
            Collider::Mesh(mesh) => Collider::Mesh(MeshShape::new(mesh.tris.iter().map(|t| t.map(|v| m.transform_point3(v))).collect())),
        }
    }
}

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

    /// The box under a further transform (a prop moving it); a scale stretches the half sizes along the axes.
    pub fn transformed(&self, m: Mat4) -> Self {
        let mut axes = self.axes;
        let mut half = self.half;
        for i in 0..3 {
            let a = m.transform_vector3(self.axes[i]);
            half[i] *= a.length();
            axes[i] = a.normalize_or_zero();
        }
        Obb { center: m.transform_point3(self.center), axes, half }
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

/// Whether a sphere touches none of the colliders.
pub fn sphere_clear(boxes: &[Collider], c: Vec3, r: f32) -> bool {
    boxes.iter().all(|b| b.push(c, r).is_none())
}

/// Moves feet by `motion` through the colliders. Returns the new feet position and whether it stands on something.
/// A long move (a slow frame, a fast fall) goes in steps of at most half the radius, so it cannot pass through a
/// thin box.
pub fn move_capsule(boxes: &[Collider], feet: Vec3, motion: Vec3) -> (Vec3, bool) {
    let steps = (motion.length() / (RADIUS * 0.5)).ceil().max(1.0) as usize;
    let (mut p, mut grounded) = (feet, false);
    for _ in 0..steps {
        let (q, g) = move_step(boxes, p, motion / steps as f32);
        (p, grounded) = (q, grounded || g);
    }
    (p, grounded)
}

fn move_step(boxes: &[Collider], feet: Vec3, motion: Vec3) -> (Vec3, bool) {
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

    fn cube(center: Vec3, size: Vec3) -> Collider {
        Collider::Box(Obb { center, axes: [Vec3::X, Vec3::Y, Vec3::Z], half: size * 0.5 })
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

    #[test]
    fn stands_on_a_mesh_floor() {
        // two triangles making a 10x10 floor at y=0; drop onto it
        let floor = Collider::Mesh(MeshShape::new(vec![
            [Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, 5.0)],
            [Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, 5.0), Vec3::new(-5.0, 0.0, 5.0)],
        ]));
        let (p, grounded) = move_capsule(&[floor], Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, -5.0, 0.0));
        assert!(grounded && p.y.abs() < 1e-2, "{p}");
    }

    #[test]
    fn stops_at_a_sphere_and_capsule() {
        // a sphere blocks sideways motion
        let floor = cube(Vec3::new(0.0, -0.5, 0.0), Vec3::new(20.0, 1.0, 20.0));
        let ball = Collider::Sphere { center: Vec3::new(2.0, 0.25, 0.0), radius: 1.0 };
        let mut p = Vec3::ZERO;
        for _ in 0..80 {
            p = move_capsule(&[floor.clone(), ball.clone()], p, Vec3::new(0.05, 0.0, 0.0)).0;
        }
        assert!(p.x < 2.0 - 1.0, "should stop before the sphere: {p}");
        // a vertical capsule blocks too
        let post = Collider::Capsule { a: Vec3::new(2.0, 0.0, 0.0), b: Vec3::new(2.0, 2.0, 0.0), r: 0.5 };
        let mut q = Vec3::ZERO;
        for _ in 0..80 {
            q = move_capsule(&[floor.clone(), post.clone()], q, Vec3::new(0.05, 0.0, 0.0)).0;
        }
        assert!(q.x < 2.0 - 0.5, "should stop before the capsule: {q}");
    }

    #[test]
    fn a_box_follows_its_prop() {
        let b = Obb::from_box_collider(Mat4::IDENTITY, [0.0, 0.5, 0.0], [1.0, 1.0, 1.0]);
        let m = Mat4::from_scale_rotation_translation(Vec3::splat(2.0), bevy::math::Quat::from_rotation_y(0.5), Vec3::new(3.0, 0.0, 0.0));
        let t = b.transformed(m);
        assert!((t.center - Vec3::new(3.0, 1.0, 0.0)).length() < 1e-5);
        assert!((t.half - Vec3::ONE).length() < 1e-5);
        // round trip
        let back = t.transformed(m.inverse());
        assert!((back.center - b.center).length() < 1e-5 && (back.half - b.half).length() < 1e-5);
    }
}
