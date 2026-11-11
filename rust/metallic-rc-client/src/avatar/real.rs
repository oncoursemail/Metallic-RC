//! The REAL Rec Room avatar from the owned game bundle, replacing the procedural "Pip" parts when the game files are
//! present. The real body mesh (plus its nose) is split by region into skin / shirt / pants so each avatar wears its
//! look's colours, and the real Rec Room eye and mouth sprites are placed on the face as decals. Per-joint skinning and
//! real store outfits come next; until then the body is static in its bind pose but still moves with its root.
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    image::Image,
    mesh::{
        skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
        Indices, PrimitiveTopology,
    },
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use unity_assets::{classes, Bundle, Mesh as UMesh, SerializedFile, Texture2D};

use super::{anim, clips, skin, Drive, Rig};
use crate::room;

/// The avatar skin bundle inside the install, and the body + nose mesh objects in it.
const AVATAR_BUNDLE: &str = "edf7f2d64a5698a6cec7ec8a1f821ab8.bundle";
const BODY_MESH_PATHID: i64 = 8332635914843264857; // BodyMesh_LOD0
const NOSE_MESH_PATHID: i64 = 62350462907586393; // Nose_01Base_LOD0
/// The real Rec Room face sprite bundles. Each holds the whole expression set as separate 256²/512² textures, named
/// (Eye_Default, Eye_Blink, Eye_Happy, ...; Mouth_Blank, Mouth_Talking_Open/Tall/Wide, Mouth_Smile, ...); `load_face_set`
/// reads them all by name and `animate_face` swaps them to blink and run talking visemes.
const EYE_BUNDLE: &str = "84554899b3243253ba56b4d0262997b6.bundle";
const MOUTH_BUNDLE: &str = "8fcb685a7a49de356df5b83ada6b0c86.bundle";

/// Rec Room ships two avatar body types. The Bean is the classic, iconic Rec Room look: a rounded legless torso with a
/// head and floating hands. The Full body is the newer humanoid with arms and legs. Both share the same `Jnt.*` rig.
/// Chosen once for the client with `METALLIC_BODY` (default: the Bean).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BodyType {
    Bean,
    Full,
}

/// The Modern Bean body bundle and its skinned mesh (`MB_BodyMesh_LOD0`).
const BEAN_BUNDLE: &str = "38066d851f08e64e9ed64b9094f31883.bundle";
const BEAN_BODY_ID: i64 = 9072706044162942483;

impl BodyType {
    fn from_env() -> Self {
        match std::env::var("METALLIC_BODY").ok().as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("full") => BodyType::Full,
            _ => BodyType::Bean,
        }
    }
    fn bundle(self) -> &'static str {
        match self {
            BodyType::Bean => BEAN_BUNDLE,
            BodyType::Full => AVATAR_BUNDLE,
        }
    }
    fn body_id(self) -> i64 {
        match self {
            BodyType::Bean => BEAN_BODY_ID,
            BodyType::Full => BODY_MESH_PATHID,
        }
    }
}

/// The loaded real avatar meshes, shared by every avatar. Absent when the game files are not installed.
#[derive(Resource)]
pub struct Roomie {
    /// the body split by region, so each avatar colours them from its look
    pub skin: Handle<Mesh>,
    pub shirt: Handle<Mesh>,
    pub pants: Handle<Mesh>,
    /// the shared eye / mouth quads (square, the full sprite canvas), facing -z (the eye quad is mirrored for the right
    /// side), and every expression by name as a decal material for `animate_face` to swap (blink, talking visemes, ...)
    pub eye_quad: Handle<Mesh>,
    pub mouth_quad: Handle<Mesh>,
    pub eyes: HashMap<String, Handle<StandardMaterial>>,
    pub mouths: HashMap<String, Handle<StandardMaterial>>,
    /// the orange "smiley box" chest logo decal (generated) and the quad it rides on
    pub logo: Handle<StandardMaterial>,
    pub logo_quad: Handle<Mesh>,
    /// the real Rec Room hair mesh (from the exported wardrobe) and its material; absent if the wardrobe isn't present
    pub hair: Option<Handle<Mesh>>,
    pub hair_mat: Handle<StandardMaterial>,
    /// the real jersey (torso) and basic-pants (legs) clothing meshes; coloured per-look when attached
    pub jersey: Option<Handle<Mesh>>,
    pub legs: Option<Handle<Mesh>>,
    /// the real joint rig and its inverse bind poses, when the body is skinned (else the body is the static bind pose)
    pub rig: Option<Arc<skin::RigDef>>,
    pub inverse_bindposes: Option<Handle<SkinnedMeshInverseBindposes>>,
    /// which body type is loaded (drives how the rig is posed: the Bean floats, the Full body strides)
    pub body: BodyType,
}

