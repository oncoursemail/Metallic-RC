//! Builds a room from the game's own prefabs in the owner's install (the Addressables bundles under
//! `RecRoom_Data/StreamingAssets/aa/StandaloneWindows64`), as plain mesh data the game turns into entities.
//!
//! Unity is left-handed (z forward), Bevy right-handed: positions, normals and transforms get z negated, triangles
//! get their winding flipped, and UV v is flipped because textures decode top row first.
use std::{collections::HashMap, path::{Path, PathBuf}, rc::Rc, sync::Arc};

use bevy::math::{Mat4, Quat, Vec3};
use unity_assets::{classes, Bundle, GameObject, Material, Mesh, MeshFilter, MeshRenderer, PPtr, SerializedFile, Texture2D, Transform};

use crate::{audio::{self, Clip}, collide::Obb, shapes::{self, Pixels, ShapeKit}};

/// The default place of the game files: the Steam copy kept for this work, then the installed game.
pub const GAME_DIRS: [&str; 2] = [
    r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711",
    r"C:\Program Files (x86)\Steam\steamapps\common\RecRoom",
];

pub fn bundle_dir(game_dir: &Path) -> PathBuf {
    game_dir.join(r"RecRoom_Data\StreamingAssets\aa\StandaloneWindows64")
}

/// One drawable piece: a submesh with its material, placed in the room.
pub struct Part {
    pub name: String,
    /// index into `RoomData::roots`: the prefab this part belongs to
    pub root: usize,
    /// the Mesh object's own name (scenes refer to meshes by it)
    pub mesh: String,
    pub world: Mat4,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// vertex colours, for the shaders that use them unbaked; usually empty
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: usize,
}

/// How a material's pixels mix: the game's blend modes as Bevy can draw them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blend {
    Opaque,
    /// alpha test at the cutoff (`*-AlphaClip` shaders, `_Cutoff`)
    Mask(f32),
    /// alpha blend (`*-Transparent*`)
    Blend,
    /// additive (`*-Additive*`)
    Add,
}

pub struct RoomMaterial {
    pub name: String,
    /// the shader's name, as the game files give it
    pub shader: String,
    /// the family's tint (sRGB; alpha = opacity of transparent materials): `_InstanceColor`, `_BaseColor` or the
    /// Maker Pen palette colour; the shader multiplies it with the vertex colour
    pub color: [f32; 4],
    /// AG lit shaders: the base map's alpha masks the tint (`rgb * lerp(1, tint, a)`) and lights the emission;
    /// otherwise (URP Lit, unlit) the tint multiplies
    pub masked: bool,
    /// `_BaseMap` as stored: width, height, RGBA8 rows top-down (sRGB)
    pub texture: Option<Pixels>,
    /// `_SpecGlossMap` as stored (linear): rgb = specular colour (F0), a = gloss
    pub spec_map: Option<Pixels>,
    /// linear `_SpecColor`, times the spec map
    pub spec_color: [f32; 3],
    /// `_Smoothness`, times the spec map's alpha (the gloss itself for URP Lit without a map)
    pub smoothness: f32,
    /// tangent-space normals, RGB (linear), from `_BumpMap`
    pub normal_map: Option<Pixels>,
    /// linear `_EmissionColor` (when the shader emits), times `emissive_map` when there is one
    pub emissive: [f32; 3],
    /// `_EmissionMap` (sRGB)
    pub emissive_map: Option<Pixels>,
    /// `_EmissivePower`: the masked albedo glows by this much
    pub emissive_power: f32,
    pub blend: Blend,
    /// drawn without lighting (`AG/Unlit-*`)
    pub unlit: bool,
    /// `_BaseMap_ST`: uv * scale + offset, already turned for the flipped v
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
}

/// A light the prefab carries (Unity `Light`, with Rec Room's `URPLight` beside it).
#[derive(Clone, Debug)]
pub struct RoomLight {
    pub root: usize,
    /// in the client's space; the light shines along -Z of this matrix (Unity's +Z, flipped)
    pub world: Mat4,
    /// 0 spot, 1 directional, 2 point (Unity `LightType`)
    pub kind: i64,
    /// linear colour
    pub color: [f32; 3],
    /// Unity intensity (URP: radiance = colour * intensity / d^2)
    pub intensity: f32,
    pub range: f32,
    /// full cone angles, degrees
    pub spot_angle: f32,
    pub inner_spot_angle: f32,
    pub shadows: bool,
    /// `URPLight.isHemisphereLight`: inner and outer full angles of a point light that only lights what is in front
    pub hemisphere: Option<(f32, f32)>,
}

/// An AudioSource that plays on its own (`m_PlayOnAwake`, enabled, with a clip); the ones scripts start are left out.
#[derive(Clone, Debug)]
pub struct RoomSound {
    pub root: usize,
    pub world: Mat4,
    pub clip: Arc<Clip>,
    pub volume: f32,
    pub pitch: f32,
    pub looped: bool,
    /// Unity spatialBlend (the first key of `panLevelCustomCurve`): 0 = 2D, 1 = 3D
    pub spatial: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// 0 logarithmic, 1 linear, 2 custom (`rolloff_curve`, over distance / max distance)
    pub rolloff: i64,
    pub rolloff_curve: Vec<(f32, f32)>,
}

