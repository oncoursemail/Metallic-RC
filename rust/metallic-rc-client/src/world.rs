//! Rooms by Addressables address: the catalog lookup (address -> bundle -> object), the inventory of what the
//! bundles can build, and `load` for `--world`.
//!
//! The 2026 level files are scrambled, so a "room" here is what the Addressables hold: one environment prefab, or a
//! group of prefabs sharing a name prefix (the Dorm's `(Dorm_*` pieces) placed by a layout file of ours.
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex},
};

use bevy::{asset::RenderAssetUsages, audio::AddAudioSource, mesh::{Indices, PrimitiveTopology}, prelude::*};
use unity_assets::{catalog::{bundle_file, Catalog}, classes, AssetBundleInfo, GameObject, PPtr};

use crate::{audio, cache, collide::{Collider, Solids}, render, room::{self, value, FileRef, Library, RoomData}, shapes::{self, ShapeKit}};


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
        let Ok(t) = value(&f, tid).and_then(|v| unity_assets::Transform::from_value(&v).map_err(|e| e.to_string())) else { continue };
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
    let mut kit = shape_kit();
    let mut addresses = None;
    load_with(lib, &mut kit, &mut addresses, dir, world)
}

/// The Maker Pen textures come from an older install, when there is one.
fn shape_kit() -> ShapeKit {
    ShapeKit::new(shapes::old_build_dirs().iter().find(|d| d.exists()).map(|d| d.as_path()))
}

fn load_with(lib: &mut Library, kit: &mut ShapeKit, addresses: &mut Option<Addresses>, dir: &Path, world: Option<&str>) -> Result<RoomData, String> {
    let world = world.unwrap_or("dorm");
    // The real decrypted 2026 scene first (the owner's "real rooms" bar): every non-group room, plus the Dorm (which is
    // still listed as a prefab GROUP but should render from its real scene `dormroom2`), routes through the corpus scene
    // loader. `load_scene` resolves the decrypted-scenes dir itself; `dir.ancestors().nth(3)` is the install's
    // `RecRoom_Data` (for the out-of-line `.resS`/`.resource` mesh streams). It returns None when the corpus has no such
    // scene, so game-specific groups (stuntrunner/sandbox) still fall through to their prefab path.
    let group_only = GROUPS.iter().any(|g| g.name.eq_ignore_ascii_case(world)) && !world.eq_ignore_ascii_case("dorm");
    if !group_only {
        if let Some(r) = crate::levels::load_scene(dir.ancestors().nth(3), world) {
            return r;
        }
    }
    // the old scene's layout and lights, when the reference install has this room (--ref-install)
    if let Some(r) = crate::levels::load(lib, kit, dir, world) {
        return r;
    }
    if let Some(g) = GROUPS.iter().find(|g| g.name.eq_ignore_ascii_case(world)) {
        let layout = room::parse_layout(g.layout)?;
        let want = |n: &str| n.starts_with(g.prefix) && (!g.only_layout || layout.iter().any(|(l, _)| l == n));
        return room::load_prefabs(lib, kit, dir, g.bundle, want, &layout);
    }
    if addresses.is_none() {
        *addresses = Some(Addresses::open(dir)?);
    }
    let addresses = addresses.as_ref().unwrap();
    let picked: Vec<(String, Location)> = match world.strip_suffix('*') {
        Some(prefix) => addresses.game_objects().into_iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, l)| (k.to_string(), l.clone())).collect(),
        None => addresses.by_key.get(world).filter(|l| l.resource_type == "UnityEngine.GameObject").map(|l| vec![(world.to_string(), l.clone())]).unwrap_or_default(),
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
    let mut r = room::load_roots(lib, kit, roots)?;
    if world.ends_with('*') || picked.len() == 1 {
        recenter(&mut r);
    }
    Ok(r)
}

