//! A disk cache of loaded rooms: the decoded meshes, baked textures, lights, colliders and sounds of a `RoomData`,
//! under `%LOCALAPPDATA%\Metallic-RC\cache\rooms` (never in the repo). A room is keyed by its world name, the
//! install's `catalog.bin` (size and date: a game update changes it) and this program's own file (size and date:
//! a rebuild may read rooms differently), so a stale entry is never read. Textures are stored once each,
//! LZ4-compressed. The folder is kept under `LIMIT` bytes by dropping the least recently written rooms.
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::math::{Mat4, Vec3};
use unity_assets::lz4_flex;

use crate::{
    audio::Clip,
    collide::{Collider, MeshShape, Obb},
    room::{Blend, ParticleEmitter, Part, RoomData, RoomLight, RoomMaterial, RoomSound},
    shapes::Pixels,
};

const MAGIC: &[u8; 8] = b"MRCROOM4";
/// The cache folder's size cap.
const LIMIT: u64 = 1 << 30;

/// `%LOCALAPPDATA%\Metallic-RC\cache\rooms`
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Metallic-RC").join("cache").join("rooms"))
}

fn stamp(p: &Path, h: &mut impl Hasher) {
    if let Ok(m) = std::fs::metadata(p) {
        m.len().hash(h);
        m.modified().ok().hash(h);
    }
    p.hash(h);
}

/// The cache file for `world` loaded from `bundle_dir` (`StreamingAssets/aa/StandaloneWindows64`).
pub fn file_for(bundle_dir: &Path, world: &str, extra: &str) -> Option<PathBuf> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    world.hash(&mut h);
    extra.hash(&mut h);
    stamp(&bundle_dir.parent()?.join("catalog.bin"), &mut h);
    stamp(&std::env::current_exe().ok()?, &mut h);
    let safe: String = world.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(40).collect();
    Some(dir()?.join(format!("{safe}-{:016x}.room", h.finish())))
}

/// The cached room, if there is a readable one.
pub fn read(path: &Path) -> Option<RoomData> {
    let bytes = std::fs::read(path).ok()?;
    let mut r = R { b: &bytes, at: 0 };
    if r.take(8)? != MAGIC {
        return None;
    }
    decode(&mut r)
}

/// Stores a room; then trims the folder to its cap. Failures only cost the next load its speed-up.
pub fn write(path: &Path, room: &RoomData) {
    let mut w = W::default();
    w.0.extend_from_slice(MAGIC);
    encode(&mut w, room);
    let Some(folder) = path.parent() else { return };
    if std::fs::create_dir_all(folder).is_err() {
        return;
    }
    let tmp = path.with_extension("part");
    if std::fs::write(&tmp, &w.0).is_err() || std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    trim(folder, path);
}

/// Drops the oldest room files (never `keep`) until the folder is under `LIMIT`.
fn trim(folder: &Path, keep: &Path) {
    let Ok(rd) = std::fs::read_dir(folder) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "room"))
        .filter_map(|e| { let m = e.metadata().ok()?; Some((m.modified().ok()?, m.len(), e.path())) })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, p) in files {
        if total <= LIMIT {
            break;
        }
        if p != keep && std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

// ---- encoding: little-endian, lengths as u32 ----

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u32(&mut self, v: u32) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn i64(&mut self, v: i64) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn f32(&mut self, v: f32) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn bool(&mut self, v: bool) { self.0.push(v as u8); }
    fn usize(&mut self, v: usize) { self.u32(v as u32); }
    fn bytes(&mut self, b: &[u8]) { self.usize(b.len()); self.0.extend_from_slice(b); }
    fn str(&mut self, s: &str) { self.bytes(s.as_bytes()); }
    fn floats(&mut self, v: &[f32]) { self.usize(v.len()); for &x in v { self.f32(x); } }
    fn mat(&mut self, m: &Mat4) { for x in m.to_cols_array() { self.f32(x); } }
    fn vec3(&mut self, v: Vec3) { self.f32(v.x); self.f32(v.y); self.f32(v.z); }
    fn flat<const N: usize>(&mut self, v: &[[f32; N]]) { self.floats(v.as_flattened()); }
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> { Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?)) }
    fn i64(&mut self) -> Option<i64> { Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?)) }
    fn f32(&mut self) -> Option<f32> { Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?)) }
    fn bool(&mut self) -> Option<bool> { Some(self.take(1)?[0] != 0) }
    fn usize(&mut self) -> Option<usize> { self.u32().map(|v| v as usize) }
    fn bytes(&mut self) -> Option<Vec<u8>> { let n = self.usize()?; self.take(n).map(<[u8]>::to_vec) }
    fn str(&mut self) -> Option<String> { String::from_utf8(self.bytes()?).ok() }
    fn floats(&mut self) -> Option<Vec<f32>> {
        let n = self.usize()?;
        Some(self.take(n.checked_mul(4)?)?.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }
    fn arr<const N: usize>(&mut self) -> Option<[f32; N]> {
        let mut a = [0.0; N];
        for x in &mut a {
            *x = self.f32()?;
        }
        Some(a)
    }
    fn mat(&mut self) -> Option<Mat4> { Some(Mat4::from_cols_array(&self.arr::<16>()?)) }
    fn vec3(&mut self) -> Option<Vec3> { Some(Vec3::from(self.arr::<3>()?)) }
    fn flat<const N: usize>(&mut self) -> Option<Vec<[f32; N]>> {
        let v = self.floats()?;
        (v.len() % N == 0).then(|| v.chunks_exact(N).map(|c| c.try_into().unwrap()).collect())
    }
    fn list<T>(&mut self, mut f: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let n = self.usize()?;
        (0..n).map(|_| f(self)).collect()
    }
}

