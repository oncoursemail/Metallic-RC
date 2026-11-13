//! The robot's accessories (on the head) and outfit pieces (on the body), our own designs. Each is a few smooth
//! parts in the avatar's colours, placed on the head's surface (`mesh::on_head`) or round the body; some have a part
//! that moves (`wiggle`): the antenna and the sprout sway with the stride, the halo turns, the scarf's tail swings.
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use super::{anim::{Anim, Pose}, mesh, part, pivot, Kit, Look, Mat, Mats, Shape, Zone, FINE};
use super::look::{Accessory, Outfit};

/// Where the antenna stands on the head (longitude, latitude), and the halo's height over the head's middle.
const ANTENNA: (f32, f32) = (0.5, 1.12);
const HALO: f32 = 0.27;

/// The accessories' meshes.
pub struct Shapes {
    stalk: Shape,
    tip: Shape,
    socket: Shape,
    band: Shape,
    cup: Shape,
    halo: Shape,
    cap: Shape,
    cap_band: Shape,
    brim: Shape,
    button: Shape,
    beanie: Shape,
    cuff: Shape,
    pom: Shape,
    ear: Shape,
    stem: Shape,
    leaf: Shape,
    strap: Shape,
    lens: Shape,
    rim: Shape,
    scarf: Shape,
    tail: Shape,
    pack: Shape,
    flap: Shape,
    arch: Shape,
    wing: Shape,
    knot: Shape,
}

impl Shapes {
    pub(super) fn make(shape: &mut impl FnMut(Mesh, f32) -> Shape) -> Shapes {
        let v2 = Vec2::new;
        let upright = |m: Mesh| m.rotated_by(Quat::from_rotation_x(-FRAC_PI_2));
        let half_ring = |inner: f32, outer: f32| upright(Torus::new(inner, outer).mesh().angle_range(0.0..=PI).build());
        Shapes {
            stalk: shape(Capsule3d::new(0.008, 0.11).into(), FINE),
            tip: shape(Sphere::new(0.026).mesh().uv(20, 12), 0.0),
            socket: shape(mesh::rounded_box(Vec3::new(0.06, 0.03, 0.06), 0.6), FINE),
            band: shape(half_ring(0.212, 0.232), FINE),
            cup: shape(mesh::rounded_box(Vec3::new(0.055, 0.12, 0.12), 0.7), FINE),
            halo: shape(Torus::new(0.105, 0.13).into(), 0.0),
            cap: shape(mesh::head_cap(0.5, 0.012), FINE),
            cap_band: shape(mesh::head_ring(0.5, 0.012, 0.011), FINE),
            brim: shape(mesh::rounded_box(Vec3::new(0.25, 0.018, 0.14), 0.45), FINE),
            button: shape(Sphere::new(0.017).mesh().uv(16, 10), FINE),
            beanie: shape(mesh::head_cap(0.36, 0.018), FINE),
            cuff: shape(mesh::head_ring(0.4, 0.02, 0.021), FINE),
            pom: shape(Sphere::new(0.042).mesh().uv(20, 12), FINE),
            ear: shape(mesh::lathe(&[v2(0.0, 0.0), v2(0.056, 0.004), v2(0.053, 0.045), v2(0.03, 0.085), v2(0.0, 0.095)], 0.45), FINE),
            stem: shape(Capsule3d::new(0.007, 0.05).into(), FINE),
            leaf: shape(mesh::rounded_box(Vec3::new(0.08, 0.012, 0.042), 0.85), FINE),
            strap: shape(mesh::head_ring(0.56, 0.006, 0.012), FINE),
            lens: shape(mesh::rounded_box(Vec3::new(0.072, 0.072, 0.028), 0.95), 0.0),
            rim: shape(Torus::new(0.034, 0.046).into(), FINE),
            scarf: shape(Torus::new(0.1, 0.16).into(), FINE),
            tail: shape(mesh::rounded_box(Vec3::new(0.075, 0.2, 0.03), 0.6), FINE),
            pack: shape(mesh::rounded_box(Vec3::new(0.25, 0.29, 0.13), 0.55), FINE),
            flap: shape(mesh::rounded_box(Vec3::new(0.235, 0.09, 0.06), 0.6), FINE),
            arch: shape(half_ring(0.158, 0.18), FINE),
            wing: shape(mesh::lathe(&[v2(0.0, 0.0), v2(0.026, 0.01), v2(0.036, 0.045), v2(0.0, 0.062)], 0.45), FINE),
            knot: shape(Sphere::new(0.019).mesh().uv(16, 10), FINE),
        }
    }
}

