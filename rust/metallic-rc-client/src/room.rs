//! Builds a room from the game's own prefabs in the owner's install (the Addressables bundles under
//! `RecRoom_Data/StreamingAssets/aa/StandaloneWindows64`), as plain mesh data the game turns into entities.
//!
//! Unity is left-handed (z forward), Bevy right-handed: positions, normals and transforms get z negated, triangles
//! get their winding flipped, and UV v is flipped because textures decode top row first.
use std::{collections::HashMap, path::{Path, PathBuf}, rc::Rc};

use bevy::math::{Mat4, Quat, Vec3};
use unity_assets::{classes, Bundle, GameObject, Material, Mesh, MeshFilter, MeshRenderer, PPtr, SerializedFile, Texture2D, Transform};

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
    pub world: Mat4,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// vertex colours (Maker Pen shapes carry their colour here); empty when the mesh has none
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: usize,
}

pub struct RoomMaterial {
    pub name: String,
    pub color: [f32; 4],
    /// width, height, RGBA8 rows top-down
    pub texture: Option<(u32, u32, Vec<u8>)>,
    /// interaction volumes and UI masks: present in the prefab, never seen in the game
    pub hidden: bool,
}

#[derive(Default)]
pub struct RoomData {
    pub parts: Vec<Part>,
    pub materials: Vec<RoomMaterial>,
    pub roots: Vec<String>,
    pub problems: Vec<String>,
}

struct Open {
    bundle: Bundle,
    files: Vec<(String, SerializedFile)>,
}

/// A serialized file inside an open bundle.
#[derive(Clone)]
struct FileRef {
    open: Rc<Open>,
    index: usize,
}

impl FileRef {
    fn sf(&self) -> &SerializedFile {
        &self.open.files[self.index].1
    }
    fn cab(&self) -> &str {
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

    fn open_bundle(&mut self, path: &Path) -> Result<Rc<Open>, String> {
        if let Some(o) = self.open.get(path) {
            return Ok(o.clone());
        }
        let bundle = Bundle::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let files = bundle.serialized_files().map_err(|e| format!("{}: {e}", path.display()))?;
        let o = Rc::new(Open { bundle, files });
        self.open.insert(path.to_path_buf(), o.clone());
        Ok(o)
    }

    fn file(&mut self, cab: &str) -> Result<FileRef, String> {
        let path = self.cabs.get(cab).cloned().ok_or_else(|| format!("{cab} is in no bundle"))?;
        let open = self.open_bundle(&path)?;
        let index = open.files.iter().position(|(n, _)| n == cab).ok_or_else(|| format!("{cab} missing from its bundle"))?;
        Ok(FileRef { open, index })
    }

    /// The file and path id a PPtr seen in `from` points at.
    fn resolve(&mut self, from: &FileRef, p: PPtr) -> Result<(FileRef, i64), String> {
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

fn class_of(f: &FileRef, path_id: i64) -> Option<i32> {
    f.sf().object(path_id).map(|o| o.class_id)
}

fn value(f: &FileRef, path_id: i64) -> Result<unity_assets::Value, String> {
    let o = f.sf().object(path_id).ok_or_else(|| format!("object {path_id} not in {}", f.cab()))?;
    f.sf().read_value(o).map_err(|e| e.to_string())
}

fn unity_to_bevy(t: &Transform) -> Mat4 {
    let [px, py, pz] = t.local_position;
    let [qx, qy, qz, qw] = t.local_rotation;
    Mat4::from_scale_rotation_translation(Vec3::from(t.local_scale), Quat::from_xyzw(-qx, -qy, qz, qw), Vec3::new(px, py, -pz))
}

struct Builder<'a> {
    lib: &'a mut Library,
    out: RoomData,
    materials: HashMap<(String, i64), usize>,
}

impl Builder<'_> {
    fn material(&mut self, f: &FileRef, p: PPtr) -> usize {
        let Ok((mf, id)) = self.lib.resolve(f, p) else { return self.fallback_material() };
        let key = (mf.cab().to_string(), id);
        if let Some(&i) = self.materials.get(&key) {
            return i;
        }
        let m = match value(&mf, id).and_then(|v| Material::from_value(&v).map_err(|e| e.to_string())) {
            Ok(m) => m,
            Err(e) => {
                self.out.problems.push(format!("material {id}: {e}"));
                return self.fallback_material();
            }
        };
        let color = m.colors.iter().find(|(n, _)| matches!(n.as_str(), "_Color" | "_BaseColor" | "_MainColor")).map(|c| c.1).unwrap_or([1.0; 4]);
        // Rec Room's own shaders name their albedo differently; take the first likely colour map
        let pick = m.textures.iter()
            .filter(|t| t.texture.path_id != 0)
            .find(|t| ["_MainTex", "_BaseMap", "_Albedo", "_BaseColorMap", "_Diffuse"].contains(&t.name.as_str()))
            .or_else(|| m.textures.iter().filter(|t| t.texture.path_id != 0).find(|t| {
                let n = t.name.to_ascii_lowercase();
                !(n.contains("norm") || n.contains("bump") || n.contains("spec") || n.contains("mask") || n.contains("ao") || n.contains("light"))
            }));
        let texture = pick.and_then(|t| {
            let (tf, tid) = self.lib.resolve(&mf, t.texture).ok()?;
            let tex = value(&tf, tid).ok().and_then(|v| Texture2D::from_value(&v, &tf.open.bundle).ok())?;
            let px = tex.decode_rgba8().ok()?;
            Some((tex.width as u32, tex.height as u32, px))
        });
        let hidden = ["InteractionVolume", "Mask_Mat", "LightShaft", "Light_Shaft"].iter().any(|k| m.name.contains(k));
        self.out.materials.push(RoomMaterial { name: m.name, color, texture, hidden });
        let i = self.out.materials.len() - 1;
        self.materials.insert(key, i);
        i
    }

    fn fallback_material(&mut self) -> usize {
        if let Some(&i) = self.materials.get(&(String::new(), 0)) {
            return i;
        }
        self.out.materials.push(RoomMaterial { name: "missing".into(), color: [0.8, 0.8, 0.8, 1.0], texture: None, hidden: false });
        let i = self.out.materials.len() - 1;
        self.materials.insert((String::new(), 0), i);
        i
    }

    /// Adds a GameObject (given by its Transform) and its active children.
    fn walk(&mut self, f: &FileRef, transform_id: i64, parent: Mat4) {
        let Ok(t) = value(f, transform_id).and_then(|v| Transform::from_value(&v).map_err(|e| e.to_string())) else { return };
        let Ok((gf, gid)) = self.lib.resolve(f, t.game_object) else { return };
        let Ok(go) = value(&gf, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string())) else { return };
        // lower levels of detail would draw on top of LOD0
        let lod = go.name.rfind("LOD").map(|i| &go.name[i + 3..]);
        if !go.active || lod.is_some_and(|n| !n.is_empty() && n != "0" && n.chars().all(|c| c.is_ascii_digit())) {
            return;
        }
        let world = parent * unity_to_bevy(&t);

        let (mut filter, mut renderer) = (None, None);
        for &c in &go.components {
            let Ok((cf, cid)) = self.lib.resolve(&gf, c) else { continue };
            match class_of(&cf, cid) {
                Some(classes::MESH_FILTER) => filter = value(&cf, cid).ok().and_then(|v| MeshFilter::from_value(&v).ok()).map(|m| (cf, m)),
                Some(classes::MESH_RENDERER) => renderer = value(&cf, cid).ok().and_then(|v| MeshRenderer::from_value(&v).ok()).map(|r| (cf, r)),
                _ => {}
            }
        }
        if let (Some((ff, filter)), Some((rf, renderer))) = (filter, renderer) {
            if renderer.enabled {
                self.add_mesh(&go.name, world, &ff, filter.mesh, &rf, &renderer.materials);
            }
        }
        for &child in &t.children {
            if let Ok((cf, cid)) = self.lib.resolve(f, child) {
                self.walk(&cf, cid, world);
            }
        }
    }