/// The real avatar's spawned joint entities (rig order) and the standing skeleton node, kept on the avatar root so the
/// animation can pose the joints and bob/lower the whole body.
#[derive(Component)]
pub struct Skeleton {
    pub joints: Vec<Entity>,
    /// the node carrying the stand-up; its vertical offset bobs/lowers the body
    pub root_node: Entity,
    /// eased finger curls per hand (outer three fingers, index, thumb) so grips don't snap
    pub curls: [[f32; 3]; 2],
    /// the Bean body floats (no legs; hands drift at the sides) and is posed differently from the Full body
    pub bean: bool,
    /// the authored-clip state currently playing, the state being blended out of (with its frozen clip-time), and the
    /// crossfade progress 0..1 — so transitions between clips ease instead of snapping
    pub clip_state: &'static str,
    pub blend_from: Option<(&'static str, f32)>,
    pub blend: f32,
}

/// The face decals of one avatar plus a per-avatar phase seed, so `animate_face` can swap their expression materials to
/// blink and run talking visemes without each avatar blinking in lockstep.
#[derive(Component)]
pub struct Face {
    pub eyes: [Entity; 2],
    pub mouth: Entity,
    pub seed: u32,
}

/// Marks a `Rig` whose real body has been attached, so it is attached once.
#[derive(Component)]
pub struct RealBuilt;

/// The node that all of one avatar's real parts hang from, so a gentle idle (breathe + sway) can move them together
/// until real per-joint skinning lands. `phase` offsets each avatar so they don't breathe in lockstep.
#[derive(Component)]
pub struct Visual {
    phase: f32,
}

/// Where the face sits in the avatar's upright local space (feet at 0, facing -z): each eye, and the mouth.
/// The eyes are set ~0.124 m apart (±0.062): the Eye_Default sprite's ink (pupil + upper-lid swoosh) spans most of its
/// canvas, so at a smaller spacing the two mirrored quads overlapped into one bar (the "sunglasses" look). Wider and a
/// touch smaller makes two distinct Rec Room eyes with a clear gap.
const EYE_POS: [Vec3; 2] = [Vec3::new(-0.062, 1.521, -0.14), Vec3::new(0.062, 1.521, -0.14)];
const MOUTH_POS: Vec3 = Vec3::new(0.0, 1.470, -0.158);
// the quads are the sprite's full square canvas (256² eye, 512² mouth): sized so the inked feature reads at the right
// size without the two eyes overlapping (eye 0.145 → a ~0.11 inked eye; mouth 0.27 → a ~0.10 line).
const EYE_W: f32 = 0.145;
const MOUTH_W: f32 = 0.27;
/// The chest "smiley box" logo, in the Full body's standing frame (feet at 0, facing -z): where it sits and how big.
const LOGO_POS: Vec3 = Vec3::new(0.0, 1.05, -0.135);
const LOGO_W: f32 = 0.19;
/// Joint names to hang the chest logo from, in preference order (the first the rig has wins).
const CHEST_JOINTS: [&str; 6] = ["Jnt.Spine.Chest", "Jnt.Chest", "Jnt.Spine.Upper", "Jnt.Spine2", "Jnt.Spine.Mid", "Jnt.Spine.Root"];
/// The Full body's head joint height in the standing frame (feet at 0); EYE_POS/MOUTH_POS are placed relative to it, so
/// another body type's face shifts by its own head height minus this.
const FULL_HEAD_Y: f32 = 1.421;

/// A float from an env var, for dialling the face in without a rebuild (e.g. `METALLIC_EYE_X`); falls back to `d`.
fn envf(key: &str, d: f32) -> f32 {
    std::env::var(key).ok().and_then(|s| s.parse().ok()).unwrap_or(d)
}

/// The resting expression (when not talking or blinking). Rec Room avatars sit with a gentle smile, so that is the
/// default rather than the stern neutral line; override with METALLIC_REST_EYE / METALLIC_REST_MOUTH (e.g.
/// `METALLIC_REST_MOUTH=Mouth_Blank` for a neutral mouth).
fn rest_eye() -> String {
    std::env::var("METALLIC_REST_EYE").unwrap_or_else(|_| "Eye_Default".into())
}
fn rest_mouth() -> String {
    std::env::var("METALLIC_REST_MOUTH").unwrap_or_else(|_| "Mouth_Smile".into())
}

/// The face layout, overridable per-field by env while tuning (METALLIC_EYE_X/Y/Z/W, METALLIC_MOUTH_Y/Z/W).
struct FaceLayout {
    eye: [Vec3; 2],
    eye_w: f32,
    mouth: Vec3,
    mouth_w: f32,
}

fn face_layout() -> FaceLayout {
    let ex = envf("METALLIC_EYE_X", EYE_POS[1].x);
    let ey = envf("METALLIC_EYE_Y", EYE_POS[0].y);
    let ez = envf("METALLIC_EYE_Z", EYE_POS[0].z);
    FaceLayout {
        eye: [Vec3::new(-ex, ey, ez), Vec3::new(ex, ey, ez)],
        eye_w: envf("METALLIC_EYE_W", EYE_W),
        mouth: Vec3::new(MOUTH_POS.x, envf("METALLIC_MOUTH_Y", MOUTH_POS.y), envf("METALLIC_MOUTH_Z", MOUTH_POS.z)),
        mouth_w: envf("METALLIC_MOUTH_W", MOUTH_W),
    }
}

/// A mesh object read out of the bundle as Bevy-space geometry (Unity is left-handed: z negated, winding flipped, uv v
/// flipped — same convention as `room.rs`).
struct Raw {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

fn read_umesh(b: &Bundle, sf: &SerializedFile, pathid: i64) -> Option<Raw> {
    let o = sf.object(pathid)?;
    let v = sf.read_value(o).ok()?;
    let m = UMesh::from_value(&v, b).ok()?;
    let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
    let pos: Vec<[f32; 3]> = m.positions().into_iter().map(flip).collect();
    if pos.is_empty() {
        return None;
    }
    let mut nrm: Vec<[f32; 3]> = m.normals().into_iter().map(flip).collect();
    nrm.resize(pos.len(), [0.0, 1.0, 0.0]);
    let mut uv = m.uv(0);
    uv.resize(pos.len(), [0.0, 0.0]);
    let uv: Vec<[f32; 2]> = uv.into_iter().map(|[u, v]| [u, 1.0 - v]).collect();
    let mut idx = Vec::new();
    for (i, sub) in m.submeshes.iter().enumerate() {
        if sub.topology != 0 {
            continue;
        }
        if let Some(mut s) = m.submesh_indices(i) {
            for tri in s.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
            idx.extend_from_slice(&s);
        }
    }
    Some(Raw { pos, nrm, uv, idx })
}

/// Which clothing zone a body triangle belongs to, from its centre in the avatar's own space (height along -z after our
/// flip, so `h = -z`; `x` is the left-right span). Head and the forearms/hands stay skin; the legs are pants; the rest
/// (torso and upper arms) is the shirt.
fn zone(p: [f32; 3]) -> usize {
    let (x, h) = (p[0].abs(), -p[2]);
    if h > 1.40 || x > 0.33 {
        0 // skin: head, forearms, hands
    } else if h < 0.86 && x < 0.3 {
        2 // pants: legs and feet
    } else {
        1 // shirt: torso and upper arms
    }
}

/// Loads the real body (+ nose) and splits it into skin / shirt / pants meshes sharing one vertex buffer.
fn load_bodies(dir: &Path) -> Option<[Mesh; 3]> {
    let path = room::bundle_dir(dir).join(AVATAR_BUNDLE);
    let b = Bundle::open(&path).ok()?;
    let files = b.serialized_files().ok()?;
    let sf = files.iter().find_map(|(_, sf)| sf.object(BODY_MESH_PATHID).map(|_| sf))?;
    let mut raw = read_umesh(&b, sf, BODY_MESH_PATHID)?;
    // the nose is omitted by default (Rec Room's face is a flat screen; the low-poly nose reads as uncanny). Set
    // AVATAR_NOSE=1 to include it. (Kept in step with `skin::load`.)
    if std::env::var("AVATAR_NOSE").is_ok() {
        if let Some(nose) = read_umesh(&b, sf, NOSE_MESH_PATHID) {
            let base = raw.pos.len() as u32;
            raw.pos.extend_from_slice(&nose.pos);
            raw.nrm.extend_from_slice(&nose.nrm);
            raw.uv.extend_from_slice(&nose.uv);
            raw.idx.extend(nose.idx.iter().map(|i| i + base));
        }
    }
    // split the triangles into the three zones by their centre
    let mut zones: [Vec<u32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for tri in raw.idx.chunks_exact(3) {
        let c = [0, 1, 2].map(|k| raw.pos[tri[k] as usize]);
        let mid = [0, 1, 2].map(|a| (c[0][a] + c[1][a] + c[2][a]) / 3.0);
        zones[zone(mid)].extend_from_slice(tri);
    }
    let make = |idx: Vec<u32>| {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, raw.pos.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, raw.nrm.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, raw.uv.clone());
        mesh.insert_indices(Indices::U32(idx));
        mesh
    };
    Some([make(std::mem::take(&mut zones[0])), make(std::mem::take(&mut zones[1])), make(std::mem::take(&mut zones[2]))])
}

/// Turns a Rec Room face sprite into a black decal over its full canvas: the ink's darkness (times its own alpha) is the
/// decal's alpha, so the white page goes transparent and the drawn feature opaque. The canvas is kept uncropped — every
/// expression in a set is authored the same size with the feature in place, so one shared quad holds them all and each
/// keeps its true relative size and offset (a talking mouth opens downward, a blink is a low flat line) when swapped.
fn face_decal(t: &Texture2D) -> Option<Image> {
    let px = t.decode_rgba8().ok()?;
    let (w, h) = (t.width as usize, t.height as usize);
    if px.len() < w * h * 4 {
        return None;
    }
    let mut out = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let lum = (px[i * 4] as u32 + px[i * 4 + 1] as u32 + px[i * 4 + 2] as u32) / 3;
        out[i * 4 + 3] = ((255 - lum.min(255)) * px[i * 4 + 3] as u32 / 255) as u8;
    }
    Some(Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        out,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

/// The Rec Room wardrobe meshes exported from the game (AssetStudio `.obj`): real hair, hats, clothing. Override the
/// folder with METALLIC_WARDROBE_DIR.
fn wardrobe_dir() -> std::path::PathBuf {
    std::env::var("METALLIC_WARDROBE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(r"C:\Users\alexi\rr-capture\decomp-full\assets\AssetStudioExport"))
}

/// Loads a Wavefront `.obj` (positions / normals / uvs / triangles) into a Bevy mesh, converting from the game's
/// authored space to Bevy's the same way the body mesh is (negate z, flip the triangle winding, flip uv v). The wardrobe
/// meshes are authored in the body's own space, so the result lines up on the rig with no fitting.
fn load_obj(path: &Path) -> Option<Mesh> {
    let text = std::fs::read_to_string(path).ok()?;
    let (mut vs, mut ns, mut ts): (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>) = (Vec::new(), Vec::new(), Vec::new());
    let nums = |it: std::str::SplitWhitespace, n: usize| -> Vec<f32> { it.take(n).filter_map(|x| x.parse().ok()).collect() };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => { let a = nums(it, 3); if a.len() == 3 { vs.push([a[0], a[1], -a[2]]); } }
            Some("vn") => { let a = nums(it, 3); if a.len() == 3 { ns.push([a[0], a[1], -a[2]]); } }
            Some("vt") => { let a = nums(it, 2); if a.len() == 2 { ts.push([a[0], 1.0 - a[1]]); } }
            _ => {}
        }
    }
    let (mut pos, mut nrm, mut uv): (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>) = (Vec::new(), Vec::new(), Vec::new());
    let mut idx: Vec<u32> = Vec::new();
    let mut map: HashMap<String, u32> = HashMap::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() != Some("f") {
            continue;
        }
        let verts: Vec<&str> = it.collect();
        if verts.len() < 3 {
            continue;
        }
        let mut face = Vec::with_capacity(verts.len());
        for v in &verts {
            let i = *map.entry((*v).to_string()).or_insert_with(|| {
                let p: Vec<&str> = v.split('/').collect();
                let vi = p[0].parse::<i64>().unwrap_or(1).max(1) as usize - 1;
                let ti = p.get(1).filter(|s| !s.is_empty()).and_then(|s| s.parse::<i64>().ok()).map(|t| t as usize - 1);
                let ni = p.get(2).and_then(|s| s.parse::<i64>().ok()).map(|n| n as usize - 1);
                pos.push(vs.get(vi).copied().unwrap_or([0.0; 3]));
                nrm.push(ni.and_then(|n| ns.get(n).copied()).unwrap_or([0.0, 1.0, 0.0]));
                uv.push(ti.and_then(|t| ts.get(t).copied()).unwrap_or([0.0; 2]));
                (pos.len() - 1) as u32
            });
            face.push(i);
        }
        // fan-triangulate, reversing winding (the z negation flips it)
        for k in 1..face.len() - 1 {
            idx.extend_from_slice(&[face[0], face[k + 1], face[k]]);
        }
    }
    if pos.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

/// Loads a wardrobe mesh `stem` (e.g. "ShirtJersey", "BasicPants_Leg", "CurlyHelmetHair_Hair") for `body`: the Full
/// body's meshes are `FB_`-prefixed, the Bean's are bare. Tries the prefixed name, then the bare one.
fn load_wardrobe(body: BodyType, stem: &str) -> Option<Mesh> {
    let prefix = if body == BodyType::Full { "FB_" } else { "" };
    load_obj(&wardrobe_dir().join(format!("{prefix}{stem}_LOD0.obj")))
        .or_else(|| load_obj(&wardrobe_dir().join(format!("{stem}_LOD0.obj"))))
}

/// The hair mesh for `body`. Default style: a short curly cut; override with METALLIC_HAIR (`Afro`, `WavyShort`, ...).
fn load_hair(body: BodyType) -> Option<Mesh> {
    let style = std::env::var("METALLIC_HAIR").unwrap_or_else(|_| "CurlyHelmetHair".into());
    load_wardrobe(body, &format!("{style}_Hair"))
}

/// Loads every face sprite (`Texture2D`) in a bundle into a map `name -> decal material` (Eye_Default, Eye_Blink, ...;
/// Mouth_Blank, Mouth_Talking_Open, ...). All share one quad (`animate_face` swaps only the material). Skips unreadable.
fn load_face_set(dir: &Path, bundle: &str, images: &mut Assets<Image>, materials: &mut Assets<StandardMaterial>) -> HashMap<String, Handle<StandardMaterial>> {
    let mut set = HashMap::new();
    let path = room::bundle_dir(dir).join(bundle);
    let Ok(b) = Bundle::open(&path) else { return set };
    let Ok(files) = b.serialized_files() else { return set };
    for (_, sf) in &files {
        for o in sf.objects.iter().filter(|o| o.class_id == classes::TEXTURE_2D) {
            let Some(name) = sf.object_name(o) else { continue };
            if set.contains_key(&name) {
                continue;
            }
            let Ok(v) = sf.read_value(o) else { continue };
            let Ok(t) = Texture2D::from_value(&v, &b) else { continue };
            if let Some(img) = face_decal(&t) {
                set.insert(name, decal(materials, images.add(img)));
            }
        }
    }
    set
}

fn decal(materials: &mut Assets<StandardMaterial>, tex: Handle<Image>) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color_texture: Some(tex),
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        unlit: true,
        ..default()
    })
}

