//! Builds a room from the game's own prefabs in the owner's install (the Addressables bundles under
//! `RecRoom_Data/StreamingAssets/aa/StandaloneWindows64`), as plain mesh data the game turns into entities.
//!
//! Unity is left-handed (z forward), Bevy right-handed: positions, normals and transforms get z negated, triangles
//! get their winding flipped, and UV v is flipped because textures decode top row first.
use std::{collections::HashMap, path::{Path, PathBuf}, rc::Rc};

use bevy::math::{Mat4, Quat, Vec3};
use unity_assets::{classes, Bundle, GameObject, Material, Mesh, MeshFilter, MeshRenderer, PPtr, SerializedFile, Texture2D, Transform};

use crate::{collide::Obb, shapes::{self, Pixels, ShapeKit}};

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

pub struct RoomMaterial {
    pub name: String,
    /// the shader's name, as the game files give it
    pub shader: String,
    /// sRGB tint multiplied with the texture
    pub color: [f32; 4],
    /// width, height, RGBA8 rows top-down; recolour tints are already baked in
    pub texture: Option<Pixels>,
    /// 1 - smoothness (specular workflow: the spec-gloss map's mean alpha times `_Smoothness`)
    pub roughness: f32,
}

/// How a game shader colours its surface (docs/agents/rc-spec/rendering.md in the private notes).
#[derive(Clone, Copy, PartialEq)]
enum Shading {
    /// `_BaseColor` times `_BaseMap`
    Lit,
    /// `AG/Standard-Opaque-Instanced`: `rgb * lerp(1, vertex colour * _InstanceColor, a)`
    Recolor,
    /// `AG/MakerPenShape`: the prop's SandboxColorable picks a palette colour and a Maker Pen material
    MakerPen,
}

fn shading(shader: &str) -> Shading {
    match shader {
        "AG/MakerPenShape" => Shading::MakerPen,
        "AG/Standard-Opaque-Instanced" => Shading::Recolor,
        _ => Shading::Lit,
    }
}

/// A game material as read, before a prop's colours are applied.
struct Source {
    name: String,
    shader: String,
    color: [f32; 4],
    /// `_InstanceColor`, linear
    instance: [f32; 3],
    texture: Option<Pixels>,
    roughness: f32,
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
    /// a recolour tint from a uniform vertex colour, 8 bits per channel
    Tint([u8; 3]),
    Shape(Colorable),
}

#[derive(Default)]
pub struct RoomData {
    pub parts: Vec<Part>,
    pub materials: Vec<RoomMaterial>,
    pub roots: Vec<String>,
    /// the solid BoxColliders (triggers left out), for walking
    pub colliders: Vec<Obb>,
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

/// Unity's BoxCollider class.
const BOX_COLLIDER: i32 = 65;

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

fn unity_to_bevy(t: &Transform) -> Mat4 {
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
}

impl Builder<'_> {
    fn texture(&mut self, from: &FileRef, p: PPtr) -> Option<Pixels> {
        let (tf, tid) = self.lib.resolve(from, p).ok()?;
        let tex = value(&tf, tid).ok().and_then(|v| Texture2D::from_value(&v, &tf.open.bundle).ok())?;
        let px = tex.decode_rgba8().ok()?;
        Some((tex.width as u32, tex.height as u32, px))
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
        let color_of = |names: &[&str]| m.colors.iter().find(|(n, _)| names.contains(&n.as_str())).map(|c| c.1);
        let color = color_of(&["_BaseColor", "_Color", "_MainColor"]).unwrap_or([1.0; 4]);
        let instance = color_of(&["_InstanceColor"]).map_or([1.0; 3], |c| shapes::linear([c[0], c[1], c[2]]));
        let float = |name: &str| m.floats.iter().find(|(n, _)| n == name).map(|f| f.1);
        let tex = |names: &[&str]| m.textures.iter().filter(|t| t.texture.path_id != 0).find(|t| names.contains(&t.name.as_str())).map(|t| t.texture);
        // Rec Room's own shaders name their albedo differently; take the first likely colour map
        let pick = tex(&["_MainTex", "_BaseMap", "_Albedo", "_BaseColorMap", "_Diffuse"])
            .or_else(|| m.textures.iter().filter(|t| t.texture.path_id != 0).find(|t| {
                let n = t.name.to_ascii_lowercase();
                !(n.contains("norm") || n.contains("bump") || n.contains("spec") || n.contains("mask") || n.contains("ao") || n.contains("light"))
            }).map(|t| t.texture));
        let texture = pick.and_then(|t| self.texture(&mf, t));
        let gloss = tex(&["_SpecGlossMap"]).and_then(|t| self.texture(&mf, t)).map_or(1.0, |t| shapes::mean_alpha(&t));
        let roughness = 1.0 - gloss * float("_Smoothness").or(float("_Glossiness")).unwrap_or(0.2);
        let hidden = ["InteractionVolume", "Mask_Mat", "LightShaft", "Light_Shaft"].iter().any(|k| m.name.contains(k));
        self.sources.push(Source { name: m.name, shader, color, instance, texture, roughness, hidden });
        let i = self.sources.len() - 1;
        self.source_of.insert(key, i);
        i
    }