/// Moves a room so the middle of its meshes is over the origin (where players start), its height left alone.
fn recenter(r: &mut RoomData) {
    let (lo, hi) = bounds(r);
    if lo.x > hi.x {
        return;
    }
    let shift = Vec3::new(-(lo.x + hi.x) / 2.0, 0.0, -(lo.z + hi.z) / 2.0);
    let m = Mat4::from_translation(shift);
    for w in r.parts.iter_mut().map(|p| &mut p.world).chain(r.places.iter_mut()).chain(r.lights.iter_mut().map(|l| &mut l.world)).chain(r.sounds.iter_mut().map(|s| &mut s.world)) {
        *w = m * *w;
    }
    for c in &mut r.colliders {
        *c = c.transformed(Mat4::from_translation(shift));
    }
}

/// The world-space box around a room's meshes (min > max when it has none).
pub fn bounds(r: &RoomData) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &r.parts {
        for &v in &p.positions {
            let w = p.world.transform_point3(v.into());
            lo = lo.min(w);
            hi = hi.max(w);
        }
    }
    (lo, hi)
}

// ---- in the game: the loader thread, rooms and props as entities ----

/// What the loader thread is asked to load.
enum Request {
    Prop { entity: Entity, address: String },
}

/// The loader thread: it owns the bundle library (bundles are read whole, so one copy), loads the room first and
/// then props on request (`spawn_prop`).
#[derive(Resource)]
pub struct Loader {
    send: Mutex<Sender<Request>>,
    props: Mutex<Receiver<(Entity, Result<RoomData, String>)>>,
}

