//! Rooms laid out and lit from the plain scene files of an older install (the "reference install", `--ref-install`,
//! by default `C:\Meow.Net` when it exists): its `levelN` files are ordinary Unity scenes (2019.4, no type trees),
//! while the 2026 install's are scrambled. The install is only read.
//!
//! - [`layout`]: every piece of a room where the old scene puts it. A 2026 Addressables prefab with the same mesh
//!   (or the Dorm's hull slices, or an anchor object) is placed where the old scene has it; what only the old scene
//!   has is drawn from the old files (mesh and material).
//! - [`lighting`]: the scene's RenderSettings (ambient, fog, sky, reflection), lights, reflection probes, light
//!   probes and lightmaps, for the renderer.
//! - [`load`]: the room as [`RoomData`], used by `world::load` in place of our `rooms/*.layout` files.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::OnceLock,
};

use bevy::math::{Mat4, Quat, Vec3};
use unity_assets::{classes, level, Material, PPtr, Reader, SerializedFile, TypeTree};

use crate::{
    collide::{Collider, MeshShape, Obb},
    room::{self, Blend, Library, ParticleEmitter, Part, RoomData, RoomLight, RoomMaterial},
    shapes::{self, Pixels, ShapeKit},
};

static REF_INSTALL: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The default reference install.
pub const DEFAULT_REF_INSTALL: &str = r"C:\Meow.Net";

/// Sets the reference install from `--ref-install` (the install folder or its `RecRoom_Data`); `none` turns the
/// levels off. Call once, before the first room loads; later calls are ignored.
pub fn set_ref_install(arg: Option<&str>) {
    let dir = match arg {
        Some(a) if a.eq_ignore_ascii_case("none") => None,
        Some(a) => Some(data_dir(Path::new(a))),
        None => Some(data_dir(Path::new(DEFAULT_REF_INSTALL))).filter(|d| d.join("globalgamemanagers").exists()),
    };
    let _ = REF_INSTALL.set(dir);
}

fn data_dir(p: &Path) -> PathBuf {
    if p.join("RecRoom_Data").is_dir() { p.join("RecRoom_Data") } else { p.to_path_buf() }
}

/// The reference install's `RecRoom_Data`, if there is one.
pub fn ref_install() -> Option<&'static Path> {
    REF_INSTALL.get_or_init(|| Some(data_dir(Path::new(DEFAULT_REF_INSTALL))).filter(|d| d.join("globalgamemanagers").exists())).as_deref()
}

/// A tag for the world cache key: rooms built from the reference install differ from those built without it.
pub fn cache_tag() -> String {
    format!("levels2:{:?}", ref_install())
}

/// Per-room knowledge for matching the old scene to the 2026 prefabs.
struct RoomInfo {
    /// our room name (`--world`) and the old scene's file name without `.unity`
    name: &'static str,
    scene: &'static str,
    /// the 2026 prefab group (`world::GROUPS` name) whose pieces the old scene places, if any
    group: Option<&'static str>,
    /// the scene object the 2026 hull slices are cut from (its path)
    hull: Option<&'static str>,
    /// 2026 prefabs placed at a scene object (2026 replaced their meshes): prefab, object path; scale dropped
    anchors: &'static [(&'static str, &'static str)],
}

const ROOMS: &[RoomInfo] = &[
    RoomInfo {
        name: "dorm",
        scene: "dormroom2",
        group: Some("dorm"),
        hull: Some("Enviroment/Static/[DormRoom2Env]/DormRoom2_fbx"),
        anchors: &[
            ("(Dorm_FreeMirror)", "Enviroment/Static/[DormRoom2Env]/DormRoom2_fbx/MirrorHolder/Quad"),
            ("(Dorm_Mirror_Buttons)", "Systems/[MirrorUIRoot]/Canvas/RRUIRoot/MirrorButtons"),
            ("(Dorm_Fluorescent_Light)", "Enviroment/Static/[Light_Fluorescent]"),
            ("(Dorm_Community_Board)", "Systems/MessageBoard"),
            ("(Dorm_Help_Buttons)", "Systems/[HelpButtonsCanvas]"),
        ],
    },
    RoomInfo { name: "reccenter", scene: "reccenter", group: None, hull: None, anchors: &[] },
];

/// Anchored prefabs that face the other way from their anchor: the free mirror's glass faces its prefab's -Z while
/// the old scene's Quad faces +Z, so unturned the frame's back hides the glass (look lane, seen in the client).
const TURNED: [&str; 1] = ["(Dorm_FreeMirror)"];

/// The Dorm's hull slices in 2026 (`Dorm_NN`, trims, mezzanine): cut from the old scene's one hull mesh, in its frame.
fn is_hull_slice(mesh: &str) -> bool {
    let rest = mesh.strip_prefix("Dorm_").unwrap_or("");
    rest.starts_with(|c: char| c.is_ascii_digit()) || rest.starts_with("Trim") || rest.starts_with("Mezzanine")
}

/// Meshes shared by many unrelated objects: a match on them says nothing about where a prefab goes.
const GENERIC: [&str; 4] = ["ButtonRectangle", "LOD0", "LOD1", "LOD2"];

/// 2026 prefabs left out: the `_Up`/`_Down` wall halves duplicate the full walls; the light shafts come from the old
/// scene's light planes. INFERRED (as in rooms/dorm.layout).
fn hidden_prefab(name: &str) -> bool {
    let n = name.trim_end_matches(')');
    n.ends_with("_Up") || n.ends_with("_Down") || n == "(Dorm_Light_Shafts"
}

// ---- the reference install's files ----

/// The plain files of a plain-file install (the 2019 reference install, or the decrypted 2026 corpus), opened as
/// they are needed (resolving PPtrs between them). `res_dirs` are where the out-of-line `.resS`/`.resource` streams
/// are looked for (the install folder; for the 2026 corpus also the game install, whose `.resS` are plaintext).
struct Files {
    dir: PathBuf,
    res_dirs: Vec<PathBuf>,
    open: HashMap<String, Option<Rc<SerializedFile>>>,
}

impl Files {
    fn new(dir: &Path) -> Self {
        Files { dir: dir.to_path_buf(), res_dirs: vec![dir.to_path_buf()], open: HashMap::new() }
    }

    /// Reads scenes from `dir` but looks for their stream data across `res_dirs` (first hit wins).
    fn with_res(dir: &Path, res_dirs: Vec<PathBuf>) -> Self {
        Files { dir: dir.to_path_buf(), res_dirs, open: HashMap::new() }
    }

    fn get(&mut self, name: &str) -> Option<Rc<SerializedFile>> {
        let dir = &self.dir;
        self.open
            .entry(name.to_string())
            .or_insert_with(|| {
                let data = std::fs::read(dir.join(name)).ok()?;
                SerializedFile::parse(data).ok().map(Rc::new)
            })
            .clone()
    }

    /// The file and path id a PPtr in `from` points at; `None` for null, built-in (`Library/...`) or missing files.
    fn resolve(&mut self, from: &str, p: PPtr) -> Option<(String, i64)> {
        if p.path_id == 0 {
            return None;
        }
        if p.file_id == 0 {
            return Some((from.to_string(), p.path_id));
        }
        let sf = self.get(from)?;
        let ext = sf.externals.get(p.file_id as usize - 1)?;
        if ext.path.starts_with("Library/") || ext.path.contains("unity_builtin_extra") {
            return None;
        }
        let name = ext.path.rsplit('/').next().unwrap_or(&ext.path).to_string();
        Some((name, p.path_id))
    }

    fn resources(&self) -> level::MultiResources {
        level::MultiResources(self.res_dirs.clone())
    }
}

// ---- the scene ----

/// One GameObject of the old scene.
pub struct Node {
    /// `Root/Child/...`
    pub path: String,
    pub name: String,
    pub parent: Option<usize>,
    /// in the client's (Bevy) space
    pub world: Mat4,
    /// active in the hierarchy
    pub active: bool,
    /// its GameObject and components (class id, path id), in the scene file
    pub game_object: i64,
    pub components: Vec<(i32, i64)>,
}

pub struct Scene {
    /// the scene's file name without `.unity`, and the level file
    pub name: String,
    pub file: String,
    pub nodes: Vec<Node>,
}

impl Scene {
    fn find(&self, path: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.path == path)
    }

    fn component(&self, i: usize, class: i32) -> Option<i64> {
        self.nodes[i].components.iter().find(|c| c.0 == class).map(|c| c.1)
    }

    /// Whether `i` is `ancestor` or under it.
    fn under(&self, mut i: usize, ancestor: usize) -> bool {
        loop {
            if i == ancestor {
                return true;
            }
            match self.nodes[i].parent {
                Some(p) => i = p,
                None => return false,
            }
        }
    }
}