/// Draws the Rec Room "smiley box" chest logo: an orange rounded-square outline with two eyes and a smile, on
/// transparent, so it sits as a decal on the jersey. The ink colour is `rgb`.
fn logo_texture(rgb: [u8; 3]) -> Image {
    const N: usize = 256;
    let nf = N as f32;
    let mut out = vec![0u8; N * N * 4];
    // signed distance to a rounded rectangle (half-extents `h`, corner radius `r`) centred at `c`
    let rrect = |p: Vec2, c: Vec2, h: Vec2, r: f32| {
        let q = (p - c).abs() - h + Vec2::splat(r);
        q.max(Vec2::ZERO).length() + q.max_element().min(0.0) - r
    };
    for y in 0..N {
        for x in 0..N {
            let p = Vec2::new((x as f32 + 0.5) / nf, (y as f32 + 0.5) / nf); // 0..1, y downward
            let stroke = 0.05;
            // the box outline
            let box_sd = rrect(p, Vec2::splat(0.5), Vec2::splat(0.33), 0.14).abs() - stroke * 0.5;
            // two eyes
            let eye = 0.052;
            let le = (p - Vec2::new(0.37, 0.41)).length() - eye;
            let re = (p - Vec2::new(0.63, 0.41)).length() - eye;
            // a smile: the lower arc of a circle
            let sc = Vec2::new(0.5, 0.45);
            let sr = 0.165;
            let smile = if p.y > sc.y + 0.015 { ((p - sc).length() - sr).abs() - stroke * 0.5 } else { 1.0 };
            let d = box_sd.min(le).min(re).min(smile);
            // anti-alias the edge over ~1.5 px
            let a = (0.5 - d * nf / 1.5).clamp(0.0, 1.0);
            if a > 0.0 {
                let i = (y * N + x) * 4;
                out[i] = rgb[0];
                out[i + 1] = rgb[1];
                out[i + 2] = rgb[2];
                out[i + 3] = (a * 255.0) as u8;
            }
        }
    }
    Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        out,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Startup: find the install and load the real body, nose and face sprites; absent if the game files are not there.
pub fn load_roomie(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut binds: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let Some(dir) = room::GAME_DIRS.iter().map(Path::new).find(|p| p.exists()) else {
        return;
    };
    let body = BodyType::from_env();
    // the skinned body (per-joint rig) is preferred; if its skin data can't be read, fall back to the static bind pose
    let (bodies, rig, inverse_bindposes) = match skin::load(dir, body.bundle(), body.body_id(), body == BodyType::Bean) {
        Some(l) => {
            info!("avatar: real Rec Room {body:?} body loaded skinned ({} joints)", l.rig.joints.len());
            (l.meshes, Some(Arc::new(l.rig)), Some(binds.add(SkinnedMeshInverseBindposes::from(l.inverse_bindposes))))
        }
        None => match load_bodies(dir) {
            Some(m) => {
                info!("avatar: real Rec Room body loaded static (no skin data)");
                (m, None, None)
            }
            None => {
                info!("avatar: no real body in the game files, using the procedural avatar");
                return;
            }
        },
    };
    let [skin, shirt, pants] = bodies;
    // the whole expression set for each (default/blink/happy/... eyes; blank/talking-visemes/... mouths)
    let eyes = load_face_set(dir, EYE_BUNDLE, &mut images, &mut materials);
    let mouths = load_face_set(dir, MOUTH_BUNDLE, &mut images, &mut materials);
    if !eyes.contains_key("Eye_Default") || !mouths.contains_key("Mouth_Blank") {
        info!("avatar: real body loaded, but the face sprites could not be read (eyes: {}, mouths: {})", eyes.len(), mouths.len());
        return;
    }
    info!("avatar: face sprites loaded ({} eye, {} mouth expressions)", eyes.len(), mouths.len());
    // the real authored locomotion clips, when a (decrypted) corpus is reachable; else the procedural poser drives the rig
    if let Some(rigdef) = &rig {
        if let Some(loaded) = clips::load(dir, rigdef) {
            commands.insert_resource(loaded);
        }
    }
    let logo = decal(&mut materials, images.add(logo_texture([0xf0, 0x81, 0x3c])));
    let hair = load_hair(body).map(|m| meshes.add(m));
    match &hair {
        Some(_) => info!("avatar: real Rec Room hair loaded"),
        None => info!("avatar: no wardrobe hair found (set METALLIC_WARDROBE_DIR)"),
    }
    let hair_mat = materials.add(StandardMaterial { base_color: Color::srgb_u8(0x8f, 0x4a, 0x27), perceptual_roughness: 0.78, ..default() });
    let jersey = load_wardrobe(body, "ShirtJersey").map(|m| meshes.add(m));
    let legs = load_wardrobe(body, "BasicPants_Leg").map(|m| meshes.add(m));
    info!("avatar: wardrobe — hair {}, jersey {}, legs {}", hair.is_some(), jersey.is_some(), legs.is_some());
    commands.insert_resource(Roomie {
        skin: meshes.add(skin),
        shirt: meshes.add(shirt),
        pants: meshes.add(pants),
        eye_quad: meshes.add(Rectangle::new(face_layout().eye_w, face_layout().eye_w)),
        mouth_quad: meshes.add(Rectangle::new(face_layout().mouth_w, face_layout().mouth_w)),
        eyes,
        mouths,
        logo,
        logo_quad: meshes.add(Rectangle::new(LOGO_W, LOGO_W)),
        hair,
        hair_mat,
        jersey,
        legs,
        rig,
        inverse_bindposes,
        body,
    });
}

/// The feet-at-origin, facing -z stand-up for the body mesh (authored lying along -z after our z-flip).
fn stand() -> Quat {
    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
}

fn solid(materials: &mut Assets<StandardMaterial>, c: [u8; 3], roughness: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: Color::srgb_u8(c[0], c[1], c[2]), perceptual_roughness: roughness, ..default() })
}

