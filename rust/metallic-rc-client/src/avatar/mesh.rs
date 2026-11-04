//! The avatar's meshes, built in code: smooth parametric surfaces (rounded boxes, the pebble head, lathed bodies and
//! limbs, a squircle patch that lies on the head for the face screen) with welded, area-weighted normals, and the
//! inflated inside-out "hull" copy of a mesh that draws its outline.
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};

/// The head shell's half size, roundness (1 ellipsoid, toward 0 a box) and taper (how much narrower the chin is than
/// the crown).
pub const HEAD_HALF: Vec3 = Vec3::new(0.205, 0.172, 0.188);
const HEAD_ROUND: f32 = 0.62;
const HEAD_TAPER: f32 = 0.1;

/// A mesh from a grid of points `f(u, v)`, u and v over 0..=1 in `nu` and `nv` steps. Normals are averaged over the
/// faces round each point (points at the same place, as on a seam or a pole, share them), and the faces are turned
/// to face away from `inside`.
pub fn grid(nu: usize, nv: usize, inside: Vec3, f: impl Fn(f32, f32) -> Vec3) -> Mesh {
    let mut ps = Vec::with_capacity((nu + 1) * (nv + 1));
    for j in 0..=nv {
        for i in 0..=nu {
            ps.push(f(i as f32 / nu as f32, j as f32 / nv as f32));
        }
    }
    let row = nu as u32 + 1;
    let mut idx = Vec::with_capacity(nu * nv * 6);
    for j in 0..nv as u32 {
        for i in 0..nu as u32 {
            let (a, c) = (j * row + i, (j + 1) * row + i);
            idx.extend([a, c, a + 1, a + 1, c, c + 1]);
        }
    }
    // the sum of each face's area-weighted normal against its distance from the inside point is three times the
    // volume: positive when the faces look outward, whatever the shape
    let outward: f32 = idx.chunks(3).map(|t| {
        let [a, b, c] = [ps[t[0] as usize], ps[t[1] as usize], ps[t[2] as usize]];
        (b - a).cross(c - a).dot((a + b + c) / 3.0 - inside)
    }).sum();
    if outward < 0.0 {
        idx.chunks_mut(3).for_each(|t| t.swap(1, 2));
    }
    let normals = smooth_normals(&ps, &idx);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, ps.iter().map(|p| p.to_array()).collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(idx))
}

fn smooth_normals(ps: &[Vec3], idx: &[u32]) -> Vec<[f32; 3]> {
    let key = |p: Vec3| (p * 1e5).round().as_ivec3();
    let mut sum: HashMap<IVec3, Vec3> = HashMap::new();
    for t in idx.chunks(3) {
        let [a, b, c] = [ps[t[0] as usize], ps[t[1] as usize], ps[t[2] as usize]];
        let n = (b - a).cross(c - a);
        for p in [a, b, c] {
            *sum.entry(key(p)).or_default() += n;
        }
    }
    ps.iter().map(|&p| sum[&key(p)].normalize_or(Vec3::Y).to_array()).collect()
}

/// The direction at longitude `lon` (0 straight ahead, -z; positive to the avatar's right, +x) and latitude `lat`
/// (positive up).
pub fn dir(lon: f32, lat: f32) -> Vec3 {
    // exactly 0 at the poles (cos(pi/2) is not), so the pole's points meet and share one normal
    let c = if lat.cos().abs() < 1e-6 { 0.0 } else { lat.cos() };
    Vec3::new(c * lon.sin(), lat.sin(), -c * lon.cos())
}

/// The point of a rounded box (superellipsoid) of half size `half` in direction `d`.
pub fn squash(d: Vec3, half: Vec3, round: f32) -> Vec3 {
    Vec3::from_array(std::array::from_fn(|i| d[i].signum() * d[i].abs().powf(round) * half[i]))
}

/// A whole surface round the origin, `at(direction)` giving its point.
fn closed(nu: usize, nv: usize, at: impl Fn(Vec3) -> Vec3) -> Mesh {
    grid(nu, nv, Vec3::ZERO, |u, v| at(dir(u * TAU, (v - 0.5) * PI)))
}

/// A rounded box of size `size`; `round` 1 is the ellipsoid, toward 0 the box.
pub fn rounded_box(size: Vec3, round: f32) -> Mesh {
    closed(48, 32, |d| squash(d, size / 2.0, round))
}

/// The head shell's point in direction `d` (centred on the origin): a rounded box a little narrower at the chin.
pub fn head_point(d: Vec3) -> Vec3 {
    let p = squash(d, HEAD_HALF, HEAD_ROUND);
    p * Vec3::new(1.0 - HEAD_TAPER * 0.5 * (1.0 - p.y / HEAD_HALF.y), 1.0, 1.0)
}

pub fn head() -> Mesh {
    closed(64, 44, head_point)
}