impl RoomSound {
    /// Unity's distance attenuation at `d` metres, blended by `spatial` (a 2D sound does not fade).
    pub fn gain_at(&self, d: f32) -> f32 {
        let (min, max) = (self.min_distance.max(0.01), self.max_distance.max(self.min_distance.max(0.01) + 0.01));
        let g = match self.rolloff {
            // INFERRED: Unity's logarithmic curve is min / d, held at its value past max distance
            0 => min / d.clamp(min, max),
            1 => 1.0 - ((d - min) / (max - min)).clamp(0.0, 1.0),
            _ => curve_at(&self.rolloff_curve, d / max),
        };
        1.0 + (g.clamp(0.0, 1.0) - 1.0) * self.spatial.clamp(0.0, 1.0)
    }
}

/// An AnimationCurve's keys, linearly between them (INFERRED: tangents ignored), held flat past the ends.
pub fn curve_at(keys: &[(f32, f32)], t: f32) -> f32 {
    let Some(first) = keys.first() else { return 1.0 };
    if t <= first.0 {
        return first.1;
    }
    for w in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if t <= t1 {
            return if t1 > t0 { v0 + (v1 - v0) * (t - t0) / (t1 - t0) } else { v1 };
        }
    }
    keys.last().map_or(1.0, |k| k.1)
}

fn curve_keys(v: Option<&unity_assets::Value>) -> Vec<(f32, f32)> {
    let Some(keys) = v.and_then(|c| c.get("m_Curve")).and_then(|c| c.as_array()) else { return vec![] };
    keys.iter().filter_map(|k| Some((k.get("time")?.as_f32()?, k.get("value")?.as_f32()?))).collect()
}

/// How a game shader colours its surface (docs/agents/rc-spec/rendering.md in the private notes).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Family {
    /// URP Lit and anything unknown: `_BaseColor * _BaseMap`
    Lit,
    /// `AG/Standard-Opaque-Instanced*`: `rgb * lerp(1, vertex colour * _InstanceColor, a)`, emission `rgb*a*power*tint`
    Instanced,
    /// `AG/Standard-*`: the same with `_BaseColor` in place of `_InstanceColor`
    Standard,
    /// `AG/MakerPenShape`: the prop's SandboxColorable picks a palette colour and a Maker Pen material
    MakerPen,
    /// `AG/Unlit-*`, `AG/Mirror`: `_BaseMap * vertex colour * _InstanceColor`, no lighting
    Unlit,
}

fn family(shader: &str) -> Family {
    if shader.starts_with("AG/MakerPenShape") {
        Family::MakerPen
    } else if shader.starts_with("AG/Standard-Opaque-Instanced") {
        Family::Instanced
    } else if shader.starts_with("AG/Standard") {
        Family::Standard
    } else if shader.starts_with("AG/Unlit") || shader.starts_with("AG/Mirror") {
        Family::Unlit
    } else {
        Family::Lit
    }
}

fn blend_of(shader: &str, cutoff: Option<f32>) -> Blend {
    if shader.contains("Additive") {
        Blend::Add
    } else if shader.contains("Transparent") {
        Blend::Blend
    } else if shader.contains("AlphaClip") || shader.contains("Cutout") {
        Blend::Mask(cutoff.unwrap_or(0.5))
    } else {
        Blend::Opaque
    }
}

/// A game material as read, before a prop's colours are applied.
struct Source {
    name: String,
    shader: String,
    family: Family,
    /// `_BaseColor` (sRGB, with alpha)
    color: [f32; 4],
    /// the family's tint before vertex colours, linear: `_InstanceColor` or `_BaseColor`
    tint: [f32; 3],
    /// tint alpha (`_InstanceColor.a` / `_BaseColor.a`), the opacity of transparent materials
    alpha: f32,
    texture: Option<Pixels>,
    st: ([f32; 2], [f32; 2]),
    spec_map: Option<Pixels>,
    smoothness: f32,
    /// linear `_SpecColor`
    spec_color: [f32; 3],
    normal_map: Option<Pixels>,
    emissive_power: f32,
    /// linear `_EmissionColor`
    emission: [f32; 3],
    emission_map: Option<Pixels>,
    blend: Blend,
    /// interaction volumes and UI masks: present in the prefab, never seen in the game
    hidden: bool,
}

/// What a prop's SandboxColorable sets on its Maker Pen pieces: `ShapeColor` and `ShapeMaterial` ids.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
struct Colorable {
    color: Option<i32>,
    material: Option<i32>,
}

/// One drawn look of a source material.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Variant {
    Plain,
    Shape(Colorable),
}