const NONE: u32 = u32::MAX;

fn encode(w: &mut W, r: &RoomData) {
    // textures once each (materials share them across colour variants), compressed
    let mut textures: Vec<&Pixels> = Vec::new();
    let mut index: HashMap<(u32, u32, u64), u32> = HashMap::new();
    let mut refs: Vec<[u32; 4]> = Vec::new();
    for m in &r.materials {
        let mut ids = [NONE; 4];
        for (k, p) in [&m.texture, &m.spec_map, &m.normal_map, &m.emissive_map].into_iter().enumerate() {
            let Some(p) = p else { continue };
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.2.hash(&mut h);
            let key = (p.0, p.1, h.finish());
            ids[k] = *index.entry(key).or_insert_with(|| {
                textures.push(p);
                textures.len() as u32 - 1
            });
        }
        refs.push(ids);
    }
    w.usize(textures.len());
    for p in &textures {
        w.u32(p.0);
        w.u32(p.1);
        w.bytes(&lz4_flex::compress_prepend_size(&p.2));
    }
    w.usize(r.materials.len());
    for (m, ids) in r.materials.iter().zip(&refs) {
        w.str(&m.name);
        w.str(&m.shader);
        w.floats(&m.color);
        w.bool(m.masked);
        w.floats(&m.spec_color);
        w.f32(m.smoothness);
        w.floats(&m.emissive);
        w.f32(m.emissive_power);
        match m.blend {
            Blend::Opaque => w.u32(0),
            Blend::Mask(c) => { w.u32(1); w.f32(c); }
            Blend::Blend => w.u32(2),
            Blend::Add => w.u32(3),
        }
        w.bool(m.unlit);
        w.floats(&m.uv_scale);
        w.floats(&m.uv_offset);
        for &i in ids {
            w.u32(i);
        }
    }
    w.usize(r.parts.len());
    for p in &r.parts {
        w.str(&p.name);
        w.usize(p.root);
        w.str(&p.mesh);
        w.mat(&p.world);
        w.flat(&p.positions);
        w.flat(&p.normals);
        w.flat(&p.uvs);
        w.flat(&p.colors);
        w.usize(p.indices.len());
        for &i in &p.indices {
            w.u32(i);
        }
        w.usize(p.material);
    }
    w.usize(r.roots.len());
    for (i, name) in r.roots.iter().enumerate() {
        w.str(name);
        w.mat(&r.places[i]);
        w.bool(r.props[i]);
    }
    w.usize(r.colliders.len());
    for (c, &root) in r.colliders.iter().zip(&r.collider_roots) {
        match c {
            Collider::Box(o) => {
                w.usize(0);
                w.vec3(o.center);
                for a in o.axes {
                    w.vec3(a);
                }
                w.vec3(o.half);
            }
            Collider::Sphere { center, radius } => {
                w.usize(1);
                w.vec3(*center);
                w.f32(*radius);
            }
            Collider::Capsule { a, b, r: rad } => {
                w.usize(2);
                w.vec3(*a);
                w.vec3(*b);
                w.f32(*rad);
            }
            Collider::Mesh(m) => {
                w.usize(3);
                w.usize(m.tris.len());
                for t in &m.tris {
                    for v in t {
                        w.vec3(*v);
                    }
                }
            }
        }
        w.usize(root);
    }
    w.usize(r.lights.len());
    for l in &r.lights {
        w.usize(l.root);
        w.mat(&l.world);
        w.i64(l.kind);
        w.floats(&l.color);
        for x in [l.intensity, l.range, l.spot_angle, l.inner_spot_angle] {
            w.f32(x);
        }
        w.bool(l.shadows);
        w.bool(l.hemisphere.is_some());
        let (a, b) = l.hemisphere.unwrap_or_default();
        w.f32(a);
        w.f32(b);
    }
    // clips once each
    let mut clips: Vec<&Arc<Clip>> = Vec::new();
    for s in &r.sounds {
        if !clips.iter().any(|c| Arc::ptr_eq(c, &s.clip)) {
            clips.push(&s.clip);
        }
    }
    w.usize(clips.len());
    for c in &clips {
        w.str(&c.name);
        w.u32(c.rate);
        w.u32(c.channels as u32);
        w.str(c.codec);
        let raw: Vec<u8> = c.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        w.bytes(&lz4_flex::compress_prepend_size(&raw));
    }
    w.usize(r.sounds.len());
    for s in &r.sounds {
        w.usize(s.root);
        w.mat(&s.world);
        w.usize(clips.iter().position(|c| Arc::ptr_eq(c, &s.clip)).unwrap_or(0));
        for x in [s.volume, s.pitch, s.spatial, s.min_distance, s.max_distance] {
            w.f32(x);
        }
        w.bool(s.looped);
        w.i64(s.rolloff);
        let flat: Vec<[f32; 2]> = s.rolloff_curve.iter().map(|&(t, v)| [t, v]).collect();
        w.flat(&flat);
    }
    w.usize(r.problems.len());
    for p in &r.problems {
        w.str(p);
    }
    w.usize(r.spawns.len());
    for s in &r.spawns {
        w.mat(s);
    }
    w.usize(r.particles.len());
    for p in &r.particles {
        w.mat(&p.world);
        w.u32(p.max_particles);
        w.bool(p.looping);
        for f in [p.rate, p.lifetime, p.speed, p.size, p.gravity, p.shape_angle, p.shape_radius] {
            w.f32(f);
        }
        w.i64(p.shape as i64);
        for x in p.start_color {
            w.f32(x);
        }
        w.usize(p.bursts.len());
        for &(t, c) in &p.bursts {
            w.f32(t);
            w.f32(c);
        }
        w.usize(p.color_over_life.len());
        for &(t, c) in &p.color_over_life {
            w.f32(t);
            for x in c {
                w.f32(x);
            }
        }
        w.usize(p.size_over_life.len());
        for &(t, s) in &p.size_over_life {
            w.f32(t);
            w.f32(s);
        }
    }
}