/// The level file of a room: our room name (`dorm`, `reccenter`), a scene name (`paintball`), or `levelN`.
fn scene_file(files: &mut Files, room: &str) -> Option<(String, String)> {
    let ggm = files.get("globalgamemanagers")?;
    let scenes = level::build_scenes(&ggm).ok()?;
    let want = ROOMS.iter().find(|r| r.name.eq_ignore_ascii_case(room)).map_or(room, |r| r.scene);
    let i = scenes.iter().position(|s| level::scene_name(s).eq_ignore_ascii_case(want))
        .or_else(|| room.strip_prefix("level")?.parse::<usize>().ok().filter(|&i| i < scenes.len()))?;
    Some((format!("level{i}"), level::scene_name(&scenes[i]).to_string()))
}

fn read_scene(files: &mut Files, file: &str, name: &str) -> Result<Scene, String> {
    let sf = files.get(file).ok_or_else(|| format!("{file}: cannot read"))?;
    let mut transforms = HashMap::new();
    for o in sf.objects.iter().filter(|o| o.class_id == classes::TRANSFORM || o.class_id == classes::RECT_TRANSFORM) {
        if let Ok(t) = sf.transform(o) {
            transforms.insert(o.path_id, t);
        }
    }
    let mut roots: Vec<i64> = transforms.iter().filter(|(_, t)| t.parent.is_null()).map(|(&id, _)| id).collect();
    roots.sort_by_key(|id| sf.objects.iter().position(|o| o.path_id == *id));
    let mut nodes = Vec::new();
    let mut stack: Vec<(i64, Option<usize>)> = roots.into_iter().rev().map(|r| (r, None)).collect();
    while let Some((tid, parent)) = stack.pop() {
        let Some(t) = transforms.get(&tid) else { continue };
        let Some(go) = sf.object(t.game_object.path_id).and_then(|o| sf.game_object(o).ok()) else { continue };
        let (pw, pa, pp) = parent.map_or((Mat4::IDENTITY, true, String::new()), |p: usize| {
            let n: &Node = &nodes[p];
            (n.world, n.active, format!("{}/", n.path))
        });
        let components = go.components.iter().filter_map(|c| sf.object(c.path_id).map(|o| (o.class_id, o.path_id))).collect();
        nodes.push(Node {
            path: format!("{pp}{}", go.name),
            name: go.name.clone(),
            parent,
            world: pw * room::unity_to_bevy(t),
            active: pa && go.active,
            game_object: t.game_object.path_id,
            components,
        });
        let me = nodes.len() - 1;
        for c in t.children.iter().rev() {
            stack.push((c.path_id, Some(me)));
        }
    }
    Ok(Scene { name: name.to_string(), file: file.to_string(), nodes })
}

/// The mesh a node draws: (mesh name, renderer). `None` without a MeshFilter and MeshRenderer.
fn node_mesh(files: &mut Files, scene: &Scene, sf: &SerializedFile, i: usize) -> Option<(String, unity_assets::MeshFilter, unity_assets::MeshRenderer)> {
    let filter = sf.mesh_filter(sf.object(scene.component(i, classes::MESH_FILTER)?)?).ok()?;
    let renderer = sf.mesh_renderer(sf.object(scene.component(i, classes::MESH_RENDERER)?)?).ok()?;
    let name = files.resolve(&scene.file, filter.mesh)
        .and_then(|(f, id)| { let x = files.get(&f)?; x.object(id).and_then(|o| x.object_name(o)) })
        .unwrap_or_default();
    Some((name, filter, renderer))
}

/// Every drawn mesh of a scene, by node index and mesh name: active renderers (or statically-batched originals
/// whose combined mesh draws them), skipping lower LODs, mobile variants, circuit helpers and shadow-only proxies.
fn drawn_meshes(files: &mut Files, scene: &Scene) -> Vec<(usize, String)> {
    let Some(sf) = files.get(&scene.file) else { return Vec::new() };
    let mut drawn: Vec<(usize, String)> = Vec::new();
    // Rec Room's own static batcher: originals keep their mesh with the renderer off, and a `StaticStaticBatch_<material>`
    // object draws the combined mesh; such originals count as drawn (INFERRED from the object names).
    let batched: HashSet<String> = scene.nodes.iter().filter_map(|n| n.name.strip_prefix("StaticStaticBatch_").map(String::from)).collect();
    let desktop_parents: HashSet<Option<usize>> = scene.nodes.iter().filter(|n| n.name.contains("Desktop")).map(|n| n.parent).collect();
    // Authoritative LOD selection: a LODGroup names the renderers at each level, so every renderer referenced at
    // LOD1+ is a lower detail level and must not be drawn (LOD0 stays). This is exact where the `*LODn` name
    // heuristic below is only a guess, and catches lower-LOD renderers that are not named with the LOD convention.
    let lower_lods: HashSet<i64> = sf
        .objects
        .iter()
        .filter(|o| o.class_id == classes::LOD_GROUP)
        .filter_map(|o| level::lod_group(&sf, o).ok())
        .flat_map(|g| g.lods.into_iter().skip(1).flatten().map(|p| p.path_id))
        .collect();
    for (i, n) in scene.nodes.iter().enumerate() {
        let renderer = n.components.iter().find(|c| c.0 == classes::MESH_RENDERER || c.0 == classes::SKINNED_MESH_RENDERER).map(|c| c.1);
        if !n.active || n.name.starts_with("StaticStaticBatch_") || lower_lod(&n.name) || mobile_variant(scene, &desktop_parents, i)
            || n.path.contains("/Circuit ") || renderer.is_some_and(|id| lower_lods.contains(&id))
        {
            continue;
        }
        let Some((mesh, _, r)) = node_mesh(files, scene, &sf, i) else { continue };
        if r.cast_shadows == 3 {
            continue;
        }
        let first_mat = r.materials.first().and_then(|&p| files.resolve(&scene.file, p))
            .and_then(|(f, id)| { let x = files.get(&f)?; x.object(id).and_then(|o| x.object_name(o)) });
        if r.enabled || first_mat.is_some_and(|m| batched.contains(&m)) {
            drawn.push((i, mesh));
        }
    }
    drawn
}

/// A lower level of detail (`*LOD1`, `*_LOD_2`): LOD0 is drawn instead.
fn lower_lod(name: &str) -> bool {
    let Some(i) = name.rfind("LOD") else { return false };
    let rest = name[i + 3..].trim_start_matches('_');
    !rest.is_empty() && rest != "0" && rest.chars().all(|c| c.is_ascii_digit())
}

/// Under a `*Mobile*` object with a `*Desktop*` sibling: the game shows one of the two by platform; we draw the
/// desktop one. INFERRED from the names.
fn mobile_variant(scene: &Scene, desktop_parents: &HashSet<Option<usize>>, mut i: usize) -> bool {
    loop {
        let n = &scene.nodes[i];
        if n.name.contains("Mobile") && desktop_parents.contains(&n.parent) {
            return true;
        }
        match n.parent {
            Some(p) => i = p,
            None => return false,
        }
    }
}

// ---- layout ----

/// What goes where.
#[derive(Clone, Debug)]
pub enum Piece {
    /// a 2026 Addressables prefab (by root name), placed at `world`
    Prefab(String),
    /// a 2026 prefab of the room's group left out
    Hidden(String),
    /// an old-scene object drawn from the old files: its node index in the scene
    Mesh(usize),
}

#[derive(Clone, Debug)]
pub struct Placed {
    pub piece: Piece,
    /// placement in the client's space (for `Mesh`: the node's world matrix)
    pub world: Mat4,
    /// the scene object it came from, or why it is where it is
    pub from: String,
}

/// The meshes of each root prefab of a 2026 group: (root name, [(mesh name, world under an identity placement)]).
fn prefab_meshes(lib: &mut Library, dir: &Path, bundle: &str, prefix: &str) -> Result<Vec<(String, Vec<(String, Mat4)>)>, String> {
    use unity_assets::{GameObject, MeshFilter, Transform};
    let open = lib.open_bundle(&dir.join(bundle))?;
    let mut out = Vec::new();
    for index in 0..open.files.len() {
        let f = room::FileRef { open: open.clone(), index };
        let roots: Vec<(String, i64)> = f.sf().objects.iter()
            .filter(|o| o.class_id == classes::GAME_OBJECT)
            .filter_map(|o| Some((f.sf().object_name(o)?, o.path_id)))
            .filter(|(n, _)| n.starts_with(prefix))
            .collect();
        for (name, gid) in roots {
            let Some((tf, tid)) = crate::world::transform_of(lib, &f, gid) else { continue };
            let is_root = room::value(&tf, tid).ok().and_then(|v| Transform::from_value(&v).ok()).is_some_and(|t| t.parent.path_id == 0);
            if !is_root {
                continue;
            }
            let mut meshes = Vec::new();
            let mut stack = vec![(tf, tid, Mat4::IDENTITY)];
            while let Some((f, tid, parent)) = stack.pop() {
                let Ok(t) = room::value(&f, tid).and_then(|v| Transform::from_value(&v).map_err(|e| e.to_string())) else { continue };
                let world = parent * room::unity_to_bevy(&t);
                let Ok((gf, gid)) = lib.resolve(&f, t.game_object) else { continue };
                let Ok(go) = room::value(&gf, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string())) else { continue };
                if !go.active {
                    continue;
                }
                for &c in &go.components {
                    let Ok((cf, cid)) = lib.resolve(&gf, c) else { continue };
                    if cf.sf().object(cid).map(|o| o.class_id) != Some(classes::MESH_FILTER) {
                        continue;
                    }
                    let mesh = room::value(&cf, cid).ok().and_then(|v| MeshFilter::from_value(&v).ok())
                        .and_then(|m| lib.resolve(&cf, m.mesh).ok())
                        .and_then(|(mf, mid)| mf.sf().object(mid).and_then(|o| mf.sf().object_name(o)));
                    if let Some(m) = mesh {
                        meshes.push((m, world));
                    }
                }
                for &child in &t.children {
                    if let Ok((cf, cid)) = lib.resolve(&f, child) {
                        stack.push((cf, cid, world));
                    }
                }
            }
            out.push((name, meshes));
        }
    }
    Ok(out)
}

