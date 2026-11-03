//! metallic-rc-client [--server http://127.0.0.1:4200] [--name NAME] [--game-dir DIR] [--headless]
//!
//! Logs into metallic-rc-server on a background thread, then walks you around the room it puts you in (screen mode:
//! WASD, mouse look after a click, Space/Ctrl up/down, Esc frees the mouse). The Dorm's pieces are loaded from the
//! installed game (`--game-dir`, default: the Steam copies in `room::GAME_DIRS`); until they arrive, or without a
//! game folder, a stand-in box is shown.
//! `--headless` only runs the network start-up and prints where it ended up (for scripts and checks).
use std::sync::{Arc, Mutex};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    input::mouse::AccumulatedMouseMotion,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use metallic_rc_client::{
    net::{self, Login},
    room::{self, RoomData},
};

struct Args {
    server: String,
    name: String,
    headless: bool,
    game_dir: Option<std::path::PathBuf>,
}

fn args() -> Args {
    let mut a = Args { server: "http://127.0.0.1:4200".into(), name: "Player".into(), headless: false, game_dir: None };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        match k.as_str() {
            "--server" => a.server = it.next().unwrap_or_else(|| die("--server needs a URL")),
            "--name" => a.name = it.next().unwrap_or_else(|| die("--name needs a value")),
            "--headless" => a.headless = true,
            "--game-dir" => a.game_dir = Some(it.next().unwrap_or_else(|| die("--game-dir needs a folder")).into()),
            "-h" | "--help" => {
                println!("metallic-rc-client [--server URL] [--name NAME] [--game-dir DIR] [--headless]");
                std::process::exit(0)
            }
            _ => die(&format!("unknown argument {k}")),
        }
    }
    a
}

fn die(msg: &str) -> ! {
    eprintln!("metallic-rc-client: {msg}");
    std::process::exit(2)
}

/// What the network thread has to say, shown on screen.
#[derive(Resource, Clone)]
struct Status(Arc<Mutex<String>>);

#[derive(Component)]
struct StatusText;

/// The Dorm as the loader thread hands it over; taken once by `spawn_room`.
#[derive(Resource, Clone)]
struct Loading(Arc<Mutex<Option<Result<RoomData, String>>>>);

/// The stand-in room, removed when the real one arrives.
#[derive(Component)]
struct StandIn;

#[derive(Component, Default)]
struct Player {
    yaw: f32,
    pitch: f32,
}

fn main() {
    let a = args();
    if a.headless {
        match net::connect(&a.server, &Login::Name(a.name)) {
            Ok(s) => println!("logged in as {} (account {}), in {} instance {} (photon room {})",
                s.account.username, s.account.id, s.room.name, s.room.instance_id, s.room.photon_room),
            Err(e) => die(&e.to_string()),
        }
        return;
    }

    let loading = Loading(Arc::new(Mutex::new(None)));
    let slot = loading.0.clone();
    let game_dir = a.game_dir.clone().or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()));
    std::thread::spawn(move || {
        let r = match game_dir {
            Some(g) => load_dorm(&g),
            None => Err("no game folder found; pass --game-dir".into()),
        };
        *slot.lock().unwrap() = Some(r);
    });

    let status = Status(Arc::new(Mutex::new(format!("Connecting to {} ...", a.server))));
    let shared = status.0.clone();
    std::thread::spawn(move || {
        let s = match net::connect(&a.server, &Login::Name(a.name)) {
            Ok(s) => s,
            Err(e) => return *shared.lock().unwrap() = format!("Offline: {e}"),
        };
        // the hub stays open for the life of this thread, which lives as long as the game
        let hub = s.open_hub();
        *shared.lock().unwrap() = format!("{}  |  {} ({})  |  {}  |  notifications {}", s.motd, s.account.display_name,
            s.account.username, if s.room.private { format!("{} (private)", s.room.name) } else { s.room.name.clone() },
            if hub.is_ok() { "on" } else { "off" });
        loop {
            std::thread::park();
        }
    });

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC".into(), ..default() }),
            ..default()
        }))
        .insert_resource(status)
        .insert_resource(loading)
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (grab_mouse, look_and_walk, show_status, spawn_room))
        .run();
}