/// A transform at the head's surface point (`lon`, `lat`, `out` above it) with +y along the surface normal.
fn on_head(lon: f32, lat: f32, out: f32) -> Transform {
    let (at, n) = mesh::on_head(lon, lat, out);
    Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::Y, n))
}

/// Puts accessory `a` on the head's holder `hat` (centred on the head shell); returns its moving part.
pub(super) fn head(c: &mut Commands, kit: &Kit, mats: &Mats, hat: Entity, a: Accessory) -> Option<Entity> {
    let g = &kit.gear;
    let z = |zone| mats.of(zone);
    let joint = Mat::Toon(kit.joint.clone());
    let put = |c: &mut Commands, s: &Shape, m: &Mat, t: Transform| part(c, kit, hat, s, m, t);
    match a {
        Accessory::None => None,
        Accessory::Antenna => {
            let base = on_head(ANTENNA.0, ANTENNA.1, 0.0);
            put(c, &g.socket, &joint, base);
            // the stalk sways about its foot; the glowing tip rides on it
            let stalk = pivot(c, hat, base);
            part(c, kit, stalk, &g.stalk, &joint, Transform::from_xyz(0.0, 0.07, 0.0));
            part(c, kit, stalk, &g.tip, &z(Zone::Glow), Transform::from_xyz(0.0, 0.145, 0.0));
            Some(stalk)
        }
        Accessory::Headphones => {
            put(c, &g.band, &joint, Transform::from_xyz(0.0, -0.015, 0.01));
            for s in [-1.0f32, 1.0] {
                put(c, &g.cup, &z(Zone::Pants), Transform::from_xyz(s * 0.222, -0.015, 0.01));
                put(c, &kit.ring, &z(Zone::Glow), Transform::from_xyz(s * 0.252, -0.015, 0.01).with_rotation(Quat::from_rotation_z(-s * FRAC_PI_2)));
            }
            None
        }
        Accessory::Halo => Some(put(c, &g.halo, &z(Zone::Glow), Transform::from_xyz(0.0, HALO, 0.0))),
        Accessory::Cap => {
            put(c, &g.cap, &z(Zone::Shirt), Transform::default());
            put(c, &g.cap_band, &z(Zone::Shirt), Transform::default());
            let front = on_head(0.0, 0.5, 0.012);
            put(c, &g.brim, &z(Zone::Shirt), Transform::from_translation(front.translation + Vec3::new(0.0, -0.004, -0.055))
                .with_rotation(Quat::from_rotation_x(-0.12)));
            put(c, &g.button, &z(Zone::Pants), on_head(0.0, FRAC_PI_2, 0.022));
            None
        }
        Accessory::Beanie => {
            put(c, &g.beanie, &z(Zone::Shirt), Transform::default());
            put(c, &g.cuff, &z(Zone::Pants), Transform::default());
            put(c, &g.pom, &z(Zone::Pants), on_head(0.0, FRAC_PI_2, 0.05));
            None
        }
        Accessory::Ears => {
            for s in [-1.0f32, 1.0] {
                let at = on_head(s * 0.62, 0.92, -0.008);
                let t = at.with_rotation(at.rotation * Quat::from_rotation_z(-s * 0.15));
                put(c, &g.ear, &z(Zone::Skin), t);
                // the inside of the ear glows
                put(c, &g.ear, &z(Zone::Glow), Transform::from_translation(t.translation + t.rotation * Vec3::new(0.0, 0.012, -0.012))
                    .with_rotation(t.rotation).with_scale(Vec3::new(0.58, 0.68, 0.5)));
            }
            None
        }
        Accessory::Sprout => {
            let leaf = Mat::Toon(kit.leaf.clone());
            let stem = pivot(c, hat, on_head(0.0, FRAC_PI_2, 0.0));
            part(c, kit, stem, &g.stem, &leaf, Transform::from_xyz(0.0, 0.03, 0.0));
            for s in [-1.0f32, 1.0] {
                part(c, kit, stem, &g.leaf, &leaf, Transform::from_xyz(s * 0.038, 0.072 + 0.006 * s, 0.0)
                    .with_rotation(Quat::from_rotation_y(0.4 * s) * Quat::from_rotation_z(s * 0.45)));
            }
            Some(stem)
        }
        Accessory::Goggles => {
            put(c, &g.strap, &joint, Transform::default());
            for s in [-1.0f32, 1.0] {
                let at = on_head(s * 0.27, 0.6, 0.016);
                // lenses face out from the forehead: the shapes' depth along the surface normal
                let facing = at.rotation * Quat::from_rotation_x(FRAC_PI_2);
                put(c, &g.rim, &z(Zone::Pants), at);
                put(c, &g.lens, &z(Zone::Screen), Transform::from_translation(at.translation).with_rotation(facing));
                put(c, &kit.ball, &Mat::Plain(kit.white.clone()), Transform::from_translation(at.translation + at.rotation * Vec3::new(0.014, 0.012, -0.016))
                    .with_scale(Vec3::splat(0.007)));
            }
            None
        }
    }
}