/// The old scene's pieces and the 2026 prefabs placed over them, plus the scene itself.
fn plan(files: &mut Files, lib: &mut Library, dir: &Path, room: &str) -> Result<(Scene, Vec<Placed>), String> {
    let (file, name) = scene_file(files, room).ok_or_else(|| format!("no scene {room:?} in the reference install"))?;
    let scene = read_scene(files, &file, &name)?;
    let info = ROOMS.iter().find(|r| r.scene.eq_ignore_ascii_case(&name));
    // every drawn mesh of the scene, by node
    let drawn = drawn_meshes(files, &scene);
    let mut out = Vec::new();
    let mut covered: Vec<usize> = Vec::new(); // anchor subtrees
    let mut covered_meshes: HashSet<String> = HashSet::new();
    let group = info.and_then(|i| i.group).and_then(|g| crate::world::GROUPS.iter().find(|x| x.name == g));
    if let (Some(info), Some(group)) = (info, group) {
        let prefabs = prefab_meshes(lib, dir, group.bundle, group.prefix)?;
        let hull = info.hull.and_then(|p| scene.find(p));
        for (pname, meshes) in prefabs {
            if hidden_prefab(&pname) {
                out.push(Placed { piece: Piece::Hidden(pname), world: Mat4::IDENTITY, from: "left out".into() });
                continue;
            }
            if let Some((_, path)) = info.anchors.iter().find(|(n, _)| *n == pname) {
                if let Some(a) = scene.find(path) {
                    let (_, mut r, t) = scene.nodes[a].world.to_scale_rotation_translation();
                    if TURNED.contains(&pname.as_str()) {
                        r *= Quat::from_rotation_y(std::f32::consts::PI);
                    }
                    out.push(Placed { piece: Piece::Prefab(pname), world: Mat4::from_rotation_translation(r, t), from: path.to_string() });
                    covered.push(a);
                    continue;
                }
            }
            let found = meshes.iter().find_map(|(m, local)| {
                if is_hull_slice(m) {
                    return hull.map(|h| (scene.nodes[h].world * local.inverse(), scene.nodes[h].path.clone()));
                }
                if GENERIC.contains(&m.as_str()) {
                    return None;
                }
                // an active drawn copy if there is one; some props are switched off in the scene
                let i = drawn.iter().find(|(_, dm)| dm == m).map(|d| d.0)?;
                Some((scene.nodes[i].world * local.inverse(), scene.nodes[i].path.clone()))
            });
            match found {
                Some((w, from)) => {
                    for (m, _) in &meshes {
                        if !GENERIC.contains(&m.as_str()) {
                            covered_meshes.insert(m.clone());
                        }
                        if is_hull_slice(m) {
                            if let Some(h) = hull {
                                covered.push(h);
                            }
                        }
                    }
                    out.push(Placed { piece: Piece::Prefab(pname), world: w, from });
                }
                // not in the old scene: a 2026 addition, left where its own root puts it
                None => out.push(Placed { piece: Piece::Prefab(pname), world: Mat4::IDENTITY, from: "not in the old scene".into() }),
            }
        }
    }
    let hull_mesh = info.and_then(|i| i.hull).and_then(|p| scene.find(p)).filter(|h| covered.contains(h));
    for (i, mesh) in drawn {
        // the room's systems (UI, tutorials, boards) are not part of its look
        if scene.nodes[i].path.starts_with("Systems/") {
            continue;
        }
        if covered_meshes.contains(&mesh) || hull_mesh == Some(i) || covered.iter().any(|&a| Some(a) != hull_mesh && scene.under(i, a)) {
            continue;
        }
        out.push(Placed { piece: Piece::Mesh(i), world: scene.nodes[i].world, from: scene.nodes[i].path.clone() });
    }
    Ok((scene, out))
}

/// Where everything of `room` goes, from the reference install's scene. `dir` is the 2026 bundle folder.
pub fn layout(lib: &mut Library, dir: &Path, room: &str) -> Result<Vec<Placed>, String> {
    let data = ref_install().ok_or("no reference install")?;
    plan(&mut Files::new(data), lib, dir, room).map(|p| p.1)
}

// ---- materials and meshes of the old files ----

#[derive(Clone, Copy, PartialEq, Debug)]
enum Family {
    Lit,
    Instanced,
    Standard,
    Unlit,
}

fn family(shader: &str) -> Family {
    if shader.starts_with("AG/Standard-Opaque-Instanced") {
        Family::Instanced
    } else if shader.starts_with("AG/Standard") || shader.starts_with("AG/MakerPenShape") {
        Family::Standard
    } else if shader.starts_with("AG/Unlit") || shader.starts_with("AG/Mirror") {
        Family::Unlit
    } else {
        Family::Lit
    }
}

fn blend_of(shader: &str, name: &str, cutoff: Option<f32>) -> Blend {
    if shader.contains("Additive") || name.contains("LightShaft") {
        Blend::Add
    } else if shader.contains("Transparent") {
        Blend::Blend
    } else if shader.contains("AlphaClip") || shader.contains("Cutout") {
        Blend::Mask(cutoff.unwrap_or(0.5))
    } else {
        Blend::Opaque
    }
}

/// A Shader's name without its type tree: the parsed form's `m_Name` is a length-prefixed string with a `/` just
/// before the fallback name (`Hidden/...`). INFERRED (a byte scan, right for every shader the Dorm uses).
fn shader_name(sf: &SerializedFile, id: i64) -> String {
    sf.object(id).map_or_else(String::new, |o| scan_shader_name(sf.object_bytes(o)))
}

fn scan_shader_name(b: &[u8]) -> String {
    let mut found: Vec<&str> = Vec::new();
    let mut i = 4;
    while i + 4 <= b.len() {
        let n = u32::from_le_bytes([b[i - 4], b[i - 3], b[i - 2], b[i - 1]]) as usize;
        if (4..=100).contains(&n) && i + n <= b.len() {
            let s = &b[i..i + n];
            if s.iter().all(|c| (0x20..0x7f).contains(c)) && s.contains(&b'/') {
                found.push(std::str::from_utf8(s).unwrap_or_default());
                i += n.div_ceil(4) * 4 + 4;
                continue;
            }
        }
        i += 4;
    }
    match found.as_slice() {
        [.., name, last] if last.starts_with("Hidden/") && !name.starts_with("Hidden/") => name.to_string(),
        [.., last] => last.to_string(),
        [] => String::new(),
    }
}

/// Largest side of an old-scene texture once decoded: big rooms (the Rec Center has hundreds of materials, each
/// with base, spec and normal maps baked into new copies) ran the PC out of memory at full size.
const MAX_TEXTURE: u32 = 512;

/// Halves a texture (2x2 box filter) until its longer side is at most `max`.
fn shrink(mut t: Pixels, max: u32) -> Pixels {
    while t.0.max(t.1) > max && t.0 >= 2 && t.1 >= 2 {
        let (w, h) = (t.0 / 2, t.1 / 2);
        let src = &t.2;
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                for c in 0..4 {
                    let at = |dx: usize, dy: usize| src[((2 * y + dy) * t.0 as usize + 2 * x + dx) * 4 + c] as u32;
                    out[(y * w as usize + x) * 4 + c] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                }
            }
        }
        t = (w, h, out);
    }
    t
}

/// Builds the old scene's own pieces into a [`RoomData`].
struct OldBuilder<'a> {
    files: &'a mut Files,
    out: &'a mut RoomData,
    root: usize,
    textures: HashMap<(String, i64), Option<Pixels>>,
    materials: HashMap<(String, i64, [u8; 3]), Option<usize>>,
    meshes: HashMap<(String, i64), Option<Rc<Decoded>>>,
}

