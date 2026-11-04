//! Rooms by Addressables address: the catalog lookup (address -> bundle -> object), the inventory of what the
//! bundles can build, and `load` for `--world`.
//!
//! The 2026 level files are scrambled, so a "room" here is what the Addressables hold: one environment prefab, or a
//! group of prefabs sharing a name prefix (the Dorm's `(Dorm_*` pieces) placed by a layout file of ours.
use std::{collections::{BTreeMap, HashMap}, path::Path};

use bevy::math::{Mat4, Vec3};
use unity_assets::{catalog::{bundle_file, Catalog}, classes, AssetBundleInfo, GameObject, PPtr, Transform};

use crate::{room::{self, value, FileRef, Library, RoomData}, shapes::{self, ShapeKit}};

/// Where an address leads: the bundle file and the bundle's own name for the asset (its `m_Container` key).
#[derive(Clone, Debug)]
pub struct Location {
    pub bundle: String,
    pub internal_id: String,
    pub resource_type: String,
}

/// The Addressables catalog (`aa/catalog.bin`, next to the bundle folder), by address.
#[derive(Default)]
pub struct Addresses {
    pub by_key: HashMap<String, Location>,
}

impl Addresses {
    pub fn open(bundle_dir: &Path) -> Result<Self, String> {
        let path = bundle_dir.parent().map(|p| p.join("catalog.bin")).ok_or("no catalog folder")?;
        let cat = Catalog::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut by_key = HashMap::new();
        for e in cat.entries().map_err(|e| e.to_string())? {
            for l in e.locations {
                let Some(bundle) = bundle_file(&l).map(String::from) else { continue };
                if l.resource_type.ends_with("IAssetBundleResource") {
                    continue;
                }
                by_key.entry(e.key.clone()).or_insert(Location { bundle, internal_id: l.internal_id, resource_type: l.resource_type });
            }
        }
        Ok(Addresses { by_key })
    }

    /// GameObject addresses, sorted.
    pub fn game_objects(&self) -> Vec<(&str, &Location)> {
        let mut v: Vec<_> = self.by_key.iter().filter(|(_, l)| l.resource_type == "UnityEngine.GameObject").map(|(k, l)| (k.as_str(), l)).collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        v
    }
}

/// The asset an address names: its file and path id, through the bundle's container table.
pub(crate) fn locate(lib: &mut Library, dir: &Path, loc: &Location) -> Result<(FileRef, i64), String> {
    let open = lib.open_bundle(&dir.join(&loc.bundle))?;
    for index in 0..open.files.len() {
        let f = FileRef { open: open.clone(), index };
        let Some(ab) = f.sf().objects.iter().find(|o| o.class_id == classes::ASSET_BUNDLE) else { continue };
        let info = f.sf().read_value(ab).and_then(|v| AssetBundleInfo::from_value(&v)).map_err(|e| e.to_string())?;
        let Some((_, p)) = info.container.iter().find(|(k, _)| *k == loc.internal_id) else { continue };
        return lib.resolve(&f, *p);
    }
    Err(format!("{} not in the container of {}", loc.internal_id, loc.bundle))
}

/// The Transform of a GameObject (given by file and path id).
pub(crate) fn transform_of(lib: &mut Library, f: &FileRef, gid: i64) -> Option<(FileRef, i64)> {
    let go = value(f, gid).ok().and_then(|v| GameObject::from_value(&v).ok())?;
    go.components.iter().find_map(|&c| {
        let (cf, cid) = lib.resolve(f, c).ok()?;
        matches!(cf.sf().object(cid).map(|o| o.class_id), Some(classes::TRANSFORM)).then_some((cf, cid))
    })
}

/// What one addressable prefab holds, for the inventory.
#[derive(Default, Debug)]
pub struct Summary {
    /// world-space bounds of its meshes (client space), min and max
    pub lo: Vec3,
    pub hi: Vec3,
    pub renderers: usize,
    pub objects: usize,
    /// components by Unity class name or MonoBehaviour script name
    pub components: BTreeMap<String, usize>,
    /// a UI prefab (RectTransform root)
    pub ui: bool,
}

impl Summary {
    pub fn size(&self) -> Vec3 {
        if self.lo.x > self.hi.x { Vec3::ZERO } else { self.hi - self.lo }
    }
}

fn script_name(lib: &mut Library, f: &FileRef, id: i64) -> Option<String> {
    let v = value(f, id).ok()?;
    let p = v.get("m_Script")?.as_pptr()?;
    let (sf, sid) = lib.resolve(f, p).ok()?;
    value(&sf, sid).ok()?.get("m_ClassName")?.as_str().map(String::from)
}