/// Puts outfit piece `o` on the body's holder `wear` (at the hips, turning with the body); returns its moving part.
pub(super) fn body(c: &mut Commands, kit: &Kit, mats: &Mats, wear: Entity, o: Outfit) -> Option<Entity> {
    let g = &kit.gear;
    let z = |zone| mats.of(zone);
    let put = |c: &mut Commands, s: &Shape, m: &Mat, t: Transform| part(c, kit, wear, s, m, t);
    match o {
        Outfit::None => None,
        Outfit::Scarf => {
            put(c, &g.scarf, &z(Zone::Cloth), Transform::from_xyz(0.0, 0.61, 0.0).with_rotation(Quat::from_rotation_x(0.12))
                .with_scale(Vec3::new(1.0, 1.0, 0.9)));
            let tail = pivot(c, wear, Transform::from_xyz(0.07, 0.59, -0.135));
            part(c, kit, tail, &g.tail, &z(Zone::Cloth), Transform::from_xyz(0.0, -0.09, -0.012).with_rotation(Quat::from_rotation_z(0.08)));
            Some(tail)
        }
        Outfit::Backpack => {
            put(c, &g.pack, &z(Zone::Pants), Transform::from_xyz(0.0, 0.38, 0.21));
            put(c, &g.flap, &z(Zone::Skin), Transform::from_xyz(0.0, 0.49, 0.235).with_rotation(Quat::from_rotation_x(-0.15)));
            put(c, &kit.ball, &z(Zone::Glow), Transform::from_xyz(0.0, 0.34, 0.277).with_scale(Vec3::new(0.022, 0.022, 0.006)));
            for s in [-1.0f32, 1.0] {
                put(c, &g.arch, &z(Zone::Pants), Transform::from_xyz(s * 0.1, 0.5, 0.01).with_rotation(Quat::from_rotation_y(FRAC_PI_2)));
            }
            None
        }
        Outfit::Bowtie => {
            for s in [-1.0f32, 1.0] {
                put(c, &g.wing, &z(Zone::Cloth), Transform::from_xyz(s * 0.008, 0.585, -0.145).with_rotation(Quat::from_rotation_z(-s * FRAC_PI_2)));
            }
            put(c, &g.knot, &z(Zone::Cloth), Transform::from_xyz(0.0, 0.585, -0.15));
            None
        }
    }
}

/// Moves the accessory's and the outfit's moving parts.
pub(super) fn wiggle(parts: &mut Query<&mut Transform, Without<super::Rig>>, wiggle: [Option<Entity>; 2], look: Look, anim: &Anim, pose: &Pose, t: f32) {
    let stride = anim.phase.sin() * (anim.speed / 3.0).min(1.0);
    if let Some(Ok(mut tr)) = wiggle[0].map(|e| parts.get_mut(e)) {
        match look.accessory {
            Accessory::Antenna => {
                let sway = 0.12 * stride + 0.04 * (t * 1.7).sin();
                tr.rotation = on_head(ANTENNA.0, ANTENNA.1, 0.0).rotation * Quat::from_rotation_z(sway) * Quat::from_rotation_x(0.6 * pose.lean + 0.05);
            }
            Accessory::Halo => {
                tr.rotation = Quat::from_rotation_y(t * 0.8);
                tr.translation.y = HALO + 0.012 * (t * 2.0).sin();
            }
            Accessory::Sprout => {
                tr.rotation = Quat::from_rotation_z(0.14 * stride + 0.05 * (t * 1.3).sin()) * Quat::from_rotation_x(0.5 * pose.lean);
            }
            _ => {}
        }
    }
    if let (Outfit::Scarf, Some(Ok(mut tr))) = (look.outfit, wiggle[1].map(|e| parts.get_mut(e))) {
        // the tail lifts off the chest as it walks and swings with the steps
        tr.rotation = Quat::from_rotation_x(0.3 * (anim.speed / 4.5).min(1.0) - 0.1 * pose.lean) * Quat::from_rotation_z(0.12 * stride);
    }
}