impl OldBuilder<'_> {
    fn texture(&mut self, from: &str, p: PPtr) -> Option<Pixels> {
        let (f, id) = self.files.resolve(from, p)?;
        if let Some(t) = self.textures.get(&(f.clone(), id)) {
            return t.clone();
        }
        let res = self.files.resources();
        let t = self.files.get(&f).and_then(|sf| {
            let tex = level::texture(&sf, sf.object(id)?, &res).ok()?;
            let rgba = tex.decode_rgba8().ok()?;
            Some(shrink((tex.width as u32, tex.height as u32, rgba), MAX_TEXTURE))
        });
        self.textures.insert((f, id), t.clone());
        t
    }

    /// The drawn material for a material and a uniform vertex colour (white for none); `None` = not drawn.
    fn material(&mut self, from: &str, p: PPtr, vc: [u8; 3]) -> Option<usize> {
        let (f, id) = self.files.resolve(from, p)?;
        let key = (f.clone(), id, vc);
        if let Some(&m) = self.materials.get(&key) {
            return m;
        }
        let built = self.build_material(&f, id, vc);
        let i = built.map(|m| {
            self.out.materials.push(m);
            self.out.materials.len() - 1
        });
        self.materials.insert(key, i);
        i
    }

    /// The same rules as the 2026 materials (room.rs), for the plain variant of a material.
    fn build_material(&mut self, f: &str, id: i64, vc: [u8; 3]) -> Option<RoomMaterial> {
        let sf = self.files.get(f)?;
        let m: Material = level::material(&sf, sf.object(id)?).ok()?;
        if ["InteractionVolume", "Mask_Mat", "InvisibleOutline"].iter().any(|k| m.name.contains(k)) {
            return None;
        }
        let shader = self.files.resolve(f, m.shader).and_then(|(sfn, sid)| self.files.get(&sfn).map(|x| shader_name(&x, sid))).unwrap_or_default();
        let fam = family(&shader);
        let color_of = |names: &[&str]| m.colors.iter().find(|(n, _)| names.contains(&n.as_str())).map(|c| c.1);
        let float = |name: &str| m.floats.iter().find(|(n, _)| n == name).map(|f| f.1);
        let env = |names: &[&str]| m.textures.iter().filter(|t| t.texture.path_id != 0).find(|t| names.contains(&t.name.as_str())).cloned();
        let tint4 = match fam {
            Family::Instanced => color_of(&["_InstanceColor"]).unwrap_or([1.0; 4]),
            // the 2019 additive (light shaft) materials tint with _TintColor
            Family::Unlit => color_of(&["_InstanceColor", "_TintColor"]).unwrap_or([1.0; 4]),
            _ => color_of(&["_BaseColor", "_Color", "_TintColor"]).unwrap_or([1.0; 4]),
        };
        let mut tint = shapes::linear(std::array::from_fn(|c| tint4[c] * vc[c] as f32 / 255.0));
        let blend = blend_of(&shader, &m.name, float("_Cutoff"));
        if fam == Family::Unlit && blend == Blend::Add {
            // additive particle-style shaders draw 2 * tint * texture (INFERRED from Unity's legacy additive shaders)
            tint = tint.map(|x| x * 2.0);
        }
        let base = env(&["_BaseMap", "_MainTex"]);
        let (sc, of) = base.as_ref().map_or(([1.0; 2], [0.0; 2]), |t| (t.scale, t.offset));
        let base = base.and_then(|t| self.texture(f, t.texture));
        let spec_map = env(&["_SpecGlossMap"]).and_then(|t| self.texture(f, t.texture));
        let normal_map = env(&["_BumpMap", "_NormalMap"]).and_then(|t| self.texture(f, t.texture)).map(|t| shapes::unpack_normals(&t));
        let smoothness = float("_Smoothness").or(float("_Glossiness")).unwrap_or(0.5);
        let spec_color = color_of(&["_SpecColor"]).map_or([1.0; 3], |c| shapes::linear([c[0], c[1], c[2]]));
        let emissive = color_of(&["_EmissionColor"]).filter(|_| m.keywords.iter().any(|k| k.contains("EMISSION")))
            .map_or([0.0; 3], |c| shapes::linear([c[0], c[1], c[2]]));
        let emissive_map = env(&["_EmissionMap"]).filter(|_| emissive != [0.0; 3]).and_then(|t| self.texture(f, t.texture));
        // the shader inputs, unbaked: render.rs's AG surface shader applies the mask, tint, gloss and emission
        let masked = matches!(fam, Family::Instanced | Family::Standard);
        let rgb = tint.map(|x| if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 });
        Some(RoomMaterial {
            name: format!("{} (2019)", m.name),
            shader,
            color: [rgb[0], rgb[1], rgb[2], tint4[3]],
            masked,
            texture: base,
            spec_map,
            spec_color,
            smoothness,
            normal_map,
            emissive,
            emissive_map,
            emissive_power: if masked { float("_EmissivePower").unwrap_or(0.0) } else { 0.0 },
            blend,
            unlit: fam == Family::Unlit || blend == Blend::Add,
            uv_scale: sc,
            uv_offset: [of[0], 1.0 - sc[1] - of[1]],
        })
    }

    fn mesh(&mut self, from: &str, p: PPtr) -> Option<Rc<Decoded>> {
        let (f, id) = self.files.resolve(from, p)?;
        if let Some(m) = self.meshes.get(&(f.clone(), id)) {
            return m.clone();
        }
        let res = self.files.resources();
        let m = self.files.get(&f).and_then(|sf| level::mesh(&sf, sf.object(id)?, &res).map_err(|e| self.out.problems.push(format!("2019 mesh {id}: {e}"))).ok());
        let d = m.map(|m| {
            let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
            Rc::new(Decoded {
                name: m.name.clone(),
                positions: m.positions().into_iter().map(flip).collect(),
                normals: m.normals().into_iter().map(flip).collect(),
                uvs: m.uv(0).into_iter().map(|[u, v]| [u, 1.0 - v]).collect(),
                colors: m.colors(),
                // triangles only, wound for Bevy
                submeshes: (0..m.submeshes.len()).map(|s| (m.submeshes[s].topology == 0).then(|| m.submesh_indices(s)).flatten().map(|mut idx| {
                    for tri in idx.chunks_exact_mut(3) {
                        tri.swap(1, 2);
                    }
                    idx
                })).collect(),
            })
        });
        self.meshes.insert((f, id), d.clone());
        d
    }

    /// One scene object's mesh, every triangle submesh with its material. A statically batched renderer points at
    /// the scene's combined mesh: it draws only its own submesh range, whose vertices are already in world space.
    fn add(&mut self, scene: &Scene, i: usize) {
        let Some(sf) = self.files.get(&scene.file) else { return };
        let Some((_, filter, renderer)) = node_mesh(self.files, scene, &sf, i) else { return };
        let Some(m) = self.mesh(&scene.file, filter.mesh) else { return };
        let (first, count) = (renderer.static_batch.0 as usize, renderer.static_batch.1 as usize);
        let (subs, world) = if count > 0 {
            // INFERRED from Unity's static batching: the batch root is the scene, so the combined vertices are in
            // world space and the object's own transform no longer applies
            (first..(first + count).min(m.submeshes.len()), Mat4::IDENTITY)
        } else {
            (0..m.submeshes.len(), scene.nodes[i].world)
        };
        for s in subs {
            let Some(idx) = &m.submeshes[s] else { continue };
            // this submesh's vertices only (the combined mesh holds the whole scene)
            let mut map: HashMap<u32, u32> = HashMap::new();
            let mut used: Vec<usize> = Vec::new();
            let indices: Vec<u32> = idx.iter().map(|&v| *map.entry(v).or_insert_with(|| {
                used.push(v as usize);
                used.len() as u32 - 1
            })).collect();
            if used.iter().any(|&v| v >= m.positions.len()) {
                self.out.problems.push(format!("2019 mesh {}: index past the vertices", m.name));
                continue;
            }
            // a uniform vertex colour folds into the material's tint (as for the 2026 pieces); varying ones are
            // dropped (INFERRED: the Dorm's meshes have none)
            let colors: Vec<_> = used.iter().filter_map(|&v| m.colors.get(v)).collect();
            let vc = colors.first().filter(|c0| colors.iter().all(|c| c == *c0)).map_or([255; 3], |c| std::array::from_fn(|k| (c[k].clamp(0.0, 1.0) * 255.0).round() as u8));
            let Some(&mp) = renderer.materials.get(s - if count > 0 { first } else { 0 }).or(renderer.materials.last()) else { continue };
            let Some(material) = self.material(&scene.file, mp, vc) else { continue };
            self.out.parts.push(Part {
                name: scene.nodes[i].name.clone(),
                root: self.root,
                mesh: if count > 0 { format!("{} #{s}", m.name) } else { m.name.clone() },
                world,
                positions: used.iter().map(|&v| m.positions[v]).collect(),
                normals: used.iter().map(|&v| m.normals.get(v).copied().unwrap_or([0.0, 1.0, 0.0])).collect(),
                uvs: used.iter().map(|&v| m.uvs.get(v).copied().unwrap_or([0.0, 0.0])).collect(),
                colors: vec![],
                indices,
                material,
            });
        }
    }
}

/// A mesh's vertex streams, converted to Bevy's axes once (the combined mesh is shared by many objects).
struct Decoded {
    name: String,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    submeshes: Vec<Option<Vec<u32>>>,
}