#[derive(Default)]
pub struct RoomData {
    pub parts: Vec<Part>,
    pub materials: Vec<RoomMaterial>,
    pub roots: Vec<String>,
    /// where each root prefab was placed (its own root transform times the layout's placement)
    pub places: Vec<Mat4>,
    /// roots that are props: grabbable or movable objects (`RigidbodyPickup` / `MoveableProp` on the root)
    pub props: Vec<bool>,
    /// the solid BoxColliders (triggers left out), for walking
    pub colliders: Vec<Obb>,
    /// the root each collider belongs to
    pub collider_roots: Vec<usize>,
    pub lights: Vec<RoomLight>,
    pub sounds: Vec<RoomSound>,
    pub problems: Vec<String>,
}

pub(crate) struct Open {
    pub(crate) bundle: Bundle,
    pub(crate) files: Vec<(String, SerializedFile)>,
}

/// A serialized file inside an open bundle.
#[derive(Clone)]
pub(crate) struct FileRef {
    pub(crate) open: Rc<Open>,
    pub(crate) index: usize,
}

impl FileRef {
    pub(crate) fn sf(&self) -> &SerializedFile {
        &self.open.files[self.index].1
    }
    pub(crate) fn cab(&self) -> &str {
        &self.open.files[self.index].0
    }
}

/// The bundles of an install, indexed by the serialized files (CAB-...) they hold, opened as they are needed.
pub struct Library {
    cabs: HashMap<String, PathBuf>,
    open: HashMap<PathBuf, Rc<Open>>,
}

impl Library {
    pub fn new(dir: &Path) -> std::io::Result<Self> {
        let mut cabs = HashMap::new();
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            if p.extension().is_some_and(|x| x == "bundle") {
                if let Ok(b) = Bundle::open_head(&p) {
                    for n in b.nodes.iter().filter(|n| n.is_serialized_file()) {
                        cabs.insert(n.path.clone(), p.clone());
                    }
                }
            }
        }
        Ok(Library { cabs, open: HashMap::new() })
    }

    pub fn bundle_count(&self) -> usize {
        self.cabs.len()
    }

    /// Closes every open bundle (they are read whole into memory).
    pub fn forget(&mut self) {
        self.open.clear();
    }

    pub(crate) fn open_bundle(&mut self, path: &Path) -> Result<Rc<Open>, String> {
        if let Some(o) = self.open.get(path) {
            return Ok(o.clone());
        }
        let bundle = Bundle::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let files = bundle.serialized_files().map_err(|e| format!("{}: {e}", path.display()))?;
        let o = Rc::new(Open { bundle, files });
        self.open.insert(path.to_path_buf(), o.clone());
        Ok(o)
    }

    pub(crate) fn file(&mut self, cab: &str) -> Result<FileRef, String> {
        let path = self.cabs.get(cab).cloned().ok_or_else(|| format!("{cab} is in no bundle"))?;
        let open = self.open_bundle(&path)?;
        let index = open.files.iter().position(|(n, _)| n == cab).ok_or_else(|| format!("{cab} missing from its bundle"))?;
        Ok(FileRef { open, index })
    }

    /// The file and path id a PPtr seen in `from` points at.
    pub(crate) fn resolve(&mut self, from: &FileRef, p: PPtr) -> Result<(FileRef, i64), String> {
        if p.path_id == 0 {
            return Err("null reference".into());
        }
        if p.file_id == 0 {
            return Ok((from.clone(), p.path_id));
        }
        let ext = from.sf().externals.get(p.file_id as usize - 1).ok_or_else(|| format!("bad file id {}", p.file_id))?;
        let cab = ext.path.rsplit('/').next().unwrap_or(&ext.path).to_string();
        Ok((self.file(&cab)?, p.path_id))
    }
}

/// Unity's BoxCollider, Light and AudioSource classes.
const BOX_COLLIDER: i32 = 65;
const LIGHT: i32 = 108;
const AUDIO_SOURCE: i32 = 82;

fn class_of(f: &FileRef, path_id: i64) -> Option<i32> {
    f.sf().object(path_id).map(|o| o.class_id)
}

pub(crate) fn value(f: &FileRef, path_id: i64) -> Result<unity_assets::Value, String> {
    let o = f.sf().object(path_id).ok_or_else(|| format!("object {path_id} not in {}", f.cab()))?;
    f.sf().read_value(o).map_err(|e| e.to_string())
}

/// Linear colour to sRGB, opaque.
fn srgb(c: [f32; 3]) -> [f32; 4] {
    let s = |x: f32| if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    [s(c[0]), s(c[1]), s(c[2]), 1.0]
}

pub(crate) fn unity_to_bevy(t: &Transform) -> Mat4 {
    let [px, py, pz] = t.local_position;
    let [qx, qy, qz, qw] = t.local_rotation;
    Mat4::from_scale_rotation_translation(Vec3::from(t.local_scale), Quat::from_xyzw(-qx, -qy, qz, qw), Vec3::new(px, py, -pz))
}