impl Loader {
    /// Starts the thread. The room (`world`, see [`load`]) goes into `room` when it is ready.
    pub fn start(game_dir: Option<PathBuf>, world: Option<String>, room: Arc<Mutex<Option<Result<RoomData, String>>>>) -> Self {
        let (send, requests) = channel::<Request>();
        let (prop_send, props) = channel();
        std::thread::spawn(move || {
            let Some(game) = game_dir else {
                *room.lock().unwrap() = Some(Err("no game folder found; pass --game-dir".into()));
                return;
            };
            let dir = room::bundle_dir(&game);
            let t = std::time::Instant::now();
            let name = world.as_deref().unwrap_or("dorm");
            // the disk cache first (METALLIC_RC_NO_CACHE skips it, for timing the cold path); the bundles are
            // indexed after it, as props need them
            let cached = std::env::var_os("METALLIC_RC_NO_CACHE").is_none()
                .then(|| cache::file_for(&dir, name, &format!("{:?}{}{}", shapes::old_build_dirs().iter().find(|d| d.exists()), crate::levels::cache_tag(), crate::levels::scenes_cache_tag()))).flatten();
            let hit = cached.as_deref().and_then(cache::read);
            let from_cache = hit.is_some();
            if let Some(r) = hit {
                println!("world {name}: loaded from the cache in {:.2} s", t.elapsed().as_secs_f32());
                *room.lock().unwrap() = Some(Ok(r));
            }
            let mut lib = match Library::new(&dir) {
                Ok(l) => l,
                Err(e) => return *room.lock().unwrap() = Some(Err(format!("{}: {e}", dir.display()))),
            };
            let (mut kit, mut addresses) = (shape_kit(), None);
            if !from_cache {
                let r = load_with(&mut lib, &mut kit, &mut addresses, &dir, world.as_deref());
                println!("world {name}: loaded from the game files in {:.2} s", t.elapsed().as_secs_f32());
                if let (Ok(r), Some(path)) = (&r, &cached) {
                    let w = std::time::Instant::now();
                    cache::write(path, r);
                    println!("world {name}: cached in {:.2} s", w.elapsed().as_secs_f32());
                }
                *room.lock().unwrap() = Some(r);
            }
            // the room's ambient probe, reflection cubemap and fog. 2026 corpus scenes get their own baked lighting
            // (Env/Skybox BC6H reflection cubemaps); group-only worlds fall back to the reference install, mirroring the
            // geometry routing in load_with.
            let group_only = GROUPS.iter().any(|g| g.name.eq_ignore_ascii_case(name)) && !name.eq_ignore_ascii_case("dorm");
            let env = if !group_only {
                crate::levels::scene_lighting(dir.ancestors().nth(3), name).ok()
            } else {
                crate::levels::ref_install().is_some().then(|| crate::levels::lighting(name).ok()).flatten()
            };
            if let Some(l) = env {
                *render::ROOM_ENV.lock().unwrap() = Some(render::RoomEnv::new(&l));
            }
            for req in requests {
                match req {
                    Request::Prop { entity, address } => {
                        let r = load_with(&mut lib, &mut kit, &mut addresses, &dir, Some(&address)).map(|mut r| {
                            // a prop is placed by its entity: its parts relative to its root
                            unplace(&mut r);
                            r
                        });
                        if prop_send.send((entity, r)).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Loader { send: Mutex::new(send), props: Mutex::new(props) }
    }
}

/// Moves a one-root room back to its root's own frame (the root at the origin).
fn unplace(r: &mut RoomData) {
    let Some(&place) = r.places.first() else { return };
    let inv = place.inverse();
    for w in r.parts.iter_mut().map(|p| &mut p.world).chain(r.places.iter_mut()).chain(r.lights.iter_mut().map(|l| &mut l.world)).chain(r.sounds.iter_mut().map(|s| &mut s.world)) {
        *w = inv * *w;
    }
    for c in &mut r.colliders {
        *c = c.transformed(inv);
    }
}

/// The room as the loader made it, handed over by the client once (it also clears its stand-in room).
#[derive(Resource, Default)]
pub struct Arrived(pub Option<RoomData>);

/// The loaded room's world-space bounding box (min, max), for framing the demo/overview camera. `None` until a room
/// is in.
#[derive(Resource, Default)]
pub struct RoomBounds(pub Option<(Vec3, Vec3)>);

/// A spawnable or interactive object in the room: the prefab's address and an id that is the same on every client
/// that loads the same room (for room props) or that the spawner chose. The realtime lane syncs props by `id`.
#[derive(Component, Clone, Debug)]
pub struct Prop {
    pub address: String,
    pub id: u64,
}

/// A prop's colliders, in the prop's own space; `Solids` gets them where the prop is now.
#[derive(Component, Clone, Default)]
pub struct PropColliders(pub Vec<Collider>);

/// Every entity a room load made (despawned with the room).
#[derive(Component)]
pub struct RoomPiece;

/// The room's fixed colliders (props' are on their entities).
#[derive(Resource, Default)]
pub struct FixedSolids(pub Vec<Collider>);

/// FNV-1a: a stable id from a room name, a root's index and its name.
pub fn stable_id(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.bytes().chain([0]) {
            h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h & !(1 << 63)
}

/// Spawns the prefab at `address` as a prop at `transform`. The entity exists at once (with `Prop`); its meshes,
/// lights and colliders arrive when the loader has read them. Its `Prop::id` is a local one (top bit set, so it
/// never meets a room prop's); the realtime side may overwrite it with the network id.
pub fn spawn_prop(commands: &mut Commands, loader: &Loader, address: &str, transform: Transform) -> Entity {
    let id = stable_id(&["spawned", address, &format!("{:?}", transform.translation)]) | (1 << 63);
    let entity = commands.spawn((Prop { address: address.to_string(), id }, PropColliders::default(), transform, Visibility::default(), RoomPiece)).id();
    let _ = loader.send.lock().unwrap().send(Request::Prop { entity, address: address.to_string() });
    entity
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(render::LookPlugin)
            .init_resource::<Arrived>()
            .init_resource::<RoomBounds>()
            .init_resource::<FixedSolids>()
            .init_resource::<Solids>()
            .add_systems(Update, (spawn_arrived, attach_props, update_solids.after(spawn_arrived).after(attach_props)))
            .add_systems(Update, spawn_from_env.after(spawn_arrived))
            .add_audio_source::<audio::Pcm>()
            .init_resource::<audio::Volumes>()
            .add_systems(Update, (audio::watch_settings, audio::follow_listener.after(audio::watch_settings), audio::log_started.after(audio::follow_listener)));
    }
}

/// Builds the meshes, materials and lights of `r` as children of one entity per root prefab. Returns the root
/// entities; `roots[i]` is `None` for a root the caller already has an entity for (`existing`).
fn build(commands: &mut Commands, r: RoomData, room_name: &str, existing: Option<Entity>,
         meshes: &mut Assets<Mesh>, materials: &mut Assets<render::AgMaterial>, images: &mut Assets<Image>, clips: &mut Assets<audio::Pcm>) -> (Vec<Entity>, Vec<Collider>) {
    let mut textures = render::Textures::default();
    let mats: Vec<Handle<render::AgMaterial>> = r.materials.iter().map(|m| materials.add(render::material(m, images, &mut textures))).collect();
    let needs_tangents: Vec<bool> = r.materials.iter().map(|m| m.normal_map.is_some()).collect();
    let mut roots: Vec<Entity> = Vec::new();
    for (i, name) in r.roots.iter().enumerate() {
        let place = Transform::from_matrix(r.places[i]);
        let e = match existing {
            Some(e) => e,
            None => {
                let mut e = commands.spawn((place, Visibility::default(), Name::new(name.clone()), RoomPiece));
                if r.props[i] {
                    e.insert((Prop { address: name.clone(), id: stable_id(&[room_name, &i.to_string(), name]) }, PropColliders::default()));
                }
                e.id()
            }
        };
        roots.push(e);
    }
    let inverse: Vec<Mat4> = r.places.iter().map(|m| m.inverse()).collect();
    let (lo, hi) = bounds(&r);
    for p in r.parts {
        // a mirror's glass: its mean normal and centre, for the mirror camera
        let glass = (r.materials[p.material].shader == "AG/Mirror" && !p.positions.is_empty()).then(|| {
            let mean = |v: &[[f32; 3]]| v.iter().fold(Vec3::ZERO, |a, x| a + Vec3::from(*x)) / v.len().max(1) as f32;
            let center = mean(&p.positions);
            let radius = p.positions.iter().map(|x| Vec3::from(*x).distance(center)).fold(0.0, f32::max);
            crate::mirror::Mirror { normal: mean(&p.normals).normalize_or(Vec3::Z), center, radius }
        });
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, p.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs);
        if !p.colors.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, p.colors);
        }
        mesh.insert_indices(Indices::U32(p.indices));
        if needs_tangents[p.material] {
            let _ = mesh.generate_tangents();
        }
        let local = Transform::from_matrix(inverse[p.root] * p.world);
        let child = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats[p.material].clone()), local, Name::new(p.name))).id();
        if let Some(m) = glass {
            commands.entity(child).insert(m);
        }
        // glass and other see-through surfaces let the sun through
        if matches!(r.materials[p.material].blend, room::Blend::Blend | room::Blend::Add) {
            commands.entity(child).insert(bevy::light::NotShadowCaster);
        }
        commands.entity(roots[p.root]).add_child(child);
    }
    for l in &r.lights {
        let local = Transform::from_matrix(inverse[l.root] * l.world);
        let e = match render::sun_of(l, lo, hi) {
            Some(sun) => render::spawn_sun(commands, sun, local.rotation, lo, hi),
            None => render::spawn_light(commands, l, local),
        };
        commands.entity(roots[l.root]).add_child(e);
    }
    // one asset per clip, however many sources play it
    let mut pcm: HashMap<*const audio::Clip, Handle<audio::Pcm>> = HashMap::new();
    for s in r.sounds {
        let handle = pcm.entry(Arc::as_ptr(&s.clip)).or_insert_with(|| clips.add(audio::Pcm::new(&s.clip))).clone();
        let local = Transform::from_matrix(inverse[s.root] * s.world);
        let (root, playback, music) = (s.root, audio::playback(&s), s.looped && s.spatial < 0.5);
        let e = commands.spawn((AudioPlayer(handle), playback, local, Name::new(s.clip.name.clone()), audio::RoomAudio { sound: s, music })).id();
        commands.entity(roots[root]).add_child(e);
    }
    // props carry their own colliders; the rest are fixed
    let mut fixed = Vec::new();
    let mut by_root: HashMap<usize, Vec<Collider>> = HashMap::new();
    for (c, &root) in r.colliders.iter().zip(&r.collider_roots) {
        if r.props[root] || existing.is_some() {
            by_root.entry(root).or_default().push(c.transformed(inverse[root]));
        } else {
            fixed.push(c.clone());
        }
    }
    for (root, boxes) in by_root {
        commands.entity(roots[root]).insert(PropColliders(boxes));
    }
    (roots, fixed)
}

