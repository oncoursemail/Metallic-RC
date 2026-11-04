//! Frame-time benchmark: loads a room the way the client does (`--world`, default the Dorm), flies the camera twice
//! around a fixed path inside the room's bounds (a warm-up lap, then a measured one, both by frame count so every
//! run renders the same views), and prints frame time p50/p95/p99, main-world and render-world CPU time, GPU time
//! per pass, triangles, drawn meshes, textures and process memory. Uncapped (no vsync), 1600x900 unless `--size`.
//!
//! `cargo run --profile fast -p metallic-rc-client --example bench -- [--world ADDRESS] [--frames N] [--size WxH]
//!  [--quality 0..3] [--texprep] [--no-envgen] [--cull-back] [--label TEXT] [--out FILE.jsonl] [--shot FILE.png]`
//! `--quality` applies `quality::Quality` (left out: the client's defaults, the baseline). `--out` appends one JSON
//! line per run.
use std::{collections::{HashMap, HashSet}, path::PathBuf, sync::{Arc, Mutex}};

use bevy::{
    asset::AssetEvent,
    diagnostic::{DiagnosticsStore, SystemInformationDiagnosticsPlugin},
    prelude::*,
    render::{diagnostic::RenderDiagnosticsPlugin, renderer::RenderAdapterInfo, view::screenshot::{save_to_disk, Screenshot}},
    window::{PresentMode, WindowResolution},
};
use metallic_rc_client::{perf::{Frames, PerfPlugin}, quality, room, texprep, world};

#[derive(Resource, Clone)]
struct Args {
    world: String,
    frames: u32,
    size: (u32, u32),
    quality: Option<u8>,
    /// `--texprep`: mips and block compression for room textures (texprep::TexPrepPlugin)
    texprep: bool,
    /// experiments on the Look lane's choices, to size them: `--no-envgen` drops the per-frame generated environment
    /// map (static sky), `--cull-back` makes every material single-sided
    no_envgen: bool,
    cull_back: bool,
    label: String,
    out: Option<PathBuf>,
    shot: Option<PathBuf>,
}

fn args() -> Args {
    let mut a = Args { world: "dorm".into(), frames: 600, size: (1600, 900), quality: None, texprep: false, no_envgen: false, cull_back: false, label: String::new(), out: None, shot: None };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("{k} needs a value"));
        match k.as_str() {
            "--world" => a.world = v(),
            "--frames" => a.frames = v().parse().expect("--frames N"),
            "--size" => {
                let s = v();
                let (w, h) = s.split_once('x').expect("--size WxH");
                a.size = (w.parse().unwrap(), h.parse().unwrap());
            }
            "--quality" => a.quality = Some(v().parse().expect("--quality 0..3")),
            "--texprep" => a.texprep = true,
            "--no-envgen" => a.no_envgen = true,
            "--cull-back" => a.cull_back = true,
            "--label" => a.label = v(),
            "--out" => a.out = Some(v().into()),
            "--shot" => a.shot = Some(v().into()),
            _ => panic!("unknown argument {k}"),
        }
    }
    a
}

/// The room as the loader hands it over.
#[derive(Resource, Clone)]
struct Slot(Arc<Mutex<Option<Result<room::RoomData, String>>>>);

/// Where the run is: waiting for the room, warming up, measuring, then the screenshot and the report.
#[derive(Resource, Default)]
struct Run {
    started: Option<std::time::Instant>,
    load_s: f32,
    /// the room's bounds, once it arrived
    bounds: Option<(Vec3, Vec3)>,
    /// frames since the room's entities were spawned
    frame: u32,
    /// sizes of the meshes and textures as they were added (their data leaves the main world once uploaded)
    triangles: HashMap<AssetId<Mesh>, u64>,
    texture_bytes: HashMap<AssetId<Image>, u64>,
    /// per measured frame: meshes visible, distinct mesh+material pairs, their triangles
    drawn: Vec<(u32, u32, u64)>,
}

#[derive(Component)]
struct BenchCamera;

fn main() {
    let a = args();
    let slot = Slot(Arc::new(Mutex::new(None)));
    let game_dir = room::GAME_DIRS.iter().map(PathBuf::from).find(|p| p.exists());
    let loader = world::Loader::start(game_dir, Some(a.world.clone()), slot.0.clone());
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Metallic RC bench".into(),
            resolution: WindowResolution::new(a.size.0, a.size.1),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins((RenderDiagnosticsPlugin, SystemInformationDiagnosticsPlugin, PerfPlugin, world::WorldPlugin))
    .insert_resource(loader)
    .insert_resource(slot)
    .insert_resource(Run { started: Some(std::time::Instant::now()), ..default() })
    .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
    .add_systems(Startup, setup)
    .add_systems(Update, (arrive, fly.after(arrive), experiments.after(arrive)))
    .add_systems(Last, count);
    if a.texprep {
        app.add_plugins(texprep::TexPrepPlugin);
    }
    if let Some(q) = a.quality {
        app.add_plugins(quality::QualityPlugin).insert_resource(quality::Quality::from_setting(q as f64));
    }
    app.insert_resource(a).run();
}