struct Builder<'a> {
    lib: &'a mut Library,
    kit: &'a mut ShapeKit,
    out: RoomData,
    sources: Vec<Source>,
    source_of: HashMap<(String, i64), usize>,
    shaders: HashMap<(String, i64), String>,
    looks: HashMap<(usize, Variant), usize>,
    textures: HashMap<(String, i64), Option<Pixels>>,
    clips: HashMap<(String, i64), Option<Arc<Clip>>>,
}

impl Builder<'_> {
    fn texture(&mut self, from: &FileRef, p: PPtr) -> Option<Pixels> {
        let (tf, tid) = self.lib.resolve(from, p).ok()?;
        let key = (tf.cab().to_string(), tid);
        if let Some(t) = self.textures.get(&key) {
            return t.clone();
        }
        let t = value(&tf, tid).ok().and_then(|v| Texture2D::from_value(&v, &tf.open.bundle).ok())
            .and_then(|tex| Some((tex.width as u32, tex.height as u32, tex.decode_rgba8().ok()?)));
        self.textures.insert(key, t.clone());
        t
    }

    fn shader_name(&mut self, from: &FileRef, p: PPtr) -> String {
        let Ok((sf, id)) = self.lib.resolve(from, p) else { return String::new() };
        let key = (sf.cab().to_string(), id);
        if let Some(n) = self.shaders.get(&key) {
            return n.clone();
        }
        let name = value(&sf, id).ok()
            .and_then(|v| v.path("m_ParsedForm/m_Name").and_then(|n| n.as_str()).map(String::from))
            .unwrap_or_default();
        self.shaders.insert(key, name.clone());
        name
    }

    /// The game material a renderer slot points at, read once.
    fn source(&mut self, f: &FileRef, p: PPtr) -> usize {
        let Ok((mf, id)) = self.lib.resolve(f, p) else { return self.fallback_source() };
        let key = (mf.cab().to_string(), id);
        if let Some(&i) = self.source_of.get(&key) {
            return i;
        }
        let m = match value(&mf, id).and_then(|v| Material::from_value(&v).map_err(|e| e.to_string())) {
            Ok(m) => m,
            Err(e) => {
                self.out.problems.push(format!("material {id}: {e}"));
                return self.fallback_source();
            }
        };
        let shader = self.shader_name(&mf, m.shader);
        let fam = family(&shader);
        let color_of = |names: &[&str]| m.colors.iter().find(|(n, _)| names.contains(&n.as_str())).map(|c| c.1);
        let float = |name: &str| m.floats.iter().find(|(n, _)| n == name).map(|f| f.1);
        let env = |names: &[&str]| m.textures.iter().filter(|t| t.texture.path_id != 0).find(|t| names.contains(&t.name.as_str())).cloned();
        let color = color_of(&["_BaseColor", "_Color", "_MainColor"]).unwrap_or([1.0; 4]);
        // the AG shaders tint with _InstanceColor (instanced, unlit) or _BaseColor (standard), never _Color
        let tint4 = match fam {
            Family::Instanced | Family::Unlit => color_of(&["_InstanceColor"]).unwrap_or([1.0; 4]),
            _ => color_of(&["_BaseColor", "_Color"]).unwrap_or([1.0; 4]),
        };
        let tint = shapes::linear([tint4[0], tint4[1], tint4[2]]);
        // Rec Room's own shaders call their albedo _BaseMap; older and third-party ones use other names
        let base = env(&["_BaseMap", "_MainTex", "_Albedo", "_BaseColorMap", "_Diffuse"]).or_else(|| match fam {
            Family::Lit => m.textures.iter().filter(|t| t.texture.path_id != 0).find(|t| {
                let n = t.name.to_ascii_lowercase();
                !(n.contains("norm") || n.contains("bump") || n.contains("spec") || n.contains("mask") || n.contains("ao") || n.contains("light"))
            }).cloned(),
            _ => None,
        });
        let st = base.as_ref().map_or(([1.0; 2], [0.0; 2]), |t| (t.scale, t.offset));
        let texture = base.and_then(|t| self.texture(&mf, t.texture));
        let spec_map = env(&["_SpecGlossMap"]).and_then(|t| self.texture(&mf, t.texture));
        let normal_map = env(&["_BumpMap", "_NormalMap"]).and_then(|t| self.texture(&mf, t.texture)).map(|t| shapes::unpack_normals(&t));
        let smoothness = float("_Smoothness").or(float("_Glossiness")).unwrap_or(0.5);
        let spec_color = color_of(&["_SpecColor"]).map_or([1.0; 3], |c| shapes::linear([c[0], c[1], c[2]]));
        let emission_map = env(&["_EmissionMap"]).and_then(|t| self.texture(&mf, t.texture));
        let emission = color_of(&["_EmissionColor"]).filter(|_| m.keywords.iter().any(|k| k.contains("EMISSION")) || shader.contains("Emission"))
            .map_or([0.0; 3], |c| shapes::linear([c[0], c[1], c[2]]));
        let blend = blend_of(&shader, float("_Cutoff"));
        let hidden = ["InteractionVolume", "Mask_Mat", "LightShaft", "Light_Shaft"].iter().any(|k| m.name.contains(k));
        self.sources.push(Source {
            name: m.name, shader, family: fam, color, tint, alpha: tint4[3], texture, st, spec_map, smoothness, spec_color, normal_map,
            emissive_power: float("_EmissivePower").unwrap_or(0.0), emission, emission_map, blend, hidden,
        });
        let i = self.sources.len() - 1;
        self.source_of.insert(key, i);
        i
    }

    fn fallback_source(&mut self) -> usize {
        if let Some(&i) = self.source_of.get(&(String::new(), 0)) {
            return i;
        }
        self.sources.push(Source {
            name: "missing".into(), shader: String::new(), family: Family::Lit, color: [0.8, 0.8, 0.8, 1.0], tint: [1.0; 3], alpha: 1.0,
            texture: None, st: ([1.0; 2], [0.0; 2]), spec_map: None, smoothness: 0.2, spec_color: [1.0; 3], normal_map: None,
            emissive_power: 0.0, emission: [0.0; 3], emission_map: None, blend: Blend::Opaque, hidden: false,
        });
        let i = self.sources.len() - 1;
        self.source_of.insert((String::new(), 0), i);
        i
    }

    /// The material drawn for a source in one variant, made once.
    fn look(&mut self, src: usize, v: Variant) -> usize {
        if let Some(&i) = self.looks.get(&(src, v)) {
            return i;
        }
        let s = &self.sources[src];
        let mut name = s.name.clone();
        let (mut base, mut spec_map, mut smoothness, mut normal_map) = (s.texture.clone(), s.spec_map.clone(), s.smoothness, s.normal_map.clone());
        // the tint in linear light: the family colour, or the palette colour
        let mut tint = s.tint;
        if let Variant::Shape(c) = v {
            let own = [s.color[0], s.color[1], s.color[2]];
            tint = shapes::linear(c.color.and_then(|id| self.kit.color(id)).unwrap_or(own));
            normal_map = None;
            match c.material.and_then(|id| self.kit.material(id)).map(|m| (m.name.clone(), m.smoothness, m.base_map.clone(), m.spec_map.clone())) {
                Some((mname, sm, base_map, spec)) => {
                    name = format!("{name} ({mname})");
                    smoothness = sm;
                    base = self.kit.texture(&base_map).cloned();
                    spec_map = self.kit.texture(&spec).cloned();
                }
                // the material's own _BaseMap is empty (white) and its _SpecGlossMap black (no gloss)
                None => {
                    base = None;
                    spec_map = None;
                }
            }
        }
        let s = &self.sources[src];
        let masked = !matches!(s.family, Family::Lit | Family::Unlit);
        let rgb = srgb(tint);
        let (sc, of) = s.st;
        let mat = RoomMaterial {
            name, shader: s.shader.clone(), color: [rgb[0], rgb[1], rgb[2], s.alpha], masked, texture: base, spec_map,
            spec_color: s.spec_color, smoothness, normal_map, emissive: s.emission, emissive_map: s.emission_map.clone(),
            emissive_power: if masked { s.emissive_power } else { 0.0 }, blend: s.blend, unlit: s.family == Family::Unlit,
            uv_scale: sc, uv_offset: [of[0], 1.0 - sc[1] - of[1]],
        };
        self.out.materials.push(mat);
        let i = self.out.materials.len() - 1;
        self.looks.insert((src, v), i);
        i
    }

    /// Starts a root prefab placed at `place`: records it and whether it is a prop, then walks it.
    fn add_root(&mut self, name: String, tf: &FileRef, tid: i64, place: Mat4) {
        let prop = value(tf, tid).ok().and_then(|v| Transform::from_value(&v).ok())
            .and_then(|t| {
                let (gf, gid) = self.lib.resolve(tf, t.game_object).ok()?;
                let go = value(&gf, gid).ok().and_then(|v| GameObject::from_value(&v).ok())?;
                Some(self.is_prop(&gf, &go.components))
            })
            .unwrap_or(false);
        let own = value(tf, tid).ok().and_then(|v| Transform::from_value(&v).ok()).map_or(Mat4::IDENTITY, |t| unity_to_bevy(&t));
        self.out.roots.push(name);
        self.out.places.push(place * own);
        self.out.props.push(prop);
        self.walk(tf, tid, place, Colorable::default());
    }

    /// A root that players can pick up or move: its scripts include `RigidbodyPickup` or `MoveableProp`
    /// (`KinematicRigidbodyPickup` alone, on room pieces such as walls, does not count).
    fn is_prop(&mut self, gf: &FileRef, components: &[PPtr]) -> bool {
        components.iter().any(|&c| {
            let Ok((cf, cid)) = self.lib.resolve(gf, c) else { return false };
            if class_of(&cf, cid) != Some(classes::MONO_BEHAVIOUR) {
                return false;
            }
            let script = value(&cf, cid).ok().and_then(|v| v.get("m_Script")?.as_pptr())
                .and_then(|p| self.lib.resolve(&cf, p).ok())
                .and_then(|(sf, sid)| value(&sf, sid).ok()?.get("m_ClassName")?.as_str().map(String::from));
            matches!(script.as_deref(), Some("RigidbodyPickup" | "MoveableProp"))
        })
    }

    /// The SandboxColorable among a GameObject's components, if any.
    fn colorable(&mut self, gf: &FileRef, components: &[PPtr]) -> Option<Colorable> {
        for &c in components {
            let Ok((cf, cid)) = self.lib.resolve(gf, c) else { continue };
            if class_of(&cf, cid) != Some(classes::MONO_BEHAVIOUR) {
                continue;
            }
            let Ok(v) = value(&cf, cid) else { continue };
            if v.get("RecolorMode").is_none() || v.get("DefaultColor").is_none() {
                continue;
            }
            let on = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) != 0;
            let id = |k: &str| v.get(k).and_then(|x| x.as_i64()).map(|x| x as i32);
            return Some(Colorable {
                color: if on("UseDefaultColor") { id("DefaultColor") } else { None },
                material: if on("UseDefaultMaterial") { id("DefaultMaterial") } else { None },
            });
        }
        None
    }

    /// Adds a GameObject (given by its Transform) and its active children.
    fn walk(&mut self, f: &FileRef, transform_id: i64, parent: Mat4, colorable: Colorable) {
        let Ok(t) = value(f, transform_id).and_then(|v| Transform::from_value(&v).map_err(|e| e.to_string())) else { return };
        let Ok((gf, gid)) = self.lib.resolve(f, t.game_object) else { return };
        let Ok(go) = value(&gf, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string())) else { return };
        // lower levels of detail would draw on top of LOD0
        let lod = go.name.rfind("LOD").map(|i| &go.name[i + 3..]);
        if !go.active || lod.is_some_and(|n| !n.is_empty() && n != "0" && n.chars().all(|c| c.is_ascii_digit())) {
            return;
        }
        let world = parent * unity_to_bevy(&t);
        let colorable = self.colorable(&gf, &go.components).unwrap_or(colorable);

        let (mut filter, mut renderer) = (None, None);
        for &c in &go.components {
            let Ok((cf, cid)) = self.lib.resolve(&gf, c) else { continue };
            match class_of(&cf, cid) {
                Some(classes::MESH_FILTER) => filter = value(&cf, cid).ok().and_then(|v| MeshFilter::from_value(&v).ok()).map(|m| (cf, m)),
                Some(classes::MESH_RENDERER) => renderer = value(&cf, cid).ok().and_then(|v| MeshRenderer::from_value(&v).ok()).map(|r| (cf, r)),
                Some(BOX_COLLIDER) => {
                    let Ok(v) = value(&cf, cid) else { continue };
                    let on = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) != 0;
                    let vec3 = |k: &str| v.get(k).map(|x| x.floats()).filter(|f| f.len() == 3).map(|f| [f[0], f[1], f[2]]);
                    if let (true, false, Some(size), Some(center)) = (on("m_Enabled"), on("m_IsTrigger"), vec3("m_Size"), vec3("m_Center")) {
                        self.out.colliders.push(Obb::from_box_collider(world, center, size));
                        self.out.collider_roots.push(self.out.roots.len() - 1);
                    }
                }
                Some(LIGHT) => {
                    if let Ok(v) = value(&cf, cid) {
                        self.light(&gf, &go.components, &v, world);
                    }
                }
                Some(AUDIO_SOURCE) => {
                    if let Ok(v) = value(&cf, cid) {
                        self.sound(&cf, &v, world);
                    }
                }
                _ => {}
            }
        }
        if let (Some((ff, filter)), Some((rf, renderer))) = (filter, renderer) {
            if renderer.enabled {
                self.add_mesh(&go.name, world, &ff, filter.mesh, &rf, &renderer.materials, colorable);
            }
        }
        for &child in &t.children {
            if let Ok((cf, cid)) = self.lib.resolve(f, child) {
                self.walk(&cf, cid, world, colorable);
            }
        }
    }

    /// A realtime Light (baked-only ones light nothing at runtime: there are no lightmaps).
    fn light(&mut self, gf: &FileRef, components: &[PPtr], v: &unity_assets::Value, world: Mat4) {
        let int = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let float = |k: &str| v.get(k).and_then(|x| x.as_f32()).unwrap_or(0.0);
        // m_Lightmapping: 4 realtime, 1 mixed, 2 baked
        if int("m_Enabled") == 0 || int("m_Lightmapping") == 2 {
            return;
        }
        let c = v.get("m_Color").map(|c| c.floats()).filter(|c| c.len() >= 3).unwrap_or(vec![1.0; 3]);
        let mut hemisphere = None;
        for &comp in components {
            let Ok((cf, cid)) = self.lib.resolve(gf, comp) else { continue };
            if class_of(&cf, cid) != Some(classes::MONO_BEHAVIOUR) {
                continue;
            }
            let Ok(u) = value(&cf, cid) else { continue };
            if u.get("isHemisphereLight").and_then(|x| x.as_i64()).unwrap_or(0) != 0 {
                let a = |k: &str| u.get(k).and_then(|x| x.as_f32()).unwrap_or(180.0);
                hemisphere = Some((a("hemisphereLightInnerAngle"), a("hemisphereLightOuterAngle")));
            }
        }
        self.out.lights.push(RoomLight {
            root: self.out.roots.len() - 1,
            world,
            kind: int("m_Type"),
            color: shapes::linear([c[0], c[1], c[2]]),
            intensity: float("m_Intensity"),
            range: float("m_Range"),
            spot_angle: float("m_SpotAngle"),
            inner_spot_angle: float("m_InnerSpotAngle"),
            shadows: v.path("m_Shadows/m_Type").and_then(|x| x.as_i64()).unwrap_or(0) != 0,
            hemisphere,
        });
    }

    /// An AudioSource that plays by itself, with its clip decoded (once per clip).
    fn sound(&mut self, sf: &FileRef, v: &unity_assets::Value, world: Mat4) {
        let on = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) != 0;
        let float = |k: &str, d: f32| v.get(k).and_then(|x| x.as_f32()).unwrap_or(d);
        if !on("m_Enabled") || !on("m_PlayOnAwake") || on("Mute") {
            return;
        }
        let Some(p) = v.get("m_audioClip").and_then(|x| x.as_pptr()) else { return };
        let Ok((cf, cid)) = self.lib.resolve(sf, p) else { return };
        let key = (cf.cab().to_string(), cid);
        let clip = match self.clips.get(&key) {
            Some(c) => c.clone(),
            None => {
                let c = match value(&cf, cid).and_then(|cv| audio::clip_from_value(&cv, &cf.open.bundle)) {
                    Ok(c) => Some(Arc::new(c)),
                    Err(e) => {
                        self.out.problems.push(format!("audio clip {cid}: {e}"));
                        None
                    }
                };
                self.clips.insert(key, c.clone());
                c
            }
        };
        let Some(clip) = clip else { return };
        let pan = curve_keys(v.get("panLevelCustomCurve"));
        self.out.sounds.push(RoomSound {
            root: self.out.roots.len() - 1,
            world,
            clip,
            volume: float("m_Volume", 1.0),
            pitch: float("m_Pitch", 1.0),
            looped: on("Loop"),
            spatial: pan.first().map_or(0.0, |k| k.1),
            min_distance: float("MinDistance", 1.0),
            max_distance: float("MaxDistance", 500.0),
            rolloff: v.get("rolloffMode").and_then(|x| x.as_i64()).unwrap_or(0),
            rolloff_curve: curve_keys(v.get("rolloffCustomCurve")),
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn add_mesh(&mut self, name: &str, world: Mat4, ff: &FileRef, mesh: PPtr, rf: &FileRef, materials: &[PPtr], colorable: Colorable) {
        let m = match self.lib.resolve(ff, mesh).and_then(|(mf, id)| value(&mf, id).and_then(|v| Mesh::from_value(&v, &mf.open.bundle).map_err(|e| e.to_string()))) {
            Ok(m) => m,
            Err(e) => return self.out.problems.push(format!("{name}: mesh: {e}")),
        };
        let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
        let positions: Vec<_> = m.positions().into_iter().map(flip).collect();
        if positions.is_empty() {
            return;
        }
        let mut normals: Vec<_> = m.normals().into_iter().map(flip).collect();
        normals.resize(positions.len(), [0.0, 1.0, 0.0]);
        let mut raw_uvs = m.uv(0);
        raw_uvs.resize(positions.len(), [0.0, 0.0]);
        let mut colors = m.colors();
        if !colors.is_empty() {
            colors.resize(positions.len(), [1.0; 4]);
        }
        for (i, sub) in m.submeshes.iter().enumerate() {
            // topology 0 = triangles; lines and points are not drawn
            if sub.topology != 0 {
                continue;
            }
            let Some(mut idx) = m.submesh_indices(i) else { continue };
            for tri in idx.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
            let src = match materials.get(i).or(materials.last()) {
                Some(&p) => self.source(rf, p),
                None => self.fallback_source(),
            };
            if self.sources[src].hidden {
                continue;
            }
            // Maker Pen pieces repeat their material's texture every `size` metres of uv0; others use uv0 as it is.
            // The AG shaders other than Maker Pen multiply the vertex colour into the tint; URP Lit and Maker Pen
            // never read it (rendering.md).
            let (variant, part_colors, size) = match self.sources[src].family {
                Family::MakerPen => {
                    let size = colorable.material.and_then(|id| self.kit.material(id)).map_or(1.0, |m| m.size);
                    (Variant::Shape(colorable), vec![], size)
                }
                Family::Lit => (Variant::Plain, vec![], 1.0),
                _ => (Variant::Plain, colors.clone(), 1.0),
            };
            let material = self.look(src, variant);
            let uvs = raw_uvs.iter().map(|[u, v]| [u / size, 1.0 - v / size]).collect();
            self.out.parts.push(Part {
                name: name.to_string(),
                root: self.out.roots.len() - 1,
                mesh: m.name.clone(),
                world,
                positions: positions.clone(),
                normals: normals.clone(),
                uvs,
                colors: part_colors,
                indices: idx,
                material,
            });
        }
    }
}

/// Where to put prefabs that are not authored in place, read from a layout file: one prefab per line,
/// `name x y z [yaw [pitch [roll]]]` (metres, degrees) or `name x y z qx qy qz qw [sx sy sz]` (a rotation
/// quaternion and scale, as the generator from the original scene writes them), all in the client's (Bevy) space;
/// `#` starts a comment. A name may appear more than once to place several copies; `name hide` leaves it out.
pub fn parse_layout(text: &str) -> Result<Vec<(String, Option<Mat4>)>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let name = it.next().unwrap_or_default().to_string();
        if line.ends_with(" hide") {
            out.push((name, None));
            continue;
        }
        let v: Vec<f32> = it.map(|x| x.parse().map_err(|_| format!("layout line {}: bad number {x:?}", n + 1))).collect::<Result<_, _>>()?;
        let pos = Vec3::new(*v.first().unwrap_or(&0.0), *v.get(1).unwrap_or(&0.0), *v.get(2).unwrap_or(&0.0));
        if v.len() == 7 || v.len() == 10 {
            let rot = Quat::from_xyzw(v[3], v[4], v[5], v[6]).normalize();
            let scale = if v.len() == 10 { Vec3::new(v[7], v[8], v[9]) } else { Vec3::ONE };
            out.push((name, Some(Mat4::from_scale_rotation_translation(scale, rot, pos))));
            continue;
        }
        if v.len() < 3 || v.len() > 6 {
            return Err(format!("layout line {}: want name x y z [yaw [pitch [roll]]] or name x y z qx qy qz qw [sx sy sz]", n + 1));
        }
        let deg = |i: usize| v.get(i).copied().unwrap_or(0.0).to_radians();
        let rot = Quat::from_euler(bevy::math::EulerRot::YXZ, deg(3), deg(4), deg(5));
        out.push((name, Some(Mat4::from_rotation_translation(rot, Vec3::new(v[0], v[1], v[2])))));
    }
    Ok(out)
}