#[allow(clippy::too_many_arguments)]
fn spawn_arrived(
    mut commands: Commands,
    mut arrived: ResMut<Arrived>,
    mut fixed: ResMut<FixedSolids>,
    old: Query<Entity, With<RoomPiece>>,
    cameras: Query<Entity, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<render::AgMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut clips: ResMut<Assets<audio::Pcm>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    env: Res<render::RoomEnvMaps>,
    mut room_bounds: ResMut<RoomBounds>,
    mut player: Single<(&mut Transform, &mut crate::player::Player), Without<Camera3d>>,
) {
    let Some(r) = arrived.0.take() else { return };
    // place the player at the real scene's spawn point (SceneSpawnPoint/GamePlayerSpawnPoint), so they stand on the
    // real floor among the real colliders instead of at the world origin (where they'd be mis-placed / clip the scene).
    // TODO: dormroom2's spawn uses a spawn-point script class scene_spawns doesn't yet match, so r.spawns is empty for
    // it — fall back to the real traced dorm spawn (maps lane) until scene_spawns learns the dorm's spawn class.
    let dorm_name = r.roots.first().is_some_and(|n| n.contains("dormroom2"));
    let spawn = r.spawns.first().copied()
        .or_else(|| (dorm_name && r.spawns.is_empty()).then(|| Mat4::from_translation(Vec3::new(-2.96, 0.01, 1.12))));
    for e in &old {
        commands.entity(e).despawn();
    }
    let (lo, hi) = bounds(&r);
    room_bounds.0 = Some((lo, hi));
    let has_lights = !r.lights.is_empty();
    let name = r.roots.first().cloned().unwrap_or_default();
    let props: Vec<&str> = r.roots.iter().zip(&r.props).filter(|(_, p)| **p).map(|(n, _)| n.as_str()).collect();
    let sounds: Vec<String> = r.sounds.iter().map(|s| format!("{} ({}, {:.1} s{})", s.clip.name, s.clip.codec,
        s.clip.samples.len() as f32 / s.clip.channels.max(1) as f32 / s.clip.rate.max(1) as f32, if s.looped { ", loop" } else { "" })).collect();
    println!("world: {} pieces, {} props {props:?}, {} lights, {} colliders, {} sounds {sounds:?}", r.roots.len(), props.len(), r.lights.len(), r.colliders.len(), sounds.len());
    let (_, solids) = build(&mut commands, r, &name, None, &mut meshes, &mut materials, &mut images, &mut clips);
    fixed.0 = solids;
    if let Some(m) = spawn {
        let (_, rot, feet) = m.to_scale_rotation_translation();
        let (yaw, ..) = rot.to_euler(EulerRot::YXZ);
        let (t, p) = &mut *player;
        p.yaw = yaw;
        p.motor.flying = false;
        t.translation = feet + Vec3::Y * p.eye;
        t.rotation = Quat::from_euler(EulerRot::YXZ, yaw, p.pitch, 0.0);
    }
    if !has_lights && lo.x <= hi.x {
        for e in render::spawn_default_lights(&mut commands, lo, hi) {
            commands.entity(e).insert(RoomPiece);
        }
    }
    // the sky lights the room (sky_on_camera); a flat ambient on top would wash out its shading
    ambient.brightness = 0.0;
    let sky = env.sky.as_ref().map_or_else(|| images.add(render::sky_cubemap(&render::SimpleGradient::default())), |s| s.0.clone());
    let inside = images.add(render::sky_cubemap(&render::SimpleGradient::interior()));
    for cam in &cameras {
        // the room's own environment (render::apply_room_env) replaces the stand-in ambient when it came first
        let ambient = env.ambient.clone().unwrap_or(inside.clone());
        render::sky_on_camera(&mut commands, cam, sky.clone(), env.sky.as_ref().map_or(Quat::IDENTITY, |s| s.1), ambient);
        commands.entity(cam).insert(audio::listener());
    }
}