/// The room as RoomData: the 2026 prefabs where the old scene puts them, the old scene's own pieces, its solid box
/// colliders and its realtime lights (which replace the prefabs' own). `None` when there is no reference install or
/// it has no such scene, so the caller falls back to its own layout.
pub fn load(lib: &mut Library, kit: &mut ShapeKit, dir: &Path, room: &str) -> Option<Result<RoomData, String>> {
    let data = ref_install()?;
    let mut files = Files::new(data);
    scene_file(&mut files, room)?;
    Some(load_with(&mut files, lib, kit, dir, room))
}

fn load_with(files: &mut Files, lib: &mut Library, kit: &mut ShapeKit, dir: &Path, room: &str) -> Result<RoomData, String> {
    let (scene, placed) = plan(files, lib, dir, room)?;
    let info = ROOMS.iter().find(|r| r.scene.eq_ignore_ascii_case(&scene.name));
    let group = info.and_then(|i| i.group).and_then(|g| crate::world::GROUPS.iter().find(|x| x.name == g));
    let mut r = match group {
        Some(g) => {
            let layout: Vec<(String, Option<Mat4>)> = placed.iter().filter_map(|p| match &p.piece {
                Piece::Prefab(n) => Some((n.clone(), Some(p.world))),
                Piece::Hidden(n) => Some((n.clone(), None)),
                Piece::Mesh(_) => None,
            }).collect();
            room::load_prefabs(lib, kit, dir, g.bundle, |n| n.starts_with(g.prefix), &layout)?
        }
        None => RoomData::default(),
    };
    r.roots.push(format!("2019:{}", scene.name));
    r.places.push(Mat4::IDENTITY);
    r.props.push(false);
    let root = r.roots.len() - 1;
    let sf = files.get(&scene.file).ok_or("scene file")?;
    let mut b = OldBuilder { files, out: &mut r, root, textures: HashMap::new(), materials: HashMap::new(), meshes: HashMap::new() };
    for p in &placed {
        if let Piece::Mesh(i) = p.piece {
            b.add(&scene, i);
        }
    }
    // solid colliders (box, sphere, capsule, mesh; triggers left out) of active objects
    let (cols, croots) = scene_colliders(files, &sf, &scene, root);
    r.colliders = cols;
    r.collider_roots = croots;
    let lights = scene_lights(&scene, &sf, root);
    if !lights.is_empty() {
        r.lights = lights;
    }
    Ok(r)
}

// ---- the decrypted 2026 scenes ----

/// The default place of the decrypted 2026 base files (the 114 `levelN` scenes, `sharedassetsN`, `resources.assets`,
/// `globalgamemanagers`), produced by the preservation capture. Their `.resS` streams are read from the game install.
pub const DEFAULT_SCENES_DIR: &str = r"C:\Users\alexi\rr-capture\decomp-full\assets\decrypted";

static SCENES_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Sets the decrypted-scene corpus from `--scenes-dir` (a folder, or `none` to turn it off). Call once, before the
/// first room loads; later calls are ignored.
pub fn set_scenes_dir(arg: Option<&str>) {
    let dir = match arg {
        Some(a) if a.eq_ignore_ascii_case("none") => None,
        Some(a) => Some(PathBuf::from(a)),
        None => default_scenes_dir(),
    };
    let _ = SCENES_DIR.set(dir);
}

fn default_scenes_dir() -> Option<PathBuf> {
    Some(PathBuf::from(DEFAULT_SCENES_DIR)).filter(|d| d.join("globalgamemanagers").exists())
}

/// The decrypted 2026 scene corpus, if there is one.
pub fn scenes_dir() -> Option<&'static Path> {
    SCENES_DIR.get_or_init(default_scenes_dir).as_deref()
}

/// A tag for the world cache key: rooms built from the decrypted corpus differ from those without it.
pub fn scenes_cache_tag() -> String {
    format!("scenes1:{:?}", scenes_dir())
}

/// One built-in scene of the 2026 build: our short name (the scene file name, e.g. `reccenter`), its `levelN` file,
/// and the full asset path.
pub struct SceneEntry {
    pub name: String,
    pub file: String,
    pub path: String,
}

/// Every built-in scene in the decrypted corpus (from `globalgamemanagers`' build settings), in `levelN` order.
pub fn scene_list() -> Vec<SceneEntry> {
    let Some(dir) = scenes_dir() else { return Vec::new() };
    let mut files = Files::new(dir);
    let Some(ggm) = files.get("globalgamemanagers") else { return Vec::new() };
    let Ok(scenes) = level::build_scenes(&ggm) else { return Vec::new() };
    scenes
        .iter()
        .enumerate()
        .map(|(i, p)| SceneEntry { name: level::scene_name(p).to_string(), file: format!("level{i}"), path: p.clone() })
        .collect()
}

/// Loads a built-in 2026 scene (by our name, scene name or `levelN`) straight from the decrypted corpus: its own
/// geometry, materials, solid colliders and realtime lights. `game_data` is the game install's `RecRoom_Data`,
/// where the plaintext `.resS` streams live. `None` when there is no corpus or it has no such scene, so the caller
/// falls back to another loader.
/// The solid colliders (box, sphere, capsule, mesh; triggers and disabled ones left out) of a scene's active
/// objects, in client (Bevy) space, with the root each belongs to.
fn scene_colliders(files: &mut Files, sf: &SerializedFile, scene: &Scene, root: usize) -> (Vec<Collider>, Vec<usize>) {
    let mut colliders = Vec::new();
    let mut roots = Vec::new();
    for n in scene.nodes.iter().filter(|n| n.active) {
        let world = n.world;
        let to_bevy = |p: [f32; 3]| world.transform_point3(Vec3::new(p[0], p[1], -p[2]));
        let smax = world.to_scale_rotation_translation().0.abs().max_element();
        for &(class, id) in &n.components {
            let Some(o) = sf.object(id) else { continue };
            let col = if class == classes::BOX_COLLIDER {
                level::box_collider(sf, o).ok().filter(|c| c.enabled && !c.trigger).map(|c| Collider::Box(Obb::from_box_collider(world, c.center, c.size)))
            } else if class == classes::SPHERE_COLLIDER {
                level::sphere_collider(sf, o).ok().filter(|c| c.enabled && !c.trigger).map(|c| Collider::Sphere { center: to_bevy(c.center), radius: c.radius * smax })
            } else if class == classes::CAPSULE_COLLIDER {
                level::capsule_collider(sf, o).ok().filter(|c| c.enabled && !c.trigger).map(|c| {
                    let dir = match c.direction { 0 => Vec3::X, 1 => Vec3::Y, _ => Vec3::Z };
                    let half = (c.height * 0.5 - c.radius).max(0.0);
                    let cen = Vec3::from(c.center);
                    let (a, b) = (cen + dir * half, cen - dir * half);
                    Collider::Capsule { a: to_bevy(a.into()), b: to_bevy(b.into()), r: c.radius * smax }
                })
            } else if class == classes::MESH_COLLIDER {
                mesh_collider_shape(files, sf, &scene.file, world, o)
            } else {
                None
            };
            if let Some(c) = col {
                colliders.push(c);
                roots.push(root);
            }
        }
    }
    (colliders, roots)
}

/// Build a [`Collider::Mesh`] from a MeshCollider's referenced mesh, in client (Bevy) world space.
fn mesh_collider_shape(files: &mut Files, sf: &SerializedFile, scene_file: &str, world: Mat4, o: &unity_assets::ObjectInfo) -> Option<Collider> {
    let mc = level::mesh_collider(sf, o).ok().filter(|c| c.enabled && !c.trigger)?;
    let (f, pid) = files.resolve(scene_file, mc.mesh)?;
    let mesh_sf = files.get(&f)?;
    let res = files.resources();
    let m = level::mesh(&mesh_sf, mesh_sf.object(pid)?, &res).ok()?;
    let pos = m.positions();
    let to_world = |i: u32| pos.get(i as usize).map(|p| world.transform_point3(Vec3::new(p[0], p[1], -p[2])));
    let mut tris = Vec::new();
    for s in 0..m.submeshes.len() {
        if m.submeshes[s].topology != 0 {
            continue; // triangle lists only
        }
        let Some(idx) = m.submesh_indices(s) else { continue };
        for t in idx.chunks_exact(3) {
            if let (Some(a), Some(b), Some(c)) = (to_world(t[0]), to_world(t[1]), to_world(t[2])) {
                tris.push([a, b, c]);
            }
        }
    }
    (!tris.is_empty()).then(|| Collider::Mesh(MeshShape::new(tris)))
}