/// Attaches the real body (skin/shirt/pants) and face under every `Rig` once. First-person rigs get no body yet.
/// With a skinned rig the body deforms per joint (the joint hierarchy is spawned under the avatar and the meshes get a
/// `SkinnedMesh`); otherwise it is the static bind pose. Either way the face decals ride the head.
pub fn attach_real(
    mut commands: Commands,
    roomie: Option<Res<Roomie>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    rigs: Query<(Entity, &Rig), Without<RealBuilt>>,
) {
    let Some(roomie) = roomie else { return };
    for (root, rig) in &rigs {
        commands.entity(root).insert(RealBuilt);
        if rig.first_person {
            continue;
        }
        // everything hangs from one visual node so the idle can move the whole avatar together
        let visual = commands
            .spawn((Transform::default(), Visibility::default(), Visual { phase: (rig.seed as f32) * 1.3 }, ChildOf(root)))
            .id();

        let look = &rig.look;
        // when real clothing meshes are worn, the base body is all skin (the jersey + pants provide shirt/legs);
        // otherwise the body's own regions carry the shirt/pants colours
        let clothed = roomie.jersey.is_some() || roomie.legs.is_some();
        let mats = if clothed {
            let skin = solid(&mut materials, look.skin, 0.55);
            [skin.clone(), skin.clone(), skin]
        } else {
            [solid(&mut materials, look.skin, 0.55), solid(&mut materials, look.shirt, 0.8), solid(&mut materials, look.pants, 0.75)]
        };
        let regions = [roomie.skin.clone(), roomie.shirt.clone(), roomie.pants.clone()];

        // where the face decals hang: under the head joint when skinned (so they follow the head), else the visual node
        let face = Quat::from_rotation_y(std::f32::consts::PI);
        let decal_parent = match (&roomie.rig, &roomie.inverse_bindposes) {
            (Some(rigdef), Some(binds)) => {
                // the standing skeleton: a node with the stand-up, the joints under it, the meshes skinned to them
                let skeleton_node = commands.spawn((Transform::from_rotation(stand()), Visibility::default(), ChildOf(visual))).id();
                let joints = skin::spawn_skeleton(&mut commands, rigdef, skeleton_node);
                for (mesh, mat) in regions.into_iter().zip(mats) {
                    // skinned geometry is placed by the joints, not by this entity's transform, so its default AABB is
                    // in the wrong place and frustum culling would wrongly hide it up close; skip culling for it.
                    commands.spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(mat),
                        Transform::default(),
                        SkinnedMesh { inverse_bindposes: binds.clone(), joints: joints.clone() },
                        NoFrustumCulling,
                        ChildOf(visual),
                    ));
                }
                // the real hair mesh rides the standing skeleton node (same flip + stand-up as the body), so it sits on
                // the head exactly as authored
                if let Some(hair) = &roomie.hair {
                    commands.spawn((Mesh3d(hair.clone()), MeshMaterial3d(roomie.hair_mat.clone()), Transform::default(), NoFrustumCulling, ChildOf(skeleton_node)));
                }
                // the real jersey (torso) and pants (legs) clothing meshes, in the look's shirt/pants colours
                if let Some(j) = &roomie.jersey {
                    let m = solid(&mut materials, look.shirt, 0.8);
                    commands.spawn((Mesh3d(j.clone()), MeshMaterial3d(m), Transform::default(), NoFrustumCulling, ChildOf(skeleton_node)));
                }
                if let Some(l) = &roomie.legs {
                    let m = solid(&mut materials, look.pants, 0.75);
                    commands.spawn((Mesh3d(l.clone()), MeshMaterial3d(m), Transform::default(), NoFrustumCulling, ChildOf(skeleton_node)));
                }
                // a group under the head joint whose transform cancels the head's bind pose, so decals placed in the
                // standing avatar frame (EYE_POS / MOUTH_POS) land on the face and then follow the head joint.
                let head = rigdef.head;
                let cancel = (Mat4::from_quat(stand()) * rigdef.bind_world[head]).inverse();
                let group = commands.spawn((Transform::from_matrix(cancel), Visibility::default(), ChildOf(joints[head]))).id();
                // an optional drawn chest logo (METALLIC_LOGO=1), hung under a chest joint so it rides the torso. Off by
                // default: the real jersey carries its own print.
                if std::env::var("METALLIC_LOGO").is_ok() {
                    let chest = CHEST_JOINTS.iter().find_map(|n| rigdef.index(n)).unwrap_or(head);
                    let chest_cancel = (Mat4::from_quat(stand()) * rigdef.bind_world[chest]).inverse();
                    let chest_group = commands.spawn((Transform::from_matrix(chest_cancel), Visibility::default(), ChildOf(joints[chest]))).id();
                    let logo_pos = Vec3::new(0.0, envf("METALLIC_LOGO_Y", LOGO_POS.y), envf("METALLIC_LOGO_Z", LOGO_POS.z));
                    commands.spawn((Mesh3d(roomie.logo_quad.clone()), MeshMaterial3d(roomie.logo.clone()),
                        Transform { translation: logo_pos, rotation: face, ..default() }, ChildOf(chest_group)));
                }
                commands.entity(root).insert(Skeleton { joints, root_node: skeleton_node, curls: [anim::Grip::Open.curls(); 2], bean: roomie.body == BodyType::Bean, clip_state: "idle", blend_from: None, blend: 1.0 });
                group
            }
            _ => {
                let stand = Transform::from_rotation(stand());
                for (mesh, mat) in regions.into_iter().zip(mats) {
                    commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), stand, ChildOf(visual)));
                }
                visual
            }
        };

        // the real face sprites as decals, facing -z; the right eye is the same sprite mirrored in x. EYE_POS/MOUTH_POS
        // are in the Full body's standing frame; the Bean head sits at a different height, so shift the face to the
        // loaded body's head joint (keeps the Full body unchanged, places the Bean's face correctly).
        let head_dy = Vec3::Y
            * roomie.rig.as_ref().map_or(0.0, |r| (Mat4::from_quat(stand()) * r.bind_world[r.head]).to_scale_rotation_translation().2.y - FULL_HEAD_Y);
        // start at the resting expression; `animate_face` swaps these materials to blink and talk.
        let eye_mat = roomie.eyes.get(&rest_eye()).cloned().unwrap_or_default();
        let mouth_mat = roomie.mouths.get(&rest_mouth()).cloned().unwrap_or_default();
        let layout = face_layout();
        let mut eyes = [Entity::PLACEHOLDER; 2];
        for (i, p) in layout.eye.into_iter().enumerate() {
            let scale = Vec3::new(if i == 0 { 1.0 } else { -1.0 }, 1.0, 1.0);
            eyes[i] = commands.spawn((Mesh3d(roomie.eye_quad.clone()), MeshMaterial3d(eye_mat.clone()),
                Transform { translation: p + head_dy, rotation: face, scale }, ChildOf(decal_parent))).id();
        }
        let mouth = commands.spawn((Mesh3d(roomie.mouth_quad.clone()), MeshMaterial3d(mouth_mat),
            Transform { translation: layout.mouth + head_dy, rotation: face, ..default() }, ChildOf(decal_parent))).id();
        commands.entity(root).insert(Face { eyes, mouth, seed: rig.seed });
    }
}