/// Test aid: `METALLIC_RC_SPAWN="address@x,y,z;..."` spawns those props once the room is in.
fn spawn_from_env(mut commands: Commands, loader: Option<Res<Loader>>, pieces: Query<(), With<RoomPiece>>, mut done: Local<bool>) {
    let (Some(loader), false, false) = (loader, *done, pieces.is_empty()) else { return };
    *done = true;
    for item in std::env::var("METALLIC_RC_SPAWN").unwrap_or_default().split(';').filter(|s| !s.is_empty()) {
        let (address, at) = item.rsplit_once('@').unwrap_or((item, "0,1,0"));
        let v: Vec<f32> = at.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let pos = Vec3::new(*v.first().unwrap_or(&0.0), *v.get(1).unwrap_or(&1.0), *v.get(2).unwrap_or(&0.0));
        let e = spawn_prop(&mut commands, &loader, address, Transform::from_translation(pos));
        println!("spawned prop {address} at {pos} as {e}");
    }
}

/// Props whose meshes the loader has read: their parts go under the entity spawned for them.
fn attach_props(
    mut commands: Commands,
    loader: Option<Res<Loader>>,
    alive: Query<(), With<Prop>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<render::AgMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut clips: ResMut<Assets<audio::Pcm>>,
) {
    let Some(loader) = loader else { return };
    let ready: Vec<_> = loader.props.lock().unwrap().try_iter().collect();
    for (entity, r) in ready {
        match r {
            Ok(r) if alive.get(entity).is_ok() => {
                let sounds: Vec<&str> = r.sounds.iter().map(|s| s.clip.name.as_str()).collect();
                let unplayable: Vec<&String> = r.problems.iter().filter(|p| p.starts_with("audio")).collect();
                if !sounds.is_empty() || !unplayable.is_empty() {
                    println!("prop {entity}: sounds {sounds:?}, unplayable {unplayable:?}");
                }
                build(&mut commands, r, "", Some(entity), &mut meshes, &mut materials, &mut images, &mut clips);
            }
            Ok(_) => {}
            Err(e) => eprintln!("prop: {e}"),
        }
    }
}