/// World transforms of a scene's spawn points (`SceneSpawnPoint` / `GamePlayerSpawnPoint` MonoBehaviours), for
/// placing the player. The script class behind each MonoBehaviour is resolved through its `m_Script` and cached.
fn scene_spawns(files: &mut Files, scene_file: &str, sf: &SerializedFile, scene: &Scene) -> Vec<Mat4> {
    let mut cache: HashMap<(i32, i64), bool> = HashMap::new();
    let mut spawns = Vec::new();
    for n in scene.nodes.iter().filter(|n| n.active) {
        let mut is_spawn = false;
        for &(class, id) in &n.components {
            if class != classes::MONO_BEHAVIOUR {
                continue;
            }
            let Some(o) = sf.object(id) else { continue };
            let Ok(p) = level::mono_script_ref(sf, o) else { continue };
            let key = (p.file_id, p.path_id);
            let hit = if let Some(&h) = cache.get(&key) {
                h
            } else {
                let name = if p.file_id == 0 {
                    sf.object(p.path_id).and_then(|so| level::mono_script_name(sf, so).ok())
                } else if let Some((f, pid)) = files.resolve(scene_file, p) {
                    files.get(&f).and_then(|x| {
                        let so = x.object(pid)?;
                        level::mono_script_name(&x, so).ok()
                    })
                } else {
                    None
                };
                let h = matches!(name.as_ref().map(|(c, _)| c.as_str()), Some("SceneSpawnPoint" | "GamePlayerSpawnPoint"));
                cache.insert(key, h);
                h
            };
            if hit {
                is_spawn = true;
                break;
            }
        }
        if is_spawn {
            spawns.push(n.world);
        }
    }
    spawns
}

static PS_TREE: OnceLock<Option<TypeTree>> = OnceLock::new();

/// The ParticleSystem(198) type tree, read once from any install bundle that carries one. The decrypted scene files
/// have no type trees, so this is injected to decode their inline ParticleSystems in place. Scans install bundles
/// until one has it (bounded, cached); returns None if no install or none found.
fn ps_type_tree() -> Option<&'static TypeTree> {
    PS_TREE
        .get_or_init(|| {
            let game = room::GAME_DIRS.iter().map(PathBuf::from).find(|p| p.exists())?;
            let bdir = room::bundle_dir(&game);
            for (i, entry) in std::fs::read_dir(&bdir).ok()?.flatten().enumerate() {
                if i >= 800 {
                    break; // bound the one-time scan
                }
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) != Some("bundle") {
                    continue;
                }
                let Ok(b) = unity_assets::Bundle::open(&p) else { continue };
                let Ok(sfs) = b.serialized_files() else { continue };
                for (_, sf) in sfs {
                    if let Some(t) = sf.types.into_iter().find(|t| t.class_id == classes::PARTICLE_SYSTEM && t.tree.is_some()).and_then(|t| t.tree) {
                        return Some(t);
                    }
                }
            }
            None
        })
        .as_ref()
}

/// A scene ParticleSystem reduced to the render essentials (representative constants; no curves).
fn to_emitter(world: Mat4, ps: &unity_assets::ParticleSystem) -> ParticleEmitter {
    let col = |c: &unity_assets::ParticleColor| [c.r, c.g, c.b, c.a];
    ParticleEmitter {
        world,
        max_particles: ps.max_particles.clamp(0, 100_000) as u32,
        looping: ps.looping,
        rate: ps.rate_over_time.constant().max(0.0),
        bursts: ps.bursts.iter().map(|b| (b.time, b.count.constant())).collect(),
        lifetime: ps.start_lifetime.constant().max(0.01),
        speed: ps.start_speed.constant(),
        size: ps.start_size.constant().max(0.001),
        gravity: ps.gravity_modifier.constant(),
        shape: if ps.shape_enabled { ps.shape_type } else { -1 },
        shape_angle: ps.shape_angle,
        shape_radius: ps.shape_radius,
        start_color: col(&ps.start_color.min_color),
        color_over_life: ps.color_over_lifetime.as_ref().map(|g| g.stops.iter().map(|(t, c)| (*t, col(c))).collect()).unwrap_or_default(),
        size_over_life: ps.size_over_lifetime.as_ref().map(|c| c.max_curve.iter().map(|k| (k.time, k.value)).collect()).unwrap_or_default(),
    }
}

/// The room's auto-playing ambient particle emitters, decoded from the scene's inline ParticleSystems via the
/// injected PS type tree. Player `VFX_Emote_*` effects and non-auto / non-emitting systems are left out.
fn scene_particles(sf: &SerializedFile, scene: &Scene) -> Vec<ParticleEmitter> {
    let Some(tree) = ps_type_tree() else { return Vec::new() };
    let mut out = Vec::new();
    for n in scene.nodes.iter().filter(|n| n.active) {
        if n.name.starts_with("VFX_Emote") {
            continue;
        }
        let Some(&(_, id)) = n.components.iter().find(|c| c.0 == classes::PARTICLE_SYSTEM) else { continue };
        let Some(o) = sf.object(id) else { continue };
        let mut r = Reader::new(sf.object_bytes(o), sf.big_endian);
        let Ok(v) = unity_assets::value::read(tree, &sf.ref_types, &mut r) else { continue };
        let ps = unity_assets::ParticleSystem::from_value(&v);
        if ps.play_on_awake && ps.emission_enabled {
            out.push(to_emitter(n.world, &ps));
        }
    }
    out
}

/// A scene GameObject matched by its hierarchy NAME (e.g. objects whose name contains "Goal", "Basket", "Tee"): its
/// world position (client/Bevy space) and, if it has a box/sphere collider, that collider's local half-extent. Per-
/// game plugins use this to place goal/basket/hazard zones and spawns from the REAL scene objects instead of
/// approximating from the arena bounds — the shared scene-object reader.
///
/// NOTE on the matcher: matching is by object NAME, not MonoBehaviour script class. The 2026 scenes reference their
/// gameplay MonoScripts EXTERNALLY (in the game assembly), so the script class can't be resolved from the scene file
/// alone (`mono_script_name` finds none) — resolving those external PPtrs is a maps-lane follow-up. Name matching
/// works today and is enough to locate the gadgets. The objects' serialized FIELDS (a goal's hole/par/team) likewise
/// need the MonoBehaviour byte layout and are not read here — mark those `// 1:1:` until the maps lane exposes them.
pub struct SceneObject {
    pub name: String,
    /// true if it carries a MonoBehaviour (a gameplay script object, not just geometry)
    pub scripted: bool,
    /// world position, client (Bevy) space
    pub pos: Vec3,
    /// the object's box/sphere collider half-extent (local), if it has one — for a trigger zone's size
    pub half: Option<Vec3>,
}

/// Every active scene object whose hierarchy name contains `needle` (case-insensitive; `*` matches all scripted
/// objects, for discovery). Empty without the corpus.
pub fn scene_objects(game_data: Option<&Path>, room: &str, needle: &str) -> Vec<SceneObject> {
    let Some(dir) = scenes_dir() else { return Vec::new() };
    let mut res_dirs = vec![dir.to_path_buf()];
    if let Some(g) = game_data {
        res_dirs.push(g.to_path_buf());
    }
    let mut files = Files::with_res(dir, res_dirs);
    let Some((file, name)) = scene_file(&mut files, room) else { return Vec::new() };
    let Ok(scene) = read_scene(&mut files, &file, &name) else { return Vec::new() };
    let Some(sf) = files.get(&scene.file) else { return Vec::new() };
    let needle = needle.to_lowercase();
    let mut out = Vec::new();
    for n in scene.nodes.iter().filter(|n| n.active) {
        let scripted = n.components.iter().any(|(c, _)| *c == classes::MONO_BEHAVIOUR);
        let wildcard = needle == "*";
        if wildcard {
            if !scripted {
                continue; // discovery lists scripted objects only
            }
        } else if !n.name.to_lowercase().contains(&needle) {
            continue;
        }
        let pos = n.world.transform_point3(Vec3::ZERO);
        let half = n.components.iter().find_map(|(c, id)| {
            let o = sf.object(*id)?;
            if *c == classes::BOX_COLLIDER {
                level::box_collider(&sf, o).ok().map(|bc| Vec3::from(bc.size) * 0.5)
            } else if *c == classes::SPHERE_COLLIDER {
                level::sphere_collider(&sf, o).ok().map(|sc| Vec3::splat(sc.radius))
            } else {
                None
            }
        });
        out.push(SceneObject { name: n.name.clone(), scripted, pos, half });
    }
    out
}

/// A gameplay "goal"/trigger object read by MonoBehaviour CLASS (e.g. `SoccerGoal`, `PaddleballGoal`): its `team`
/// field and the world-space AABB of the trigger `BoxCollider` it references. Unlike [`scene_objects`] (name match),
/// this resolves the script class across the scene's sibling files (`mono_script_ref` → `mono_script_name`) and reads
/// the serialized fields with [`level::MonoFields`], so it returns the REAL per-goal config, not an approximation.
pub struct SceneGoal {
    /// the team that DEFENDS this goal (`GameTeam`); scoring into it credits the OTHER team
    pub team: i32,
    /// world-space (client/bevy) centre of the trigger box
    pub center: Vec3,
    /// world-space half-extent of the trigger box (axis-aligned; goal boxes are not rotated on the pitch)
    pub half: Vec3,
}