impl<'a> Builder<'a> {
    fn new(lib: &'a mut Library, kit: &'a mut ShapeKit) -> Self {
        Builder { lib, kit, out: RoomData::default(), sources: vec![], source_of: HashMap::new(), shaders: HashMap::new(), looks: HashMap::new(), textures: HashMap::new(), clips: HashMap::new() }
    }
}

/// Every root prefab in `bundle` whose name passes `want`. A prefab named in `layout` is placed there (once per
/// entry); the others stay where their own root transform puts them.
pub fn load_prefabs(lib: &mut Library, kit: &mut ShapeKit, dir: &Path, bundle: &str, want: impl Fn(&str) -> bool, layout: &[(String, Option<Mat4>)]) -> Result<RoomData, String> {
    let open = lib.open_bundle(&dir.join(bundle))?;
    let mut b = Builder::new(lib, kit);
    for index in 0..open.files.len() {
        let f = FileRef { open: open.clone(), index };
        let roots: Vec<(String, i64)> = f.sf().objects.iter()
            .filter(|o| o.class_id == classes::GAME_OBJECT)
            .filter_map(|o| Some((f.sf().object_name(o)?, o.path_id)))
            .filter(|(n, _)| want(n))
            .collect();
        for (name, gid) in roots {
            // the GameObject's Transform; only roots (no parent) are prefabs
            let Some((tf, tid)) = crate::world::transform_of(b.lib, &f, gid) else { continue };
            let is_root = value(&tf, tid).ok().and_then(|v| Transform::from_value(&v).ok()).is_some_and(|t| t.parent.path_id == 0);
            if !is_root {
                continue;
            }
            let entries: Vec<Option<Mat4>> = layout.iter().filter(|(n, _)| *n == name).map(|(_, m)| *m).collect();
            let places: Vec<Mat4> = if entries.is_empty() { vec![Mat4::IDENTITY] } else { entries.into_iter().flatten().collect() };
            for place in places {
                b.add_root(name.clone(), &tf, tid, place);
            }
        }
    }
    Ok(b.out)
}

/// Prefabs given as `(name, (file, GameObject path id), placement)`, each with its children.
pub(crate) fn load_roots(lib: &mut Library, kit: &mut ShapeKit, roots: Vec<(String, (FileRef, i64), Mat4)>) -> Result<RoomData, String> {
    let mut b = Builder::new(lib, kit);
    for (name, (f, gid), place) in roots {
        let Some((tf, tid)) = crate::world::transform_of(b.lib, &f, gid) else {
            b.out.problems.push(format!("{name}: no Transform"));
            continue;
        };
        b.add_root(name, &tf, tid, place);
    }
    Ok(b.out)
}