/// A stand-in dorm: floor, four walls, a bed and a desk, lit from the ceiling.
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let floor = materials.add(Color::srgb(0.55, 0.42, 0.30));
    let wall = materials.add(Color::srgb(0.82, 0.84, 0.88));
    let bed = materials.add(Color::srgb(0.25, 0.45, 0.80));
    let wood = materials.add(Color::srgb(0.45, 0.30, 0.18));
    let (w, d, h) = (6.0, 8.0, 3.0);

    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(w, d))), MeshMaterial3d(floor), StandIn));
    for (size, pos) in [
        (Vec3::new(w, h, 0.1), Vec3::new(0.0, h / 2.0, -d / 2.0)),
        (Vec3::new(w, h, 0.1), Vec3::new(0.0, h / 2.0, d / 2.0)),
        (Vec3::new(0.1, h, d), Vec3::new(-w / 2.0, h / 2.0, 0.0)),
        (Vec3::new(0.1, h, d), Vec3::new(w / 2.0, h / 2.0, 0.0)),
    ] {
        commands.spawn((Mesh3d(meshes.add(Cuboid::from_size(size))), MeshMaterial3d(wall.clone()), Transform::from_translation(pos), StandIn));
    }
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(1.2, 0.5, 2.1))), MeshMaterial3d(bed), Transform::from_xyz(-2.2, 0.25, -2.6), StandIn));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(1.4, 0.75, 0.7))), MeshMaterial3d(wood), Transform::from_xyz(2.2, 0.375, -3.4), StandIn));
    commands.spawn((PointLight { intensity: 400_000.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(0.0, h - 0.2, 0.0)));
    commands.spawn((DirectionalLight { illuminance: 3000.0, ..default() }, Transform::from_xyz(2.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));

    commands.spawn((Camera3d::default(), Player::default(), Transform::from_xyz(0.0, 1.6, 2.5)));
    commands.spawn((
        Text::new(""),
        StatusText,
        Node { position_type: PositionType::Absolute, top: px(12.0), left: px(12.0), ..default() },
    ));
}

fn grab_mouse(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>) {
    if mouse.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn look_and_walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut player: Single<(&mut Player, &mut Transform)>,
) {
    let (p, t) = &mut *player;
    if cursor.grab_mode != CursorGrabMode::None {
        p.yaw -= motion.delta.x * 0.002;
        p.pitch = (p.pitch - motion.delta.y * 0.002).clamp(-1.5, 1.5);
        t.rotation = Quat::from_euler(EulerRot::YXZ, p.yaw, p.pitch, 0.0);
    }
    let forward = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
    let mut go = Vec3::ZERO;
    for (key, dir) in [(KeyCode::KeyW, forward), (KeyCode::KeyS, -forward), (KeyCode::KeyD, right), (KeyCode::KeyA, -right),
                       (KeyCode::Space, Vec3::Y), (KeyCode::ControlLeft, -Vec3::Y)] {
        if keys.pressed(key) {
            go += dir;
        }
    }
    t.translation += go.normalize_or_zero() * 3.0 * time.delta_secs();
}

fn show_status(status: Res<Status>, mut text: Single<&mut Text, With<StatusText>>) {
    let now = status.0.lock().unwrap().clone();
    if text.0 != now {
        text.0 = now;
    }
}

/// The Dorm's pieces, from the Addressables bundle that holds them (found with unity-assets' catalog tool).
fn load_dorm(game_dir: &std::path::Path) -> Result<RoomData, String> {
    let dir = room::bundle_dir(game_dir);
    let mut lib = room::Library::new(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    room::load_prefabs(&mut lib, &dir, "43ea1176ca25c0333547813ec3298808.bundle", |n| n.starts_with("(Dorm_"))
}

fn spawn_room(
    mut commands: Commands,
    loading: Res<Loading>,
    status: Res<Status>,
    stand_in: Query<Entity, With<StandIn>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(result) = loading.0.lock().unwrap().take() else { return };
    let r = match result {
        Ok(r) => r,
        Err(e) => {
            status.0.lock().unwrap().push_str(&format!("  |  room: {e}"));
            return;
        }
    };
    for e in &stand_in {
        commands.entity(e).despawn();
    }
    let mats: Vec<Handle<StandardMaterial>> = r.materials.iter().map(|m| {
        let base_color_texture = m.texture.as_ref().map(|(w, h, px)| {
            let mut img = Image::new(Extent3d { width: *w, height: *h, depth_or_array_layers: 1 }, TextureDimension::D2, px.clone(),
                TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
            img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
            images.add(img)
        });
        let [cr, cg, cb, ca] = m.color;
        materials.add(StandardMaterial {
            base_color: Color::srgba(cr, cg, cb, ca),
            base_color_texture,
            perceptual_roughness: 0.8,
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    }).collect();
    for p in r.parts {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, p.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs);
        if !p.colors.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, p.colors);
        }
        mesh.insert_indices(Indices::U32(p.indices));
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats[p.material].clone()), Transform::from_matrix(p.world), Name::new(p.name)));
    }
    status.0.lock().unwrap().push_str(&format!("  |  {} Dorm pieces from the game files", r.roots.len()));
}