/// The scene's goal objects of MonoBehaviour class `class`, read the way the decompile lane specifies the field order.
/// `read`: a cursor positioned just past the MB header; return `(defending_team, goal_collider_pptr_path_id)` — e.g.
/// SoccerGoal is `|mf| Ok((/* goalCollider */ { let p = mf.pptr()?; p.path_id }, /* team */ mf.i32()?))` but with the
/// two in field order. Here we hard-wire the SoccerGoal/PaddleballGoal shapes via `goal_collider_first`.
pub fn scene_goals(game_data: Option<&Path>, room: &str, class: &str, goal_collider_first: bool) -> Vec<SceneGoal> {
    let Some(dir) = scenes_dir() else { return Vec::new() };
    let mut res_dirs = vec![dir.to_path_buf()];
    if let Some(g) = game_data {
        res_dirs.push(g.to_path_buf());
    }
    let mut files = Files::with_res(dir, res_dirs);
    let Some((file, name)) = scene_file(&mut files, room) else { return Vec::new() };
    let Ok(scene) = read_scene(&mut files, &file, &name) else { return Vec::new() };
    let scene_file_name = scene.file.clone();
    // externals of the scene file, for resolving an m_Script PPtr that points into a sibling file
    let externals: Vec<String> = match files.get(&scene_file_name) {
        Some(sf) => sf.externals.iter().map(|e| e.path.rsplit('/').next().unwrap_or(&e.path).to_string()).collect(),
        None => return Vec::new(),
    };
    // component object id -> owning node index (to find the goalCollider's world transform)
    let mut owner: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (i, n) in scene.nodes.iter().enumerate() {
        for &(_, id) in &n.components {
            owner.insert(id, i);
        }
    }
    let mut out = Vec::new();
    for n in scene.nodes.iter().filter(|n| n.active) {
        for &(_, id) in n.components.iter().filter(|(c, _)| *c == classes::MONO_BEHAVIOUR) {
            // resolve the script class name (same file or an external), each `files.get` in its own scope
            let p = match files.get(&scene_file_name).and_then(|sf| sf.object(id).and_then(|o| level::mono_script_ref(&sf, o).ok())) {
                Some(p) => p,
                None => continue,
            };
            let class_name = if p.file_id == 0 {
                files.get(&scene_file_name).and_then(|sf| sf.object(p.path_id).and_then(|so| level::mono_script_name(&sf, so).ok())).map(|(c, _)| c)
            } else if let Some(ext) = externals.get(p.file_id as usize - 1).cloned() {
                files.get(&ext).and_then(|f| f.object(p.path_id).and_then(|so| level::mono_script_name(&f, so).ok())).map(|(c, _)| c)
            } else {
                None
            };
            if class_name.as_deref() != Some(class) {
                continue;
            }
            // read (goalCollider PPtr, team) in the class's declared field order
            let read = files.get(&scene_file_name).and_then(|sf| {
                let o = sf.object(id)?;
                let mut mf = level::MonoFields::open(&sf, o).ok()?;
                if goal_collider_first {
                    let gc = mf.pptr().ok()?; // goalCollider: PPtr(BoxCollider)
                    let team = mf.i32().ok()?; // team: GameTeam
                    Some((team, gc.path_id))
                } else {
                    let team = mf.i32().ok()?; // team first (PaddleballGoal)
                    Some((team, 0))
                }
            });
            let Some((team, collider_pid)) = read else { continue };
            // the referenced goal collider's world AABB (its owning node's transform × the collider's local box)
            let coll_world = collider_pid.ne(&0).then(|| owner.get(&collider_pid).map(|&i| scene.nodes[i].world)).flatten().unwrap_or(n.world);
            let (center, half) = files
                .get(&scene_file_name)
                .and_then(|sf| sf.object(collider_pid).and_then(|co| level::box_collider(&sf, co).ok()))
                .map(|bc| {
                    let center = coll_world.transform_point3(Vec3::new(bc.center[0], bc.center[1], -bc.center[2]));
                    let scale = coll_world.to_scale_rotation_translation().0.abs();
                    let half = Vec3::new(bc.size[0], bc.size[1], bc.size[2]) * 0.5 * scale;
                    (center, half)
                })
                .unwrap_or((n.world.transform_point3(Vec3::ZERO), Vec3::splat(2.0)));
            out.push(SceneGoal { team, center, half });
        }
    }
    out
}

pub fn load_scene(game_data: Option<&Path>, room: &str) -> Option<Result<RoomData, String>> {
    let dir = scenes_dir()?;
    let mut res_dirs = vec![dir.to_path_buf()];
    if let Some(g) = game_data {
        res_dirs.push(g.to_path_buf());
    }
    let mut files = Files::with_res(dir, res_dirs);
    scene_file(&mut files, room)?;
    Some(load_scene_with(&mut files, room))
}

fn load_scene_with(files: &mut Files, room: &str) -> Result<RoomData, String> {
    let (file, name) = scene_file(files, room).ok_or_else(|| format!("no scene {room:?} in the decrypted corpus"))?;
    let scene = read_scene(files, &file, &name)?;
    let drawn = drawn_meshes(files, &scene);
    let mut r = RoomData::default();
    r.roots.push(format!("2026:{name}"));
    r.places.push(Mat4::IDENTITY);
    r.props.push(false);
    let root = r.roots.len() - 1;
    // solid colliders (box, sphere, capsule, mesh; triggers left out) and realtime lights of active objects
    // (read before the mesh builder borrows `files`)
    let sf = files.get(&scene.file).ok_or("scene file")?;
    let (cols, croots) = scene_colliders(files, &sf, &scene, root);
    r.colliders = cols;
    r.collider_roots = croots;
    let lights = scene_lights(&scene, &sf, root);
    if !lights.is_empty() {
        r.lights = lights;
    }
    r.spawns = scene_spawns(files, &scene.file, &sf, &scene);
    r.particles = scene_particles(&sf, &scene);
    drop(sf);
    let mut b = OldBuilder { files, out: &mut r, root, textures: HashMap::new(), materials: HashMap::new(), meshes: HashMap::new() };
    for (i, _mesh) in &drawn {
        // the room's systems (UI, tutorials, boards) are not part of its look
        if scene.nodes[*i].path.starts_with("Systems/") {
            continue;
        }
        b.add(&scene, *i);
    }
    Ok(r)
}

// ---- lighting ----

/// The scene's realtime and mixed lights (baked-only ones light nothing without lightmaps).
fn scene_lights(scene: &Scene, sf: &SerializedFile, root: usize) -> Vec<RoomLight> {
    let mut out = Vec::new();
    let desktop_parents: HashSet<Option<usize>> = scene.nodes.iter().filter(|n| n.name.contains("Desktop")).map(|n| n.parent).collect();
    for (_, n) in scene.nodes.iter().enumerate().filter(|(i, n)| n.active && !mobile_variant(scene, &desktop_parents, *i)) {
        for &(_, id) in n.components.iter().filter(|c| c.0 == classes::LIGHT) {
            let Ok(l) = level::light(sf, sf.object(id).unwrap()) else { continue };
            if !l.enabled || l.lightmapping == 2 || l.intensity <= 0.0 {
                continue;
            }
            out.push(RoomLight {
                root,
                world: n.world,
                kind: l.kind as i64,
                color: shapes::linear([l.color[0], l.color[1], l.color[2]]),
                intensity: l.intensity,
                range: l.range,
                spot_angle: l.spot_angle,
                inner_spot_angle: l.inner_spot_angle,
                shadows: l.shadows != 0,
                hemisphere: None,
            });
        }
    }
    out
}

/// A cubemap's six faces (+X -X +Y -Y +Z -Z), mip 0, RGBA8 (HDR formats clamped to 0..1).
#[derive(Clone)]
pub struct Cubemap {
    pub name: String,
    pub size: u32,
    pub faces: Vec<Pixels>,
}

/// A reflection probe of the scene, with its baked or custom cubemap.
#[derive(Clone)]
pub struct Probe {
    pub world: Mat4,
    /// box size and offset (Unity's local space, z not flipped)
    pub size: [f32; 3],
    pub offset: [f32; 3],
    pub intensity: f32,
    pub box_projection: bool,
    pub importance: i16,
    pub cubemap: Option<Cubemap>,
}

#[derive(Clone, Debug)]
pub struct Fog {
    /// linear colour
    pub color: [f32; 3],
    /// 1 linear, 2 exponential, 3 exponential squared
    pub mode: i32,
    pub density: f32,
    pub start: f32,
    pub end: f32,
}

/// Everything about a room's light the old scene holds. Colours are linear unless named otherwise.
pub struct Lighting {
    pub scene: String,
    /// the raw RenderSettings (gamma-space colours, as stored)
    pub settings: level::RenderSettings,
    /// 0 skybox, 1 trilight (sky/equator/ground), 3 flat (sky), 4 custom
    pub ambient_mode: i32,
    pub ambient_sky: [f32; 3],
    pub ambient_equator: [f32; 3],
    pub ambient_ground: [f32; 3],
    pub ambient_intensity: f32,
    /// the baked ambient SH L2: `[channel][coefficient]`, Unity's coefficient order
    pub ambient_sh: [[f32; 9]; 3],
    pub fog: Option<Fog>,
    /// the skybox material (its colours and floats; `Skybox/Procedural`-style for the Dorm) and its cubemap, if any
    pub skybox: Option<Material>,
    pub skybox_shader: String,
    pub skybox_cubemap: Option<Cubemap>,
    /// the default reflection (RenderSettings custom reflection) and its intensity
    pub reflection: Option<Cubemap>,
    pub reflection_intensity: f32,
    /// realtime and mixed lights, as `RoomData::lights`; `sun` indexes the sun among them
    pub lights: Vec<RoomLight>,
    pub sun: Option<usize>,
    pub probes: Vec<Probe>,
    /// light probe positions (client space) and their SH (27 floats, as `ambient_sh` flattened)
    pub light_probes: Vec<(Vec3, [f32; 27])>,
    /// baked lightmaps (colour), indexed by a renderer's lightmap index; empty when the scene has none
    pub lightmaps: Vec<Pixels>,
    pub lightmap_settings: Option<level::LightmapSettings>,
}