/// A gentle idle for the *static* (un-skinned) body: it breathes and sways so it is never dead-still. Skinned avatars
/// are posed per joint (`animate_skeleton`) and skip this.
pub fn idle(time: Res<Time>, roomie: Option<Res<Roomie>>, mut visuals: Query<(&Visual, &mut Transform)>) {
    if roomie.as_ref().and_then(|r| r.rig.as_ref()).is_some() {
        return; // the real rig is posed per joint; don't also bob the whole visual
    }
    let t = time.elapsed_secs();
    for (v, mut tr) in &mut visuals {
        let b = t * 1.6 + v.phase;
        tr.translation.y = (b.sin() * 0.006) - 0.006;
        let s = 1.0 + b.sin() * 0.004;
        tr.scale = Vec3::new(1.0, s, 1.0);
        tr.rotation = Quat::from_rotation_z((t * 0.8 + v.phase).sin() * 0.012);
    }
}

// Pose angles (radians), in the rig's own skeleton space: the body-up axis is −z (feet→head), down is +z, forward
// (the way it faces) is −y, right is +x. Swings are expressed about those axes and conjugated into each joint's local
// frame by `skel_swing`, so no per-joint axis-guessing is needed. Tuned against `avatar_preview`.
const ARM_DOWN: f32 = 0.62; // swing the A-pose arms down to the sides, about the body-up axis
const ELBOW_REST: f32 = 0.18; // a slight resting elbow bend so the arms aren't ramrod straight
const ARM_SWING: f32 = 0.5; // fore/aft arm swing while walking (opposite the legs)
const KNEE_BEND: f32 = 1.15; // knee flex on the recovery (back) part of the stride
const SPINE_LEAN: f32 = 0.45; // share of the torso lean the spine takes (the rest leans the hips)
const CROUCH_KNEE: f32 = 0.9; // extra knee/hip flex when crouched
const BEAN_ARM_DOWN: f32 = 0.05; // the Bean's floating hands rest at the sides of the lower torso (near the bind height)
const BEAN_ARM_FWD: f32 = 0.18; // a slight fore tuck, so the hands float just ahead of the belly
const BEAN_ARM_IN: f32 = 0.28; // pulled in toward the body's midline
const BEAN_HAND_SWING: f32 = 0.22; // gentle fore/aft hand drift while the Bean moves
const CLIP_BLEND: f32 = 0.15; // seconds to crossfade between authored clip states (the game blends ~0.1-0.2 s)

