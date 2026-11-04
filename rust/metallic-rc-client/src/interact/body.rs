//! A small rigid body for props that are let go: gravity, contacts against the room's boxes (and other props, taken
//! as still), impulses with friction and bounce at the contact points, and sleep once it rests. Boxes collide by their
//! corners, balls by their centre. INFERRED throughout (the game uses Unity's PhysX); the constants are Unity's
//! defaults where it has one (gravity 9.81, friction 0.6, sleep below ~0.05 m/s).
use bevy::math::{Mat3, Quat, Vec3};

use crate::collide::Obb;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// half extents in the prop's own space
    Box(Vec3),
    Ball(f32),
}

impl Shape {
    /// The radius of a ball that holds the whole shape.
    pub fn radius(&self) -> f32 {
        match *self {
            Shape::Box(h) => h.length(),
            Shape::Ball(r) => r,
        }
    }

    /// Inverse inertia in the shape's own space, for `mass` kg.
    fn inv_inertia(&self, mass: f32) -> Vec3 {
        let i = match *self {
            Shape::Box(h) => {
                let s = h * 2.0;
                Vec3::new(s.y * s.y + s.z * s.z, s.x * s.x + s.z * s.z, s.x * s.x + s.y * s.y) * mass / 12.0
            }
            Shape::Ball(r) => Vec3::splat(0.4 * mass * r * r),
        };
        Vec3::ONE / i.max(Vec3::splat(1e-6))
    }

    /// The box this shape fills at `pos`, `rot` (a ball's is its bounding cube).
    pub fn obb(&self, pos: Vec3, rot: Quat) -> Obb {
        let half = match *self {
            Shape::Box(h) => h,
            Shape::Ball(r) => Vec3::splat(r),
        };
        Obb { center: pos, axes: [rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z], half }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// angular velocity, radians per second about each world axis
    pub spin: Vec3,
    /// seconds it has been nearly still; asleep from `SLEEP_AFTER`
    pub still: f32,
}

pub const GRAVITY: f32 = 9.81;
const FRICTION: f32 = 0.6;
const SLEEP_SPEED: f32 = 0.06;
const SLEEP_AFTER: f32 = 0.4;
/// per second
const LINEAR_DAMPING: f32 = 0.05;
const ANGULAR_DAMPING: f32 = 0.6;
/// steps longer than this are split
const MAX_STEP: f32 = 1.0 / 120.0;

fn closest(b: &Obb, p: Vec3) -> Vec3 {
    let d = p - b.center;
    (0..3).fold(b.center, |q, i| q + b.axes[i] * d.dot(b.axes[i]).clamp(-b.half[i], b.half[i]))
}

/// Where a ball of radius `r` at `c` touches box `b`: the contact point, the normal out of the box and the depth.
pub fn contact(b: &Obb, c: Vec3, r: f32) -> Option<(Vec3, Vec3, f32)> {
    let q = closest(b, c);
    let d = c - q;
    let dist = d.length();
    if dist >= r {
        return None;
    }
    if dist > 1e-6 {
        return Some((q, d / dist, r - dist));
    }
    // the centre is inside: out by the nearest face
    let local = c - b.center;
    let (i, depth) = (0..3).map(|i| (i, b.half[i] - local.dot(b.axes[i]).abs())).min_by(|a, b| a.1.total_cmp(&b.1))?;
    let n = b.axes[i] * local.dot(b.axes[i]).signum();
    Some((c, if n == Vec3::ZERO { Vec3::Y } else { n }, depth + r))
}

/// Where a ray from `from` along unit `dir` first enters box `b`, as a distance.
pub fn ray_box(b: &Obb, from: Vec3, dir: Vec3) -> Option<f32> {
    let (mut near, mut far) = (f32::NEG_INFINITY, f32::INFINITY);
    let p = b.center - from;
    for i in 0..3 {
        let (e, f) = (b.axes[i].dot(p), b.axes[i].dot(dir));
        if f.abs() > 1e-9 {
            let (t1, t2) = ((e + b.half[i]) / f, (e - b.half[i]) / f);
            near = near.max(t1.min(t2));
            far = far.min(t1.max(t2));
            if near > far || far < 0.0 {
                return None;
            }
        } else if e.abs() > b.half[i] {
            return None;
        }
    }
    Some(near.max(0.0))
}

/// Where a ray from `from` along unit `dir` first touches a ball at `c` of radius `r`.
pub fn ray_ball(c: Vec3, r: f32, from: Vec3, dir: Vec3) -> Option<f32> {
    let m = from - c;
    let b = m.dot(dir);
    let k = m.length_squared() - r * r;
    if k > 0.0 && b > 0.0 {
        return None;
    }
    let disc = b * b - k;
    (disc >= 0.0).then(|| (-b - disc.sqrt()).max(0.0))
}

impl Body {
    pub fn new(pos: Vec3, rot: Quat) -> Self {
        Body { pos, rot, vel: Vec3::ZERO, spin: Vec3::ZERO, still: 0.0 }
    }