fn cubemap(files: &mut Files, from: &str, p: PPtr) -> Option<Cubemap> {
    let (f, id) = files.resolve(from, p)?;
    let sf = files.get(&f)?;
    let t = level::texture(&sf, sf.object(id)?, &files.resources()).ok()?;
    let mut face_bytes = 0;
    for m in 0..t.mip_count.max(1) as usize {
        face_bytes += unity_assets::texture::mip0_size(t.format, (t.width >> m).max(1), (t.height >> m).max(1))?;
    }
    let faces = (0..t.image_count.max(1) as usize)
        .map(|i| t.data.get(i * face_bytes..).and_then(|d| unity_assets::texture::decode_rgba8(t.format, t.width, t.height, d).ok()))
        .map(|d| d.map(|d| (t.width as u32, t.height as u32, d)))
        .collect::<Option<Vec<_>>>()?;
    Some(Cubemap { name: t.name, size: t.width as u32, faces })
}

/// The lighting of `room` (a room name, scene name or `levelN`) from the reference install.
pub fn lighting(room: &str) -> Result<Lighting, String> {
    let data = ref_install().ok_or("no reference install")?;
    let files = &mut Files::new(data);
    let (file, name) = scene_file(files, room).ok_or_else(|| format!("no scene {room:?} in the reference install"))?;
    let scene = read_scene(files, &file, &name)?;
    lighting_from(files, &file, &name, &scene)
}

/// The lighting of a 2026 scene from the decrypted corpus: its real RenderSettings, baked reflection probes,
/// skybox cubemap, ambient and realtime lights. `game_data` (the install's `RecRoom_Data`) supplies the out-of-line
/// cubemap/texture streams (`.resource`/`.resS`) that are plaintext in the install.
pub fn scene_lighting(game_data: Option<&Path>, room: &str) -> Result<Lighting, String> {
    let dir = scenes_dir().ok_or("no scenes dir")?;
    let mut res_dirs = vec![dir.to_path_buf()];
    if let Some(g) = game_data {
        res_dirs.push(g.to_path_buf());
    }
    let files = &mut Files::with_res(dir, res_dirs);
    let (file, name) = scene_file(files, room).ok_or_else(|| format!("no scene {room:?} in the decrypted corpus"))?;
    let scene = read_scene(files, &file, &name)?;
    lighting_from(files, &file, &name, &scene)
}

fn lighting_from(files: &mut Files, file: &str, name: &str, scene: &Scene) -> Result<Lighting, String> {
    let sf = files.get(file).ok_or("scene file")?;
    let rs = sf.objects.iter().find(|o| o.class_id == classes::RENDER_SETTINGS).ok_or("no RenderSettings")?;
    let settings = level::render_settings(&sf, rs).map_err(|e| e.to_string())?;
    let lightmap_settings = sf.objects.iter().find(|o| o.class_id == classes::LIGHTMAP_SETTINGS).and_then(|o| level::lightmap_settings(&sf, o).ok());
    let lin = |c: [f32; 4]| shapes::linear([c[0], c[1], c[2]]);
    let fog = settings.fog.then(|| Fog { color: lin(settings.fog_color), mode: settings.fog_mode, density: settings.fog_density, start: settings.fog_start, end: settings.fog_end });
    let skybox = files.resolve(&file, settings.skybox_material).and_then(|(f, id)| { let x = files.get(&f)?; level::material(&x, x.object(id)?).ok().map(|m| (f, m)) });
    let skybox_shader = skybox.as_ref().and_then(|(f, m)| files.resolve(f, m.shader)).and_then(|(sfn, sid)| files.get(&sfn).map(|x| shader_name(&x, sid))).unwrap_or_default();
    let skybox_cubemap = skybox.as_ref().and_then(|(f, m)| {
        let t = m.textures.iter().find(|t| t.name == "_Tex" || t.name == "_Cube")?;
        cubemap(files, f, t.texture)
    });
    let reflection = cubemap(files, file, settings.custom_reflection);
    let lights = scene_lights(scene, &sf, 0);
    // the sun: the RenderSettings' sun light, by its GameObject's world matrix
    let sun_go = sf.object(settings.sun.path_id).and_then(|o| level::light(&sf, o).ok()).map(|l| l.game_object.path_id);
    let sun_world = sun_go.and_then(|g| scene.nodes.iter().find(|n| n.game_object == g)).map(|n| n.world);
    // the RenderSettings' sun, or (when that is the mobile variant, left out) the strongest shadowed spot or
    // directional light (INFERRED)
    let sun = sun_world.and_then(|w| lights.iter().position(|l| l.world == w)).or_else(|| {
        lights.iter().enumerate().filter(|(_, l)| l.shadows && l.kind <= 1).max_by(|a, b| a.1.intensity.total_cmp(&b.1.intensity)).map(|x| x.0)
    });
    let mut probes = Vec::new();
    for n in scene.nodes.iter().filter(|n| n.active) {
        for &(_, id) in n.components.iter().filter(|c| c.0 == classes::REFLECTION_PROBE) {
            let Ok(p) = level::reflection_probe(&sf, sf.object(id).unwrap()) else { continue };
            if !p.enabled {
                continue;
            }
            let tex = if p.custom_texture.path_id != 0 { p.custom_texture } else { p.baked_texture };
            probes.push(Probe {
                world: n.world, size: p.box_size, offset: p.box_offset, intensity: p.intensity, box_projection: p.box_projection,
                importance: p.importance, cubemap: cubemap(files, file, tex),
            });
        }
    }
    let mut light_probes = Vec::new();
    let mut lightmaps = Vec::new();
    if let Some(lm) = &lightmap_settings {
        if let Some((f, id)) = files.resolve(file, lm.light_probes) {
            if let Some(lp) = files.get(&f).and_then(|x| level::light_probes(&x, x.object(id)?).ok()) {
                light_probes = lp.positions.iter().zip(&lp.coefficients).map(|(p, c)| (Vec3::new(p[0], p[1], -p[2]), *c)).collect();
            }
        }
        let res = files.resources();
        for d in &lm.lightmaps {
            let px = files.resolve(file, d.color).and_then(|(f, id)| {
                let x = files.get(&f)?;
                let t = level::texture(&x, x.object(id)?, &res).ok()?;
                Some((t.width as u32, t.height as u32, t.decode_rgba8().ok()?))
            });
            lightmaps.push(px.unwrap_or((1, 1, vec![255; 4])));
        }
    }
    let sh = settings.ambient_probe;
    Ok(Lighting {
        scene: name.to_owned(),
        ambient_mode: settings.ambient_mode,
        ambient_sky: lin(settings.ambient_sky),
        ambient_equator: lin(settings.ambient_equator),
        ambient_ground: lin(settings.ambient_ground),
        ambient_intensity: settings.ambient_intensity,
        ambient_sh: std::array::from_fn(|c| std::array::from_fn(|k| sh[c * 9 + k])),
        fog,
        skybox_shader,
        skybox: skybox.map(|s| s.1),
        skybox_cubemap,
        reflection,
        reflection_intensity: settings.reflection_intensity,
        lights,
        sun,
        probes,
        light_probes,
        lightmaps,
        lightmap_settings,
        settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_and_hull_names() {
        assert!(lower_lod("Desk_Lamp_LOD1") && lower_lod("Aspen_Tree_Mobile_LOD_2") && !lower_lod("Desk_Lamp_LOD0") && !lower_lod("LODGroup"));
        assert!(is_hull_slice("Dorm_04") && is_hull_slice("Dorm_Trim_06") && !is_hull_slice("Dorm_Rug"));
        assert!(hidden_prefab("(Dorm_Wall_1_Up)") && hidden_prefab("(Dorm_Light_Shafts)") && !hidden_prefab("(Dorm_Wall_1)"));
    }

    #[test]
    fn shader_name_scan() {
        // a fake shader body: property strings, then the name, the custom editor (empty), the fallback
        let mut b = Vec::new();
        for s in ["_BaseMap", "Base/Map", "AG/Standard-Opaque", "", "Hidden/Universal Render Pipeline/FallbackError"] {
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
            while b.len() % 4 != 0 {
                b.push(0);
            }
        }
        assert_eq!(scan_shader_name(&b), "AG/Standard-Opaque");
    }
}