fn codec(name: &str) -> &'static str {
    ["pcm8", "pcm16", "pcm24", "pcm32", "pcmfloat", "gcadpcm", "imaadpcm", "mpeg", "vorbis", "fadpcm", "opus"]
        .into_iter().find(|c| *c == name).unwrap_or("other")
}

fn decode(r: &mut R) -> Option<RoomData> {
    let textures: Vec<Pixels> = r.list(|r| {
        let (w, h) = (r.u32()?, r.u32()?);
        Some((w, h, lz4_flex::decompress_size_prepended(&r.bytes()?).ok()?))
    })?;
    let tex = |i: u32| if i == NONE { Some(None) } else { textures.get(i as usize).map(|p| Some(p.clone())) };
    let mut out = RoomData::default();
    out.materials = r.list(|r| {
        let (name, shader, color) = (r.str()?, r.str()?, r.floats()?);
        let (masked, spec_color, smoothness, emissive, emissive_power) = (r.bool()?, r.floats()?, r.f32()?, r.floats()?, r.f32()?);
        let blend = match r.u32()? {
            0 => Blend::Opaque,
            1 => Blend::Mask(r.f32()?),
            2 => Blend::Blend,
            _ => Blend::Add,
        };
        let (unlit, uv_scale, uv_offset) = (r.bool()?, r.floats()?, r.floats()?);
        let (t, sm, nm, em) = (tex(r.u32()?)?, tex(r.u32()?)?, tex(r.u32()?)?, tex(r.u32()?)?);
        Some(RoomMaterial {
            name, shader, color: color.try_into().ok()?, masked, texture: t, spec_map: sm, spec_color: spec_color.try_into().ok()?,
            smoothness, normal_map: nm, emissive: emissive.try_into().ok()?, emissive_map: em, emissive_power, blend, unlit,
            uv_scale: uv_scale.try_into().ok()?, uv_offset: uv_offset.try_into().ok()?,
        })
    })?;
    out.parts = r.list(|r| {
        Some(Part {
            name: r.str()?, root: r.usize()?, mesh: r.str()?, world: r.mat()?,
            positions: r.flat()?, normals: r.flat()?, uvs: r.flat()?, colors: r.flat()?,
            indices: r.list(|r| r.u32())?, material: r.usize()?,
        })
    })?;
    for (name, place, prop) in r.list(|r| Some((r.str()?, r.mat()?, r.bool()?)))? {
        out.roots.push(name);
        out.places.push(place);
        out.props.push(prop);
    }
    for (c, root) in r.list(|r| {
        let col = match r.usize()? {
            0 => Collider::Box(Obb { center: r.vec3()?, axes: [r.vec3()?, r.vec3()?, r.vec3()?], half: r.vec3()? }),
            1 => Collider::Sphere { center: r.vec3()?, radius: r.f32()? },
            2 => Collider::Capsule { a: r.vec3()?, b: r.vec3()?, r: r.f32()? },
            _ => {
                let n = r.usize()?;
                let mut tris = Vec::with_capacity(n);
                for _ in 0..n {
                    tris.push([r.vec3()?, r.vec3()?, r.vec3()?]);
                }
                Collider::Mesh(MeshShape::new(tris))
            }
        };
        Some((col, r.usize()?))
    })? {
        out.colliders.push(c);
        out.collider_roots.push(root);
    }
    out.lights = r.list(|r| {
        let (root, world, kind, color) = (r.usize()?, r.mat()?, r.i64()?, r.floats()?);
        let [intensity, range, spot_angle, inner_spot_angle] = r.arr::<4>()?;
        let (shadows, hemi, a, b) = (r.bool()?, r.bool()?, r.f32()?, r.f32()?);
        Some(RoomLight { root, world, kind, color: color.try_into().ok()?, intensity, range, spot_angle, inner_spot_angle, shadows, hemisphere: hemi.then_some((a, b)) })
    })?;
    let clips: Vec<Arc<Clip>> = r.list(|r| {
        let (name, rate, channels, codec_name) = (r.str()?, r.u32()?, r.u32()? as u16, r.str()?);
        let raw = lz4_flex::decompress_size_prepended(&r.bytes()?).ok()?;
        let samples = raw.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        Some(Arc::new(Clip { name, rate, channels, samples, codec: codec(&codec_name) }))
    })?;
    out.sounds = r.list(|r| {
        let (root, world, clip) = (r.usize()?, r.mat()?, clips.get(r.usize()?)?.clone());
        let [volume, pitch, spatial, min_distance, max_distance] = r.arr::<5>()?;
        let (looped, rolloff, curve) = (r.bool()?, r.i64()?, r.flat::<2>()?);
        Some(RoomSound { root, world, clip, volume, pitch, looped, spatial, min_distance, max_distance, rolloff, rolloff_curve: curve.into_iter().map(|[t, v]| (t, v)).collect() })
    })?;
    out.problems = r.list(|r| r.str())?;
    out.spawns = r.list(|r| r.mat())?;
    out.particles = r.list(|r| {
        let world = r.mat()?;
        let max_particles = r.u32()?;
        let looping = r.bool()?;
        let (rate, lifetime, speed, size, gravity, shape_angle, shape_radius) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        let shape = r.i64()? as i32;
        let start_color = r.arr::<4>()?;
        let bursts = r.list(|r| Some((r.f32()?, r.f32()?)))?;
        let color_over_life = r.list(|r| Some((r.f32()?, r.arr::<4>()?)))?;
        let size_over_life = r.list(|r| Some((r.f32()?, r.f32()?)))?;
        Some(ParticleEmitter { world, max_particles, looping, rate, bursts, lifetime, speed, size, gravity, shape, shape_angle, shape_radius, start_color, color_over_life, size_over_life })
    })?;
    (r.at == r.b.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_round_trips() {
        let px: Pixels = (2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let clip = Arc::new(Clip { name: "c".into(), rate: 8000, channels: 1, samples: vec![1, -2, 3], codec: "pcm16" });
        let mut r = RoomData::default();
        r.materials.push(RoomMaterial {
            name: "m".into(), shader: "s".into(), color: [1.0, 0.5, 0.25, 1.0], masked: true, texture: Some(px.clone()),
            spec_map: None, spec_color: [0.2; 3], smoothness: 0.4, normal_map: Some(px.clone()), emissive: [0.0, 1.0, 0.0],
            emissive_map: None, emissive_power: 2.0, blend: Blend::Mask(0.3), unlit: false, uv_scale: [1.0, 2.0], uv_offset: [0.0, -1.0],
        });
        r.parts.push(Part {
            name: "p".into(), root: 0, mesh: "mesh".into(), world: Mat4::from_translation(Vec3::X), positions: vec![[1.0, 2.0, 3.0]],
            normals: vec![[0.0, 1.0, 0.0]], uvs: vec![[0.5, 0.5]], colors: vec![], indices: vec![0, 0, 0], material: 0,
        });
        r.roots.push("root".into());
        r.places.push(Mat4::IDENTITY);
        r.props.push(true);
        r.colliders.push(Collider::Box(Obb { center: Vec3::ONE, axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::splat(0.5) }));
        r.colliders.push(Collider::Sphere { center: Vec3::Y, radius: 0.7 });
        r.colliders.push(Collider::Capsule { a: Vec3::ZERO, b: Vec3::Y, r: 0.3 });
        r.colliders.push(Collider::Mesh(MeshShape::new(vec![[Vec3::ZERO, Vec3::X, Vec3::Y]])));
        for _ in 0..4 {
            r.collider_roots.push(0);
        }
        r.lights.push(RoomLight { root: 0, world: Mat4::IDENTITY, kind: 2, color: [1.0; 3], intensity: 2.0, range: 5.0, spot_angle: 30.0, inner_spot_angle: 20.0, shadows: true, hemisphere: Some((90.0, 180.0)) });
        for _ in 0..2 {
            r.sounds.push(RoomSound { root: 0, world: Mat4::IDENTITY, clip: clip.clone(), volume: 0.5, pitch: 1.0, looped: true, spatial: 1.0, min_distance: 1.0, max_distance: 9.0, rolloff: 2, rolloff_curve: vec![(0.0, 1.0), (1.0, 0.0)] });
        }
        r.problems.push("x".into());

        let mut w = W::default();
        encode(&mut w, &r);
        let back = decode(&mut R { b: &w.0, at: 0 }).expect("decodes");
        assert_eq!(back.materials[0].texture, Some(px.clone()));
        assert_eq!(back.materials[0].normal_map, Some(px));
        assert_eq!(back.materials[0].blend, Blend::Mask(0.3));
        assert_eq!((back.materials[0].masked, back.materials[0].emissive_power, back.materials[0].spec_color), (true, 2.0, [0.2; 3]));
        assert_eq!(back.parts[0].positions, r.parts[0].positions);
        assert_eq!(back.parts[0].world, r.parts[0].world);
        assert_eq!((back.roots.clone(), back.props.clone(), back.collider_roots.clone()), (r.roots.clone(), r.props.clone(), r.collider_roots.clone()));
        assert_eq!(back.colliders.len(), 4);
        assert!(matches!(back.colliders[0], Collider::Box(ref o) if o.half == Vec3::splat(0.5)));
        assert!(matches!(back.colliders[1], Collider::Sphere { radius, .. } if (radius - 0.7).abs() < 1e-6));
        assert!(matches!(back.colliders[2], Collider::Capsule { r, .. } if (r - 0.3).abs() < 1e-6));
        assert!(matches!(back.colliders[3], Collider::Mesh(ref m) if m.tris.len() == 1));
        assert_eq!(back.lights[0].hemisphere, Some((90.0, 180.0)));
        assert_eq!(back.sounds.len(), 2);
        assert!(Arc::ptr_eq(&back.sounds[0].clip, &back.sounds[1].clip));
        assert_eq!((back.sounds[0].clip.samples.clone(), back.sounds[0].clip.codec), (vec![1, -2, 3], "pcm16"));
        assert_eq!(back.problems, ["x"]);
        // a cut-off file is refused, not misread
        assert!(decode(&mut R { b: &w.0[..w.0.len() - 3], at: 0 }).is_none());
    }
}