/// Walks a prefab without building meshes: counts components and adds up mesh bounds.
pub fn summarize(lib: &mut Library, dir: &Path, loc: &Location) -> Result<Summary, String> {
    let (f, gid) = locate(lib, dir, loc)?;
    let mut s = Summary { lo: Vec3::splat(f32::MAX), hi: Vec3::splat(f32::MIN), ..Default::default() };
    let go = value(&f, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string()))?;
    let root = go.components.iter().find_map(|&c| {
        let (cf, cid) = lib.resolve(&f, c).ok()?;
        let class = cf.sf().object(cid)?.class_id;
        matches!(class, classes::TRANSFORM | classes::RECT_TRANSFORM).then_some((cf, cid, class))
    });
    let Some((tf, tid, class)) = root else { return Err("no transform".into()) };
    s.ui = class == classes::RECT_TRANSFORM;
    if s.ui {
        return Ok(s);
    }
    let mut stack = vec![(tf, tid, Mat4::IDENTITY)];
    while let Some((f, tid, parent)) = stack.pop() {
        let Ok(t) = value(&f, tid).and_then(|v| Transform::from_value(&v).map_err(|e| e.to_string())) else { continue };
        let world = parent * room::unity_to_bevy(&t);
        let Ok((gf, gid)) = lib.resolve(&f, t.game_object) else { continue };
        let Ok(go) = value(&gf, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string())) else { continue };
        s.objects += 1;
        for &c in &go.components {
            let Ok((cf, cid)) = lib.resolve(&gf, c) else { continue };
            let Some(class) = cf.sf().object(cid).map(|o| o.class_id) else { continue };
            let name = match class {
                classes::TRANSFORM | classes::GAME_OBJECT => continue,
                classes::MONO_BEHAVIOUR => script_name(lib, &cf, cid).unwrap_or_else(|| "MonoBehaviour".into()),
                _ => classes::class_name(class).map_or_else(|| format!("Class{class}"), String::from),
            };
            if class == classes::MESH_FILTER {
                let aabb = value(&cf, cid).ok().and_then(|v| v.get("m_Mesh")?.as_pptr())
                    .and_then(|p: PPtr| lib.resolve(&cf, p).ok())
                    .and_then(|(mf, mid)| value(&mf, mid).ok())
                    .and_then(|v| Some((v.path("m_LocalAABB/m_Center")?.floats(), v.path("m_LocalAABB/m_Extent")?.floats())));
                if let Some((c, e)) = aabb.filter(|(c, e)| c.len() == 3 && e.len() == 3) {
                    for corner in 0..8 {
                        let sign = |b: usize| if corner & (1 << b) != 0 { 1.0 } else { -1.0 };
                        let p = Vec3::new(c[0] + sign(0) * e[0], c[1] + sign(1) * e[1], -(c[2] + sign(2) * e[2]));
                        let w = world.transform_point3(p);
                        s.lo = s.lo.min(w);
                        s.hi = s.hi.max(w);
                    }
                    s.renderers += 1;
                }
            }
            *s.components.entry(name).or_default() += 1;
        }
        for &child in &t.children {
            if let Ok((cf, cid)) = lib.resolve(&f, child) {
                stack.push((cf, cid, world));
            }
        }
    }
    Ok(s)
}

/// Our placement of the Dorm pieces the game does not author in place (the original layout is in the scrambled
/// level files).
const DORM_LAYOUT: &str = include_str!("../rooms/dorm.layout");

/// A room that needs more than one prefab: its prefabs are the root GameObjects of `bundle` whose names start with
/// `prefix` (all of them, or only those named in the layout when `only_layout`), placed by `layout`.
pub struct Group {
    pub name: &'static str,
    pub bundle: &'static str,
    pub prefix: &'static str,
    pub layout: &'static str,
    pub only_layout: bool,
}

const SANDBOX: &str = "43ea1176ca25c0333547813ec3298808.bundle";

pub const GROUPS: &[Group] = &[
    Group { name: "dorm", bundle: SANDBOX, prefix: "(Dorm_", layout: DORM_LAYOUT, only_layout: false },
    Group { name: "stuntrunner", bundle: SANDBOX, prefix: "(StuntRunner_", layout: include_str!("../rooms/stuntrunner.layout"), only_layout: true },
    Group { name: "sandbox", bundle: SANDBOX, prefix: "(Sandbox_", layout: include_str!("../rooms/sandbox.layout"), only_layout: true },
];

/// Loads a room for `--world`: a group name from [`GROUPS`] (default `dorm`), one GameObject address from the
/// catalog, or `prefix*` for every GameObject address starting with `prefix` (each where its own root puts it).
pub fn load(lib: &mut Library, dir: &Path, world: Option<&str>) -> Result<RoomData, String> {
    let world = world.unwrap_or("dorm");
    // the Maker Pen textures come from an older install, when there is one
    let mut kit = ShapeKit::new(shapes::old_build_dirs().iter().find(|d| d.exists()).map(|d| d.as_path()));
    if let Some(g) = GROUPS.iter().find(|g| g.name.eq_ignore_ascii_case(world)) {
        let layout = room::parse_layout(g.layout)?;
        let want = |n: &str| n.starts_with(g.prefix) && (!g.only_layout || layout.iter().any(|(l, _)| l == n));
        return room::load_prefabs(lib, &mut kit, dir, g.bundle, want, &layout);
    }
    let addresses = Addresses::open(dir)?;
    let picked: Vec<(String, Location)> = match world.strip_suffix('*') {
        Some(prefix) => addresses.game_objects().into_iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, l)| (k.to_string(), l.clone())).collect(),
        None => addresses.by_key.get(world).map(|l| vec![(world.to_string(), l.clone())]).unwrap_or_default(),
    };
    if picked.is_empty() {
        return Err(format!("no GameObject address {world:?} in the catalog"));
    }
    let mut roots = Vec::new();
    for (key, loc) in &picked {
        match locate(lib, dir, loc) {
            Ok(r) => roots.push((key.clone(), r, Mat4::IDENTITY)),
            Err(e) => eprintln!("{key}: {e}"),
        }
    }
    let mut r = room::load_roots(lib, &mut kit, roots)?;
    recenter(&mut r);
    Ok(r)
}

/// Moves a room so the middle of its meshes is over the origin (where players start), its height left alone.
fn recenter(r: &mut RoomData) {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &r.parts {
        for &v in &p.positions {
            let w = p.world.transform_point3(v.into());
            lo = lo.min(w);
            hi = hi.max(w);
        }
    }
    if lo.x > hi.x {
        return;
    }
    let shift = Vec3::new(-(lo.x + hi.x) / 2.0, 0.0, -(lo.z + hi.z) / 2.0);
    let m = Mat4::from_translation(shift);
    for p in &mut r.parts {
        p.world = m * p.world;
    }
    for c in &mut r.colliders {
        c.center += shift;
    }
}
