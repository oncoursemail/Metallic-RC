//! The pen's maths and memory, without Bevy systems: undo/redo, grid snapping, rays against boxes.
use bevy::prelude::*;

use crate::collide::Obb;

/// The grid the snap puts positions on (INFERRED: the game's Maker Pen grid is configurable; 0.25 m is ours).
pub const GRID: f32 = 0.25;
/// Rotation steps of the wheel: with snap / without.
pub const TURN_SNAP: f32 = 15.0;
pub const TURN_FREE: f32 = 5.0;
/// Size steps of the wheel and the limits of a maker object's scale.
pub const SCALE_STEP: f32 = 1.1;
pub const SCALE_MIN: f32 = 0.05;
pub const SCALE_MAX: f32 = 40.0;
const HISTORY: usize = 100;

/// A maker object as it stands: its address (with its uid) and its Transform.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjState {
    pub address: String,
    pub at: Transform,
}

/// One edit: object `uid` went from `before` to `after` (`None`: did not exist).
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub uid: u32,
    pub before: Option<ObjState>,
    pub after: Option<ObjState>,
}

#[derive(Default, Debug)]
pub struct History {
    done: Vec<Change>,
    undone: Vec<Change>,
}

impl History {
    pub fn push(&mut self, c: Change) {
        if c.before == c.after {
            return;
        }
        self.undone.clear();
        self.done.push(c);
        if self.done.len() > HISTORY {
            self.done.remove(0);
        }
    }

    /// The last edit, taken back: `(uid, state to put it in)`.
    pub fn undo(&mut self) -> Option<(u32, Option<ObjState>)> {
        let c = self.done.pop()?;
        let r = (c.uid, c.before.clone());
        self.undone.push(c);
        Some(r)
    }

    pub fn redo(&mut self) -> Option<(u32, Option<ObjState>)> {
        let c = self.undone.pop()?;
        let r = (c.uid, c.after.clone());
        self.done.push(c);
        Some(r)
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

pub fn snap_position(p: Vec3) -> Vec3 {
    (p / GRID).round() * GRID
}

/// The rotation with each Euler angle on the snap step.
pub fn snap_rotation(q: Quat) -> Quat {
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    let s = TURN_SNAP.to_radians();
    Quat::from_euler(EulerRot::YXZ, (y / s).round() * s, (x / s).round() * s, (z / s).round() * s)
}

pub fn clamp_scale(s: Vec3) -> Vec3 {
    s.clamp(Vec3::splat(SCALE_MIN), Vec3::splat(SCALE_MAX))
}

/// Where a ray first enters a box (distance along the unit `dir`), if it does.
pub fn ray_obb(b: &Obb, from: Vec3, dir: Vec3) -> Option<f32> {
    let d = b.center - from;
    let (mut near, mut far) = (f32::NEG_INFINITY, f32::INFINITY);
    for i in 0..3 {
        let (e, f) = (b.axes[i].dot(d), b.axes[i].dot(dir));
        if f.abs() < 1e-8 {
            if e.abs() > b.half[i] {
                return None;
            }
            continue;
        }
        let (t1, t2) = ((e - b.half[i]) / f, (e + b.half[i]) / f);
        near = near.max(t1.min(t2));
        far = far.min(t1.max(t2));
        if near > far || far < 0.0 {
            return None;
        }
    }
    Some(near.max(0.0))
}

/// The nearest hit of a ray among boxes.
pub fn ray_boxes(boxes: &[Obb], from: Vec3, dir: Vec3) -> Option<f32> {
    boxes.iter().filter_map(|b| ray_obb(b, from, dir)).min_by(|a, b| a.total_cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(x: f32) -> Option<ObjState> {
        Some(ObjState { address: "maker/cube/1/9#m1".into(), at: Transform::from_xyz(x, 0.0, 0.0) })
    }

    #[test]
    fn undo_and_redo_walk_the_edits() {
        let mut h = History::default();
        h.push(Change { uid: 1, before: None, after: st(0.0) });
        h.push(Change { uid: 1, before: st(0.0), after: st(2.0) });
        h.push(Change { uid: 1, before: st(2.0), after: st(2.0) }); // no change: not kept
        assert_eq!(h.undo(), Some((1, st(0.0))));
        assert_eq!(h.undo(), Some((1, None)));
        assert_eq!(h.undo(), None);
        assert_eq!(h.redo(), Some((1, st(0.0))));
        // a new edit drops what was undone
        h.push(Change { uid: 2, before: None, after: st(5.0) });
        assert_eq!(h.redo(), None);
        assert_eq!(h.undo().map(|u| u.0), Some(2));
    }

    #[test]
    fn snapping_rounds_to_the_grid_and_steps() {
        assert_eq!(snap_position(Vec3::new(0.13, 1.37, -0.6)), Vec3::new(0.25, 1.25, -0.5));
        let q = snap_rotation(Quat::from_rotation_y(17f32.to_radians()));
        assert!(q.angle_between(Quat::from_rotation_y(15f32.to_radians())) < 1e-3);
        assert_eq!(clamp_scale(Vec3::new(0.0, 1.0, 100.0)), Vec3::new(SCALE_MIN, 1.0, SCALE_MAX));
    }

    #[test]
    fn rays_hit_turned_boxes() {
        let b = Obb { center: Vec3::new(0.0, 0.0, -5.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::splat(0.5) };
        assert!((ray_obb(&b, Vec3::ZERO, Vec3::NEG_Z).unwrap() - 4.5).abs() < 1e-5);
        assert_eq!(ray_obb(&b, Vec3::ZERO, Vec3::Z), None);
        assert_eq!(ray_obb(&b, Vec3::new(2.0, 0.0, 0.0), Vec3::NEG_Z), None);
        // turned 45 degrees: the corner is nearer
        let r = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let t = Obb { axes: [r * Vec3::X, r * Vec3::Y, r * Vec3::Z], ..b.clone() };
        assert!((ray_obb(&t, Vec3::ZERO, Vec3::NEG_Z).unwrap() - (5.0 - 0.5 * 2f32.sqrt())).abs() < 1e-4);
        // from inside: 0
        assert_eq!(ray_obb(&b, Vec3::new(0.0, 0.0, -5.0), Vec3::X), Some(0.0));
        assert_eq!(ray_boxes(&[t, b], Vec3::ZERO, Vec3::NEG_Z).map(|d| d < 4.5), Some(true));
    }
}