/// The point on the head's surface at `lon`, `lat`, `out` metres along the surface normal, and that normal.
pub fn on_head(lon: f32, lat: f32, out: f32) -> (Vec3, Vec3) {
    let e = 1e-3;
    let du = head_point(dir(lon + e, lat)) - head_point(dir(lon - e, lat));
    let dv = head_point(dir(lon, lat + e)) - head_point(dir(lon, lat - e));
    let p = head_point(dir(lon, lat));
    let n = du.cross(dv).normalize_or(p.normalize());
    let n = if n.dot(p) < 0.0 { -n } else { n };
    (p + n * out, n)
}

/// A rounded-rectangle (squircle) patch lying on the head `out` metres above it, centred at `lat0`, reaching `lon`
/// to each side and `lat` up and down: the face screen.
pub fn head_patch(lon: f32, lat0: f32, lat: f32, out: f32) -> Mesh {
    grid(72, 12, Vec3::ZERO, |u, v| {
        let (s, c) = (u * TAU).sin_cos();
        // |x|^4 + |y|^4 = 1, scaled by v (0 the middle, 1 the edge)
        let (x, y) = (c.signum() * c.abs().sqrt() * v, s.signum() * s.abs().sqrt() * v);
        on_head(x * lon, lat0 + y * lat, out).0
    })
}

/// The top of the head from latitude `lat` up, `out` metres above it: a cap or hat crown.
pub fn head_cap(lat: f32, out: f32) -> Mesh {
    grid(64, 14, Vec3::ZERO, |u, v| on_head(u * TAU, lat + v * (PI / 2.0 - lat), out).0)
}

/// A tube of radius `thick` all round the head at latitude `lat`, `out` metres above it: a hat band or a strap.
pub fn head_ring(lat: f32, out: f32, thick: f32) -> Mesh {
    grid(96, 12, Vec3::Y * on_head(0.0, lat, out).0.y, |u, v| {
        let lon = u * TAU;
        let (c, n) = on_head(lon, lat, out);
        let along = (on_head(lon + 1e-3, lat, out).0 - c).normalize();
        let side = along.cross(n).normalize();
        let (s, k) = (v * TAU).sin_cos();
        c + (n * k + side * s) * thick
    })
}

/// A body of revolution round the y axis through the points `(radius, height)` of `profile` (bottom to top; ends at
/// radius 0 close it), smoothed between them; `depth` scales it front to back.
pub fn lathe(profile: &[Vec2], depth: f32) -> Mesh {
    let n = profile.len();
    let at = |i: isize| profile[i.clamp(0, n as isize - 1) as usize];
    let steps = 6;
    let mut curve = vec![];
    for i in 0..n - 1 {
        let [p0, p1, p2, p3] = [-1, 0, 1, 2].map(|k| at(i as isize + k));
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            // Catmull-Rom through the points
            let (t2, t3) = (t * t, t * t * t);
            curve.push(0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3));
        }
    }
    curve.push(profile[n - 1]);
    let along = curve.len() - 1;
    let middle = profile.iter().map(|p| p.y).sum::<f32>() / n as f32;
    grid(48, along, Vec3::Y * middle, |u, v| {
        let p = curve[(v * along as f32).round() as usize];
        let (s, c) = (u * TAU).sin_cos();
        Vec3::new(p.x.max(0.0) * c, p.y, p.x.max(0.0) * s * depth)
    })
}

/// A tube of radius `thick` bent along a circle of radius `radius` over `angle` radians in the xy plane, bulging up
/// (an arch), its ends rounded, centred on the origin: a happy eye, or upside down a smile.
pub fn arc(radius: f32, thick: f32, angle: f32) -> Mesh {
    // the caps take this share of the length parameter at each end
    let k = 0.25;
    let lift = radius * (1.0 + (angle / 2.0).cos()) / 2.0;
    grid(48, 12, Vec3::ZERO, |u, v| {
        let s = u * (2.0 + 2.0 * k) - (1.0 + k);
        let th = s.clamp(-1.0, 1.0) * angle / 2.0;
        let (sn, cs) = th.sin_cos();
        let (centre, out, along) = (Vec3::new(sn, cs, 0.0) * radius, Vec3::new(sn, cs, 0.0), Vec3::new(cs, -sn, 0.0) * s.signum());
        // round the tube, then past the ends a half ball closing it
        let cap = ((s.abs() - 1.0).max(0.0) / k).min(1.0) * std::f32::consts::FRAC_PI_2;
        let (ps, pc) = (v * TAU).sin_cos();
        centre - Vec3::Y * lift + (out * pc + Vec3::Z * ps) * thick * cap.cos() + along * thick * cap.sin()
    })
}