    pub fn asleep(&self) -> bool {
        self.still >= SLEEP_AFTER
    }

    /// Wakes it (it was moved, hit or let go).
    pub fn wake(&mut self) {
        self.still = 0.0;
    }

    /// Advances `dt` seconds against `solids` (which do not move).
    pub fn step(&mut self, shape: &Shape, mass: f32, bounce: f32, solids: &[Obb], dt: f32) {
        if self.asleep() {
            return;
        }
        let n = (dt / MAX_STEP).ceil().max(1.0) as usize;
        for _ in 0..n {
            self.substep(shape, mass, bounce, solids, dt / n as f32);
        }
        let slow = self.vel.length() < SLEEP_SPEED && self.spin.length() < SLEEP_SPEED * 4.0;
        self.still = if slow { self.still + dt } else { 0.0 };
        if self.asleep() {
            (self.vel, self.spin) = (Vec3::ZERO, Vec3::ZERO);
        }
    }

    fn substep(&mut self, shape: &Shape, mass: f32, bounce: f32, solids: &[Obb], dt: f32) {
        self.vel.y -= GRAVITY * dt;
        self.vel *= 1.0 - LINEAR_DAMPING * dt;
        self.spin *= 1.0 - ANGULAR_DAMPING * dt;
        self.pos += self.vel * dt;
        let turn = self.spin * dt;
        if turn.length() > 1e-9 {
            self.rot = (Quat::from_scaled_axis(turn) * self.rot).normalize();
        }
        let r_world = Mat3::from_quat(self.rot);
        let inv_i = r_world * Mat3::from_diagonal(shape.inv_inertia(mass)) * r_world.transpose();
        let inv_m = 1.0 / mass;
        for b in solids {
            // the points that can touch: a box's corners (and its centre, for boxes much bigger than it), a ball's centre
            let points: Vec<(Vec3, f32)> = match *shape {
                Shape::Box(h) => {
                    let mut v: Vec<(Vec3, f32)> = (0..8)
                        .map(|i| Vec3::new(if i & 1 == 0 { -h.x } else { h.x }, if i & 2 == 0 { -h.y } else { h.y }, if i & 4 == 0 { -h.z } else { h.z }))
                        .map(|c| (self.rot * c, 0.0))
                        .collect();
                    v.push((Vec3::ZERO, h.min_element()));
                    v
                }
                Shape::Ball(r) => vec![(Vec3::ZERO, r)],
            };
            for (local, radius) in points {
                let Some((_, normal, depth)) = contact(b, self.pos + local, radius.max(1e-3)) else { continue };
                // out of the box first, then the velocity at the point
                self.pos += normal * depth;
                let r = local - normal * radius;
                let v = self.vel + self.spin.cross(r);
                let vn = v.dot(normal);
                if vn >= 0.0 {
                    continue;
                }
                let k = |dir: Vec3| inv_m + dir.dot((inv_i * r.cross(dir)).cross(r));
                let e = if vn < -0.5 { bounce } else { 0.0 };
                let j = -(1.0 + e) * vn / k(normal);
                self.vel += normal * j * inv_m;
                self.spin += inv_i * r.cross(normal * j);
                // friction against the sliding along the surface
                let v = self.vel + self.spin.cross(r);
                let slide = v - normal * v.dot(normal);
                if slide.length() > 1e-6 {
                    let t = slide.normalize();
                    let jt = (-v.dot(t) / k(t)).clamp(-FRICTION * j, FRICTION * j);
                    self.vel += t * jt * inv_m;
                    self.spin += inv_i * r.cross(t * jt);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> Obb {
        Obb { center: Vec3::new(0.0, -0.5, 0.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::new(10.0, 0.5, 10.0) }
    }

    fn settle(b: &mut Body, shape: Shape, solids: &[Obb], secs: f32) {
        for _ in 0..(secs * 60.0) as usize {
            b.step(&shape, 1.0, 0.0, solids, 1.0 / 60.0);
        }
    }

    #[test]
    fn a_dropped_box_lands_flat_and_sleeps() {
        let shape = Shape::Box(Vec3::splat(0.2));
        let mut b = Body::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY);
        settle(&mut b, shape, &[floor()], 3.0);
        assert!((b.pos.y - 0.2).abs() < 0.02, "{}", b.pos);
        assert!(b.asleep());
    }

    #[test]
    fn a_tilted_box_falls_over_onto_a_face() {
        let shape = Shape::Box(Vec3::new(0.3, 0.1, 0.2));
        let mut b = Body::new(Vec3::new(0.0, 0.8, 0.0), Quat::from_rotation_z(0.6) * Quat::from_rotation_x(0.3));
        settle(&mut b, shape, &[floor()], 5.0);
        // resting on its largest face: 0.1 m up, and its y axis (thinnest) vertical
        assert!((b.pos.y - 0.1).abs() < 0.03, "{}", b.pos);
        assert!((b.rot * Vec3::Y).y.abs() > 0.97, "{}", b.rot * Vec3::Y);
    }

    #[test]
    fn a_thrown_ball_bounces_off_a_wall_and_rolls_on_the_floor() {
        let wall = Obb { center: Vec3::new(3.0, 1.0, 0.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::new(0.1, 2.0, 5.0) };
        let shape = Shape::Ball(0.12);
        let mut b = Body::new(Vec3::new(0.0, 1.2, 0.0), Quat::IDENTITY);
        b.vel = Vec3::new(8.0, 0.0, 0.0);
        let mut top = 0f32;
        let big = Obb { half: Vec3::new(100.0, 0.5, 100.0), ..floor() };
        for _ in 0..600 {
            b.step(&shape, 0.5, 0.5, &[big.clone(), wall.clone()], 1.0 / 60.0);
            top = top.max(b.pos.x);
        }
        // it never passes into the wall, comes back off it, and ends on the floor
        assert!(top <= 2.9 - 0.12 + 0.02, "{top}");
        assert!(b.pos.x < 2.0 && (b.pos.y - 0.12).abs() < 0.02, "{}", b.pos);
    }

    #[test]
    fn rays_find_boxes_and_balls() {
        let b = Obb { center: Vec3::new(0.0, 0.0, -3.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::splat(0.5) };
        assert!((ray_box(&b, Vec3::ZERO, Vec3::NEG_Z).unwrap() - 2.5).abs() < 1e-5);
        assert_eq!(ray_box(&b, Vec3::ZERO, Vec3::Z), None);
        assert_eq!(ray_box(&b, Vec3::new(1.0, 0.0, 0.0), Vec3::NEG_Z), None);
        assert!((ray_ball(Vec3::new(0.0, 0.0, -2.0), 0.5, Vec3::ZERO, Vec3::NEG_Z).unwrap() - 1.5).abs() < 1e-5);
        assert_eq!(ray_ball(Vec3::new(0.0, 0.0, -2.0), 0.5, Vec3::ZERO, Vec3::X), None);
        let (_, n, d) = contact(&b, Vec3::new(0.0, 0.0, -2.45), 0.1).unwrap();
        assert!((n - Vec3::Z).length() < 1e-5 && (d - 0.05).abs() < 1e-5);
    }
}