fn setup(mut commands: Commands) {
    // the client's camera (player.rs): fov 60, near 0.06, far 1000
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 60f32.to_radians(), near: 0.06, far: 1000.0, ..default() }),
        Transform::from_xyz(0.0, 1.6, 0.0),
        BenchCamera,
    ));
}

fn arrive(slot: Res<Slot>, mut arrived: ResMut<world::Arrived>, mut run: ResMut<Run>) {
    let Some(r) = slot.0.lock().unwrap().take() else { return };
    let r = r.unwrap_or_else(|e| panic!("room: {e}"));
    run.load_s = run.started.map_or(0.0, |t| t.elapsed().as_secs_f32());
    run.bounds = Some(world::bounds(&r));
    println!("bench: room in {:.2} s: {} roots, {} parts, {} materials, {} lights", run.load_s, r.roots.len(), r.parts.len(), r.materials.len(), r.lights.len());
    arrived.0 = Some(r);
}

/// The camera at `t` (0..1) of the lap: an ellipse at 40 % of the room's half size around its middle, at eye height
/// above its floor, looking ahead along the path and a little inwards, with a slow up-down nod.
fn on_path(lo: Vec3, hi: Vec3, t: f32) -> Transform {
    let mid = (lo + hi) / 2.0;
    let half = (hi - lo) / 2.0 * 0.4;
    let a = t * std::f32::consts::TAU;
    let eye = lo.y + 1.6_f32.min((hi.y - lo.y) * 0.5);
    let at = |a: f32| Vec3::new(mid.x + half.x * a.cos(), eye, mid.z + half.z * a.sin());
    let p = at(a);
    let ahead = at(a + 0.6).lerp(Vec3::new(mid.x, eye, mid.z), 0.3) + Vec3::Y * (a * 3.0).sin() * 0.6;
    Transform::from_translation(p).looking_at(ahead, Vec3::Y)
}

const SETTLE: u32 = 30;