/// The mesh pushed out `width` along its normals: drawn inside out (front faces culled) in a dark colour it shows
/// only as a rim round the original, the outline.
pub fn hull(mesh: &Mesh, width: f32) -> Mesh {
    let mut m = mesh.clone();
    let (Some(VertexAttributeValues::Float32x3(ps)), Some(VertexAttributeValues::Float32x3(ns))) =
        (m.attribute(Mesh::ATTRIBUTE_POSITION), m.attribute(Mesh::ATTRIBUTE_NORMAL)) else { return m };
    let pushed: Vec<[f32; 3]> = ps.iter().zip(ns).map(|(p, n)| (Vec3::from(*p) + Vec3::from(*n) * width).to_array()).collect();
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pushed);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(m: &Mesh) -> Vec<(Vec3, Vec3)> {
        let (Some(VertexAttributeValues::Float32x3(ps)), Some(VertexAttributeValues::Float32x3(ns))) =
            (m.attribute(Mesh::ATTRIBUTE_POSITION), m.attribute(Mesh::ATTRIBUTE_NORMAL)) else { panic!() };
        ps.iter().zip(ns).map(|(p, n)| (Vec3::from(*p), Vec3::from(*n))).collect()
    }

    /// Every normal is unit length and points away from `inside`; faces wind the same way (the mesh's own face
    /// normals agree with its vertex normals).
    fn outward(m: &Mesh, inside: impl Fn(Vec3) -> Vec3) {
        let pts = points(m);
        for &(p, n) in &pts {
            assert!((n.length() - 1.0).abs() < 1e-3, "{n}");
            let away = p - inside(p);
            assert!(away.length() < 1e-4 || n.dot(away) > -1e-3, "{p} {n}");
        }
        let Some(Indices::U32(idx)) = m.indices() else { panic!() };
        for t in idx.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| pts[t[k] as usize]);
            let face = (b.0 - a.0).cross(c.0 - a.0);
            if face.length() > 1e-9 {
                assert!(face.dot(a.1 + b.1 + c.1) > 0.0, "{t:?}");
            }
        }
    }

    #[test]
    fn closed_shapes_face_outward_with_smooth_normals() {
        outward(&rounded_box(Vec3::new(0.2, 0.1, 0.3), 0.5), |_| Vec3::ZERO);
        outward(&head(), |_| Vec3::ZERO);
        let body = lathe(&[Vec2::new(0.0, 0.0), Vec2::new(0.2, 0.1), Vec2::new(0.15, 0.5), Vec2::new(0.0, 0.55)], 0.7);
        outward(&body, |p| Vec3::Y * p.y);
        // the lathe keeps to its profile: as wide as its widest point, as tall as its ends
        let pts = points(&body);
        let (lo, hi) = pts.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), (p, _)| (lo.min(*p), hi.max(*p)));
        assert!((lo.y - 0.0).abs() < 1e-5 && (hi.y - 0.55).abs() < 1e-5, "{lo} {hi}");
        assert!(hi.x > 0.199 && hi.x < 0.215 && (hi.z / hi.x - 0.7).abs() < 0.02, "{hi}");
    }

    #[test]
    fn arcs_are_closed_arches_round_the_origin() {
        let m = arc(0.03, 0.006, 2.0);
        outward(&m, |p| {
            // the nearest point of the bent centre line
            let q = p + Vec3::Y * 0.03 * (1.0 + 1f32.cos()) / 2.0;
            let th = q.x.atan2(q.y).clamp(-1.0, 1.0);
            Vec3::new(th.sin(), th.cos(), 0.0) * 0.03 - Vec3::Y * 0.03 * (1.0 + 1f32.cos()) / 2.0
        });
        let pts = points(&m);
        let (lo, hi) = pts.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), (p, _)| (lo.min(*p), hi.max(*p)));
        // an arch: the top in the middle, the ends lower and to the sides; about centred
        assert!((hi.y + lo.y).abs() < 0.008 && (hi.x + lo.x).abs() < 1e-4 && (hi.z - 0.006).abs() < 1e-4, "{lo} {hi}");
        let top = pts.iter().max_by(|a, b| a.0.y.total_cmp(&b.0.y)).unwrap().0;
        assert!(top.x.abs() < 0.003, "{top}");
    }

    #[test]
    fn the_hull_is_the_mesh_grown_by_its_width() {
        let m = rounded_box(Vec3::splat(0.2), 0.6);
        for ((p, n), (q, _)) in points(&m).into_iter().zip(points(&hull(&m, 0.01))) {
            assert!(((q - p) - n * 0.01).length() < 1e-6);
        }
    }

    #[test]
    fn the_face_lies_on_the_head() {
        // straight ahead is the middle of the front, its normal straight out
        let (p, n) = on_head(0.0, 0.0, 0.0);
        assert!((p - Vec3::new(0.0, 0.0, -HEAD_HALF.z)).length() < 1e-3 && n.dot(Vec3::NEG_Z) > 0.999, "{p} {n}");
        // off to the side and up, `out` is along the normal, which leans the same way
        let (p, n) = on_head(0.5, 0.2, 0.0);
        let (q, _) = on_head(0.5, 0.2, 0.01);
        assert!(((q - p) - n * 0.01).length() < 1e-6 && n.x > 0.1 && n.y > 0.0);
        // the chin is narrower than the crown
        assert!(head_point(dir(PI / 2.0, -0.3)).x < head_point(dir(PI / 2.0, 0.3)).x);
        let patch = head_patch(0.9, -0.1, 0.5, 0.004);
        outward(&patch, |_| Vec3::ZERO);
        for (p, n) in points(&patch) {
            assert!(p.z < -0.05 && p.x.abs() < HEAD_HALF.x && n.z < 0.0, "{p} {n}");
        }
    }
}