    fn add_mesh(&mut self, name: &str, world: Mat4, ff: &FileRef, mesh: PPtr, rf: &FileRef, materials: &[PPtr]) {
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
        let mut uvs: Vec<_> = m.uv(0).into_iter().map(|[u, v]| [u, 1.0 - v]).collect();
        uvs.resize(positions.len(), [0.0, 0.0]);
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
            let material = match materials.get(i).or(materials.last()) {
                Some(&p) => self.material(rf, p),
                None => self.fallback_material(),
            };
            if self.out.materials[material].hidden {
                continue;
            }
            self.out.parts.push(Part {
                name: name.to_string(),
                root: self.out.roots.len() - 1,
                world,
                positions: positions.clone(),
                normals: normals.clone(),
                uvs: uvs.clone(),
                colors: colors.clone(),
                indices: idx,
                material,
            });
        }
    }
}

/// Where to put prefabs that are not authored in place, read from a layout file: one prefab per line,
/// `name x y z [yaw [pitch [roll]]]`, metres and degrees in the client's (Bevy) space, `#` starts a comment.
/// A name may appear more than once to place several copies; `name hide` leaves a prefab out.
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
        if v.len() < 3 || v.len() > 6 {
            return Err(format!("layout line {}: want name x y z [yaw [pitch [roll]]]", n + 1));
        }
        let deg = |i: usize| v.get(i).copied().unwrap_or(0.0).to_radians();
        let rot = Quat::from_euler(bevy::math::EulerRot::YXZ, deg(3), deg(4), deg(5));
        out.push((name, Some(Mat4::from_rotation_translation(rot, Vec3::new(v[0], v[1], v[2])))));
    }
    Ok(out)
}

/// Every root prefab in `bundle` whose name passes `want`. A prefab named in `layout` is placed there (once per
/// entry); the others stay where their own root transform puts them.
pub fn load_prefabs(lib: &mut Library, dir: &Path, bundle: &str, want: impl Fn(&str) -> bool, layout: &[(String, Option<Mat4>)]) -> Result<RoomData, String> {
    let open = lib.open_bundle(&dir.join(bundle))?;
    let mut b = Builder { lib, out: RoomData::default(), materials: HashMap::new() };
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
                b.walk(&tf, tid, place);
            }
        }
    }
    Ok(b.out)
}