/// The player's boxes: the fixed ones plus every prop's where it is now.
fn update_solids(fixed: Res<FixedSolids>, props: Query<(&GlobalTransform, &PropColliders)>, mut solids: ResMut<Solids>) {
    let mut all = fixed.0.clone();
    for (t, c) in &props {
        let m = t.to_matrix();
        all.extend(c.0.iter().map(|b| b.transformed(m)));
    }
    solids.0 = all;
}

/// Debug aid: the value trees of the components of `address` (and its children) whose class or script name
/// contains `filter`, as `(GameObject name, component name, value)`.
pub fn dump_components(lib: &mut Library, dir: &Path, address: &str, filter: &str, limit: usize) -> Result<Vec<(String, String, unity_assets::Value)>, String> {
    let loc = Addresses::open(dir)?.by_key.get(address).cloned().ok_or("no such address")?;
    let (f, gid) = locate(lib, dir, &loc)?;
    let (tf, tid) = transform_of(lib, &f, gid).ok_or("no transform")?;
    let mut out = Vec::new();
    let mut stack = vec![(tf, tid)];
    while let Some((f, tid)) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let Ok(t) = value(&f, tid).and_then(|v| unity_assets::Transform::from_value(&v).map_err(|e| e.to_string())) else { continue };
        let Ok((gf, gid)) = lib.resolve(&f, t.game_object) else { continue };
        let Ok(go) = value(&gf, gid).and_then(|v| GameObject::from_value(&v).map_err(|e| e.to_string())) else { continue };
        for &c in &go.components {
            let Ok((cf, cid)) = lib.resolve(&gf, c) else { continue };
            let Some(class) = cf.sf().object(cid).map(|o| o.class_id) else { continue };
            let name = match class {
                classes::MONO_BEHAVIOUR => script_name(lib, &cf, cid).unwrap_or_default(),
                _ => classes::class_name(class).map_or_else(|| format!("Class{class}"), String::from),
            };
            if name.contains(filter) {
                if let Ok(v) = value(&cf, cid) {
                    out.push((go.name.clone(), name, v));
                }
            }
        }
        for &child in &t.children {
            if let Ok(r) = lib.resolve(&f, child) {
                stack.push(r);
            }
        }
    }
    Ok(out)
}