#[allow(clippy::too_many_arguments)]
fn fly(
    mut commands: Commands,
    a: Res<Args>,
    mut run: ResMut<Run>,
    mut frames: ResMut<Frames>,
    mut camera: Single<&mut Transform, With<BenchCamera>>,
    pieces: Query<(), With<world::RoomPiece>>,
    store: Res<DiagnosticsStore>,
    adapter: Res<RenderAdapterInfo>,
    lights: (Query<&PointLight>, Query<&SpotLight>, Query<&DirectionalLight>),
    mut exit: MessageWriter<AppExit>,
) {
    let Some((lo, hi)) = run.bounds else { return };
    if pieces.is_empty() {
        return;
    }
    run.frame += 1;
    let n = a.frames;
    let f = run.frame;
    // settle, warm-up lap (pipelines compile, textures upload), measured lap, then the screenshot and the report
    let measure = SETTLE + n;
    let lap_t = |i: u32| (i % n) as f32 / n as f32;
    if f < measure + n {
        camera.as_mut().clone_from(&on_path(lo, hi, lap_t(f.saturating_sub(SETTLE))));
    }
    if f == measure {
        frames.recording = true;
        frames.samples.clear();
        frames.passes.clear();
        run.drawn.clear();
    }
    if f == measure + n {
        frames.recording = false;
        **camera = on_path(lo, hi, 0.0);
    }
    if f == measure + n + 5 {
        if let Some(path) = &a.shot {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
        }
    }
    if f < measure + n + 60 {
        return;
    }
    // the report
    let pc = |p: f32, g: fn(&metallic_rc_client::perf::Sample) -> f32| frames.percentile(p, g);
    let frame = |s: &metallic_rc_client::perf::Sample| s.frame_ms;
    let (p50, p95, p99) = (pc(50.0, frame), pc(95.0, frame), pc(99.0, frame));
    let main = pc(50.0, |s| s.main_ms);
    let render = pc(50.0, |s| s.render_ms);
    let gpu = pc(50.0, |s| s.gpu_ms);
    let gpu95 = pc(95.0, |s| s.gpu_ms);
    let mean = |v: &[(u32, u32, u64)], f: fn(&(u32, u32, u64)) -> f64| if v.is_empty() { 0.0 } else { v.iter().map(f).sum::<f64>() / v.len() as f64 };
    let visible = mean(&run.drawn, |d| d.0 as f64);
    let batches = mean(&run.drawn, |d| d.1 as f64);
    let tris = mean(&run.drawn, |d| d.2 as f64);
    let tex_mb = run.texture_bytes.values().sum::<u64>() as f64 / 1048576.0;
    let mem_gb = store.get(&SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE).and_then(|d| d.value()).unwrap_or(0.0);
    let (pl, sl, dl) = (&lights.0, &lights.1, &lights.2);
    let shadowed = pl.iter().filter(|l| l.shadow_maps_enabled).count() + sl.iter().filter(|l| l.shadow_maps_enabled).count() + dl.iter().filter(|l| l.shadow_maps_enabled).count();
    let lights_n = pl.iter().count() + sl.iter().count() + dl.iter().count();
    let fps = if p50 > 0.0 { 1000.0 / p50 } else { 0.0 };
    println!("\n== bench {} {} ({}x{}, quality {:?}) on {} ({:?}) ==", a.world, a.label, a.size.0, a.size.1, a.quality, adapter.name, adapter.backend);
    println!("frame ms   p50 {p50:.2}  p95 {p95:.2}  p99 {p99:.2}   ({fps:.0} fps at p50, {} frames)", frames.samples.len());
    println!("cpu ms     main p50 {main:.2}   render p50 {render:.2} (includes waiting for the GPU)");
    println!("gpu ms     p50 {gpu:.2}  p95 {gpu95:.2}");
    println!("scene      {visible:.0} meshes visible (any view), {batches:.0} distinct mesh+material, {:.0} k triangles in them", tris / 1000.0);
    println!("           {} textures {tex_mb:.0} MB (as uploaded), {lights_n} lights ({shadowed} with shadows), process memory {:.2} GB, load {:.2} s", run.texture_bytes.len(), mem_gb, run.load_s);
    let mut passes: Vec<(&String, f64)> = frames.passes.keys().filter(|k| k.ends_with("/elapsed_gpu")).filter_map(|k| Some((k, frames.pass_mean(k)?))).collect();
    passes.sort_by(|x, y| y.1.total_cmp(&x.1));
    for (k, ms) in passes.iter().take(10) {
        let pass = k.trim_end_matches("/elapsed_gpu");
        let prims = frames.pass_mean(&format!("{pass}/clipper_primitives_out")).unwrap_or(0.0);
        println!("  gpu {ms:7.3} ms  {pass}  ({:.0} k primitives)", prims / 1000.0);
    }
    if let Some(out) = &a.out {
        let line = format!(
            "{{\"world\":{:?},\"label\":{:?},\"size\":\"{}x{}\",\"quality\":{},\"adapter\":{:?},\"p50\":{p50:.3},\"p95\":{p95:.3},\"p99\":{p99:.3},\"main_p50\":{main:.3},\"render_p50\":{render:.3},\"gpu_p50\":{gpu:.3},\"gpu_p95\":{gpu95:.3},\"visible\":{visible:.0},\"batches\":{batches:.0},\"triangles\":{tris:.0},\"textures\":{},\"texture_mb\":{tex_mb:.1},\"lights\":{lights_n},\"shadowed\":{shadowed},\"mem_gb\":{mem_gb:.3},\"load_s\":{:.2}}}\n",
            a.world, a.label, a.size.0, a.size.1, a.quality.map_or("null".to_string(), |q| q.to_string()), adapter.name, run.texture_bytes.len(), run.load_s);
        use std::io::Write;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(out).and_then(|mut f| f.write_all(line.as_bytes()));
    }
    exit.write(AppExit::Success);
}

/// `--no-envgen` and `--cull-back`, applied to whatever the room load added.
fn experiments(
    mut commands: Commands,
    a: Res<Args>,
    envgen: Query<Entity, With<bevy::light::GeneratedEnvironmentMapLight>>,
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if a.no_envgen {
        for e in &envgen {
            commands.entity(e).remove::<bevy::light::GeneratedEnvironmentMapLight>();
        }
    }
    let added: Vec<_> = events.read().filter_map(|e| if let AssetEvent::Added { id } = e { Some(*id) } else { None }).collect();
    if a.cull_back {
        for id in added {
            if let Some(mut m) = materials.get_mut(id) {
                m.double_sided = false;
                m.cull_mode = Some(bevy::render::render_resource::Face::Back);
            }
        }
    }
}

/// Sizes of new meshes and textures (while their data is still here), and per measured frame what is drawn.
fn count(
    mut run: ResMut<Run>,
    frames: Res<Frames>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    drawn: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &ViewVisibility)>,
) {
    for e in mesh_events.read() {
        if let AssetEvent::Added { id } | AssetEvent::Modified { id } = e {
            if let Some(m) = meshes.get(*id) {
                let tris = m.indices().map_or(m.count_vertices(), |i| i.len()) as u64 / 3;
                run.triangles.insert(*id, tris);
            }
        }
    }
    for e in image_events.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                if let Some(i) = images.get(*id) {
                    run.texture_bytes.insert(*id, metallic_rc_client::quality::image_bytes(i));
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => drop(run.texture_bytes.remove(id)),
            _ => {}
        }
    }
    if !frames.recording {
        return;
    }
    let (mut n, mut tris) = (0, 0);
    let mut pairs = HashSet::new();
    for (m, mat, v) in &drawn {
        if v.get() {
            n += 1;
            tris += run.triangles.get(&m.id()).copied().unwrap_or(0);
            pairs.insert((m.id(), mat.id()));
        }
    }
    run.drawn.push((n, pairs.len() as u32, tris));
}