/// The skeleton-space swing `r` (about the axes above), conjugated into joint `i`'s local frame and composed onto its
/// bind local rotation: `local' = (Wₚ⁻¹·r·Wₚ)·L`. `Wₚ` is the parent's bind-pose world rotation, so the bone swings by
/// `r` in skeleton space about its own pivot (exact when the parent is at bind, a good approximation when it isn't).
fn skel_swing(rig: &skin::RigDef, i: usize, r: Quat) -> Quat {
    let wp = match rig.joints[i].parent {
        Some(p) => rig.bind_world[p].to_scale_rotation_translation().1,
        None => Quat::IDENTITY,
    };
    (wp.inverse() * r * wp) * rig.joints[i].local.rotation
}

/// Drive the real 111-joint rig from the locomotion model: a natural rest (arms down, slight bend), a walk/run stride on
/// the hips/knees with the arms swinging against the legs, a forward lean, crouch, and a head that turns to the look.
/// Resets every joint to its bind pose first, then applies the swings; the body bobs/lowers via the skeleton node.
pub fn animate_skeleton(
    time: Res<Time>,
    roomie: Option<Res<Roomie>>,
    anim_clips: Option<Res<clips::Clips>>,
    mut rigs: Query<(&mut Rig, &Drive, &mut Skeleton, &Transform)>,
    mut xf: Query<&mut Transform, Without<Rig>>,
) {
    const STAND_EYE: f32 = 1.55;
    const CROUCH_EYE: f32 = 1.55 * 0.725;
    let Some(roomie) = roomie else { return };
    let Some(rig) = &roomie.rig else { return };
    let dt = time.delta_secs().min(0.1);
    let (x_axis, y_axis, z_axis) = (Vec3::X, Vec3::Y, Vec3::Z);
    let rx = |a: f32| Quat::from_axis_angle(x_axis, a);
    let ry = |a: f32| Quat::from_axis_angle(y_axis, a);
    let rz = |a: f32| Quat::from_axis_angle(z_axis, a);

    for (mut r, drive, mut skel, root) in &mut rigs {
        let crouch = drive.head_height.map_or(0.0, |h| ((STAND_EYE - h) / (STAND_EYE - CROUCH_EYE)).clamp(0.0, 1.0));
        r.anim.step(dt, anim::Input { velocity: drive.velocity, grounded: drive.grounded, flying: drive.flying, talking: drive.talking, voice: 0.0, crouch });
        let pose = r.anim.pose();
        let phase = r.anim.phase;
        let swing = phase.sin() * (r.anim.speed / 3.0).min(1.0); // stride amplitude, 0 standing still

        // reset all joints to their bind pose; posed joints are overwritten below
        for (i, j) in rig.joints.iter().enumerate() {
            if let Ok(mut t) = xf.get_mut(skel.joints[i]) {
                *t = j.local;
            }
        }

        // if a real authored clip covers this state, play it on the joints in place (the head-look and grips below are
        // layered on top); otherwise the hand-tuned procedural pose drives the body.
        let state = if let Some(emote) = drive.emote {
            emote // an emote overrides locomotion while held; the crossfade eases in and back out
        } else if drive.flying {
            "fly"
        } else {
            match r.anim.state {
                anim::Move::Idle => if crouch > 0.5 { "crouch_idle" } else { "idle" },
                anim::Move::Walk => if crouch > 0.5 { "crouch_walk" } else { "walk" },
                anim::Move::Run => "run",
                anim::Move::Jump => "jump",
                anim::Move::Fall => "fall",
                _ => "idle",
            }
        };
        // Authored clips are full-body locomotion (legs + arm swing). The Bean has no legs/arms — only floating hands —
        // so a walk clip would drop its hands like invisible arm-ends; keep the Bean on its procedural float pose.
        let mut root_y = pose.bob - pose.drop; // the body's vertical offset; an emote pose grounds it from its clip
        let clip_driven = !skel.bean && anim_clips.as_deref().map_or(false, |clips| {
            // the clip-time for a state (walk/run follow the stride phase so cadence tracks speed; the rest free-run)
            let clip_time = |st: &str, c: &clips::Clip| match st {
                "walk" | "run" | "crouch_walk" => (phase / std::f32::consts::TAU).rem_euclid(1.0) * c.stop_time,
                _ => time.elapsed_secs().rem_euclid(c.stop_time),
            };
            let Some(cur) = clips.get(state) else { return false };
            // ground an emote pose on the floor (sit/prone) from the clip's hip height; leave locomotion on its bob
            if drive.emote.is_some() {
                if let Some(y) = cur.root_y(clip_time(state, cur), rig, stand()) {
                    root_y = y;
                }
            }

            // start a crossfade when the state changes: blend out of the old state, frozen at its current clip-time
            if state != skel.clip_state {
                let from_t = clips.get(skel.clip_state).map(|c| clip_time(skel.clip_state, c)).unwrap_or(0.0);
                skel.blend_from = Some((skel.clip_state, from_t));
                skel.clip_state = state;
                skel.blend = 0.0;
            }
            skel.blend = (skel.blend + dt / CLIP_BLEND).min(1.0);
            if skel.blend >= 1.0 {
                skel.blend_from = None;
            }

            // the new state's authored local rotations by joint, then (while blending) ease from the old state's
            // rotations into them — a joint a clip doesn't touch rests at its bind rotation.
            let to: HashMap<usize, Quat> = cur.pose(clip_time(state, cur), rig).into_iter().collect();
            let from: HashMap<usize, Quat> = skel.blend_from
                .and_then(|(st, t)| clips.get(st).map(|c| c.pose(t, rig).into_iter().collect()))
                .unwrap_or_default();
            for &j in to.keys().chain(from.keys()) {
                let bind = rig.joints[j].local.rotation;
                let a = from.get(&j).copied().unwrap_or(bind);
                let b = to.get(&j).copied().unwrap_or(bind);
                if let Ok(mut t) = xf.get_mut(skel.joints[j]) {
                    t.rotation = if skel.blend_from.is_some() { a.slerp(b, skel.blend) } else { b };
                }
            }
            true
        });

        let mut set = |name: &str, rot: Quat| {
            if let Some(i) = rig.index(name) {
                if let Ok(mut t) = xf.get_mut(skel.joints[i]) {
                    t.rotation = skel_swing(rig, i, rot);
                }
            }
        };

        // --- torso lean (forward when moving/crouched): tilt the spine toward −y --- (procedural fallback only)
        let lean = pose.lean;
        if !clip_driven {
            set("Jnt.Spine.Mid", rx(-lean * SPINE_LEAN));
            set("Jnt.Spine.Chest", rx(-lean * SPINE_LEAN * 0.6));

            if skel.bean {
                // The Bean floats: its hands rest at the sides of the lower torso (a slight abduction, since there is no
                // arm mesh — only the hands float), tucked a little forward and inward, and drift a little while moving.
                let drift = swing * BEAN_HAND_SWING;
                // down = abduction (about body-up), fwd = fore (about left-right), in = pull toward the midline (about up)
                set("Jnt.UpperArm.L", rz(BEAN_ARM_IN) * rx(BEAN_ARM_FWD + drift) * ry(BEAN_ARM_DOWN));
                set("Jnt.UpperArm.R", rz(-BEAN_ARM_IN) * rx(BEAN_ARM_FWD - drift) * ry(-BEAN_ARM_DOWN));
            } else {
                // --- arms: down to the sides (about body-up), swinging fore/aft opposite the legs ---
                let arm_l = swing * ARM_SWING; // left arm forward as the left leg goes back
                let arm_r = -swing * ARM_SWING;
                set("Jnt.UpperArm.L", rx(arm_l) * ry(ARM_DOWN));
                set("Jnt.UpperArm.R", rx(arm_r) * ry(-ARM_DOWN));
                set("Jnt.LowerArm.L", rx(ELBOW_REST + 0.25 * crouch));
                set("Jnt.LowerArm.R", rx(ELBOW_REST + 0.25 * crouch));

                // --- legs: swing the thighs fore/aft; bend the knee on the recovery; add a crouch squat ---
                let leg = [pose.legs[0], pose.legs[1]];
                for (k, side) in ["L", "R"].iter().enumerate() {
                    let thigh = leg[k] - 0.5 * crouch * CROUCH_KNEE; // crouch flexes the hips forward
                    let knee = KNEE_BEND * (-leg[k]).max(0.0) + CROUCH_KNEE * crouch;
                    set(&format!("Jnt.UpperLeg.{side}"), rx(thigh));
                    set(&format!("Jnt.LowerLeg.{side}"), rx(knee));
                    set(&format!("Jnt.Foot.{side}"), rx(-0.4 * knee + 0.5 * leg[k].max(0.0)));
                }
            }
        }

        // --- head: turn to the synced look (relative to the avatar's facing); counter the spine lean a little ---
        let (yaw, pitch) = drive.head.map_or((0.0, 0.0), |h| {
            let (y, x, _) = (root.rotation.inverse() * h).to_euler(EulerRot::YXZ);
            (y.clamp(-1.1, 1.1), x.clamp(-0.7, 0.7))
        });
        // override the head to the synced look; but when a clip is driving and there's no look target, let its authored
        // head motion play instead of flattening it.
        if drive.head.is_some() || !clip_driven {
            set("Jnt.Neck", rz(yaw * 0.4) * rx(pitch * 0.4 + lean * SPINE_LEAN));
            set("Jnt.Head", rz(yaw * 0.6) * rx(pitch * 0.6));
        }

        // finger grips (both body types have hands): ease the curls so grips don't snap, then flex each finger's three
        // bones about their local X into the palm — Open rests, Point extends the index over a curled fist, Hold fists.
        for i in 0..2 {
            let s = if i == 0 { "L" } else { "R" };
            let want = drive.grip[i].curls();
            for (k, c) in skel.curls[i].iter_mut().enumerate() {
                *c += (want[k] - *c) * (1.0 - (-18.0 * dt).exp());
            }
            let [outer, index, thumb] = skel.curls[i];
            let mut flex = |name: &str, a: f32| {
                if let Some(j) = rig.index(name) {
                    if let Ok(mut t) = xf.get_mut(skel.joints[j]) {
                        t.rotation = rig.joints[j].local.rotation * rx(a);
                    }
                }
            };
            for finger in ["Middle", "Ring", "Pinky"] {
                for seg in 1..=3 {
                    flex(&format!("Jnt.Hand.{finger}{seg}.{s}"), outer);
                }
            }
            for seg in 1..=3 {
                flex(&format!("Jnt.Hand.Index{seg}.{s}"), index);
            }
            for seg in 1..=3 {
                flex(&format!("Jnt.Hand.Thumb{seg}.{s}"), thumb);
            }
        }

        // the whole body bobs with the stride, lowers when crouching, or grounds an emote pose, via the standing node
        if let Ok(mut t) = xf.get_mut(skel.root_node) {
            let mut tr = Transform::from_rotation(stand());
            tr.translation.y = root_y;
            *t = tr;
        }
    }
}