    fn fallback_source(&mut self) -> usize {
        if let Some(&i) = self.source_of.get(&(String::new(), 0)) {
            return i;
        }
        self.sources.push(Source { name: "missing".into(), shader: String::new(), color: [0.8, 0.8, 0.8, 1.0], instance: [1.0; 3], texture: None, roughness: 0.8, hidden: false });
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
        let (mut name, shader, mut roughness) = (s.name.clone(), s.shader.clone(), s.roughness);
        let (color, texture) = match v {
            Variant::Plain => (s.color, s.texture.clone()),
            Variant::Tint(q) => {
                let tint: [f32; 3] = std::array::from_fn(|c| q[c] as f32 / 255.0 * s.instance[c]);
                match &s.texture {
                    Some(t) => ([1.0; 4], Some(shapes::bake_tint(t, tint))),
                    None => (srgb(tint), None),
                }
            }
            Variant::Shape(c) => {
                let own = [s.color[0], s.color[1], s.color[2]];
                let tint = shapes::linear(c.color.and_then(|id| self.kit.color(id)).unwrap_or(own));
                let shape = c.material.and_then(|id| self.kit.material(id))
                    .map(|m| (m.name.clone(), m.smoothness, m.base_map.clone(), m.spec_map.clone()));
                match shape {
                    Some((mname, smoothness, base_map, spec_map)) => {
                        name = format!("{name} ({mname})");
                        if let Some(gloss) = self.kit.texture(&spec_map).map(shapes::mean_alpha) {
                            roughness = 1.0 - gloss * smoothness;
                        }
                        match self.kit.texture(&base_map) {
                            Some(t) => ([1.0; 4], Some(shapes::bake_tint(t, tint))),
                            None => (srgb(tint), None),
                        }
                    }
                    // the material's own _BaseMap is empty (white) and its _SpecGlossMap black (no gloss)
                    None => {
                        roughness = 1.0;
                        (srgb(tint), None)
                    }
                }
            }
        };
        self.out.materials.push(RoomMaterial { name, shader, color, texture, roughness });
        let i = self.out.materials.len() - 1;
        self.looks.insert((src, v), i);
        i
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
        // a recolour prop's vertex colour is one colour per mesh in practice; it is baked into its texture
        let uniform = colors.first().filter(|c0| colors.iter().all(|c| c == *c0)).map(|c| c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8));
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
            // Maker Pen pieces repeat their material's texture every `size` metres of uv0; others use uv0 as it is
            let (variant, part_colors, size) = match shading(&self.sources[src].shader) {
                Shading::MakerPen => {
                    let size = colorable.material.and_then(|id| self.kit.material(id)).map_or(1.0, |m| m.size);
                    (Variant::Shape(colorable), vec![], size)
                }
                Shading::Recolor => match uniform {
                    Some(q) => (Variant::Tint([q[0], q[1], q[2]]), vec![], 1.0),
                    None => (Variant::Plain, colors.clone(), 1.0),
                },
                Shading::Lit => (Variant::Plain, vec![], 1.0),
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

/// Every root prefab in `bundle` whose name passes `want`. A prefab named in `layout` is placed there (once per
/// entry); the others stay where their own root transform puts them.
pub fn load_prefabs(lib: &mut Library, kit: &mut ShapeKit, dir: &Path, bundle: &str, want: impl Fn(&str) -> bool, layout: &[(String, Option<Mat4>)]) -> Result<RoomData, String> {
    let open = lib.open_bundle(&dir.join(bundle))?;
    let mut b = Builder { lib, kit, out: RoomData::default(), sources: vec![], source_of: HashMap::new(), shaders: HashMap::new(), looks: HashMap::new() };
    for index in 0..open.files.len() {
        let f = FileRef { open: open.clone(), index };
        let roots: Vec<(String, i64)> = f.sf().objects.iter()
            .filter(|o| o.class_id == classes::GAME_OBJECT)
            .filter_map(|o| Some((f.sf().object_name(o)?, o.path_id)))
            .filter(|(n, _)| want(n))
            .collect();
        for (name, gid) in roots {
            let Ok(go) = value(&f, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string())) else { continue };
            // the GameObject's Transform; only roots (no parent) are prefabs
            let tr = go.components.iter().find_map(|&c| {
                let (cf, cid) = b.lib.resolve(&f, c).ok()?;
                matches!(class_of(&cf, cid), Some(classes::TRANSFORM)).then_some((cf, cid))
            });
            let Some((tf, tid)) = tr else { continue };
            let is_root = value(&tf, tid).ok().and_then(|v| Transform::from_value(&v).ok()).is_some_and(|t| t.parent.path_id == 0);
            if !is_root {
                continue;
            }
            let entries: Vec<Option<Mat4>> = layout.iter().filter(|(n, _)| *n == name).map(|(_, m)| *m).collect();
            let places: Vec<Mat4> = if entries.is_empty() { vec![Mat4::IDENTITY] } else { entries.into_iter().flatten().collect() };
            for place in places {
                b.out.roots.push(name.clone());
                b.walk(&tf, tid, place, Colorable::default());
            }
        }
    }
    Ok(b.out)
}