/// Brings the real face to life: the eyes blink every few seconds (each avatar on its own clock) and the mouth runs the
/// talking visemes while the avatar talks, resting neutral otherwise — swapping the decal materials from the loaded
/// expression sets. Only the symmetric expressions are used, so the mirrored right eye stays correct.
pub fn animate_face(
    time: Res<Time>,
    roomie: Option<Res<Roomie>>,
    faces: Query<(&Face, &Drive)>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let Some(roomie) = roomie else { return };
    let t = time.elapsed_secs();
    let (rest_eye, rest_mouth) = (rest_eye(), rest_mouth());
    const VISEMES: [&str; 3] = ["Mouth_Talking_Open", "Mouth_Talking_Tall", "Mouth_Talking_Wide"];
    // swap a decal entity to the expression material `m`, only when it actually changes
    let mut put = |ent: Entity, m: Option<&Handle<StandardMaterial>>| {
        if let (Some(m), Ok(mut mat)) = (m, mats.get_mut(ent)) {
            if mat.0.id() != m.id() {
                mat.0 = m.clone();
            }
        }
    };
    for (face, drive) in &faces {
        // blink: a quick close for ~0.12 s every few seconds, offset per avatar so they don't blink in lockstep
        let period = 3.2 + (face.seed % 7) as f32 * 0.45;
        let blinking = (t + (face.seed as f32 * 0.613).rem_euclid(period)).rem_euclid(period) < 0.12;
        let eye = roomie.eyes.get(if blinking { "Eye_Blink" } else { rest_eye.as_str() });
        for &e in &face.eyes {
            put(e, eye);
        }
        // mouth: cycle the talking shapes while talking (a speech-like flutter), else the neutral mouth
        let mouth = if drive.talking {
            VISEMES[((t * 10.0 + face.seed as f32 * 1.7) as i64).rem_euclid(VISEMES.len() as i64) as usize]
        } else {
            rest_mouth.as_str()
        };
        put(face.mouth, roomie.mouths.get(mouth));
    }
}
