//! metallic-rc-client [--server http://127.0.0.1:4200] [--name NAME] [--game-dir DIR] [--room NAME] [--realtime ADDR]
//!                    [--headless]
//!
//! Logs into metallic-rc-server on a background thread, then walks you around the room it puts you in (screen mode:
//! WASD, Shift sprint, Space jump, F fly (Space/Ctrl up/down), mouse look after a click, Esc frees the mouse). The Dorm's pieces are loaded from the
//! installed game (`--game-dir`, default: the Steam copies in `room::GAME_DIRS`); until they arrive, or without a
//! game folder, a stand-in box is shown.
//! After login it joins the room on the room server (`--realtime`, default: the server's host, UDP port 4201); other
//! players there show as capsules. `--room` picks the room instance (default: the one matchmaking gave), so two
//! windows given the same `--room` meet.
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
    collide::{self, Obb},
    net::{self, Login},
    realtime::{self, Realtime},
    room::{self, RoomData},
    shapes::{self, ShapeKit},
};
use rr_net::{room::RoomEvent, sync::{AvatarData, Pose}};

struct Args {
    server: String,
    name: String,
    headless: bool,
    game_dir: Option<std::path::PathBuf>,
    /// start position and view: x, y, z, yaw, pitch (metres, degrees)
    camera: [f32; 5],
    room: Option<String>,
    realtime: Option<String>,
}

fn args() -> Args {
    let mut a = Args { server: "http://127.0.0.1:4200".into(), name: "Player".into(), headless: false, game_dir: None, camera: [0.0, 1.55, 0.0, 0.0, 0.0], room: None, realtime: None };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        match k.as_str() {
            "--server" => a.server = it.next().unwrap_or_else(|| die("--server needs a URL")),
            "--name" => a.name = it.next().unwrap_or_else(|| die("--name needs a value")),
            "--headless" => a.headless = true,
            "--room" => a.room = Some(it.next().unwrap_or_else(|| die("--room needs a name"))),
            "--realtime" => a.realtime = Some(it.next().unwrap_or_else(|| die("--realtime needs ADDR:PORT"))),
            "--camera" => {
                let v: Vec<f32> = it.next().unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
                a.camera = v.try_into().unwrap_or_else(|_| die("--camera wants x,y,z,yaw,pitch"));
            }
            "--game-dir" => a.game_dir = Some(it.next().unwrap_or_else(|| die("--game-dir needs a folder")).into()),
            "-h" | "--help" => {
                println!("metallic-rc-client [--server URL] [--name NAME] [--game-dir DIR] [--camera x,y,z,yaw,pitch] [--room NAME] [--realtime ADDR] [--headless]");
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

/// The room connection, once the login thread has made it.
#[derive(Resource, Clone, Default)]
struct Net(Arc<Mutex<Option<Realtime>>>);

#[derive(Component)]
struct RoomText;

/// Another player's avatar, by actor number.
#[derive(Component)]
struct Avatar(i32);

/// The Dorm as the loader thread hands it over; taken once by `spawn_room`.
#[derive(Resource, Clone)]
struct Loading(Arc<Mutex<Option<Result<RoomData, String>>>>);

#[derive(Resource)]
struct Start([f32; 5]);

/// What the player bumps into: the room's box colliders, once it is loaded.
#[derive(Resource, Default)]
struct Solids(Vec<Obb>);

/// The stand-in room, removed when the real one arrives.
#[derive(Component)]
struct StandIn;

#[derive(Component, Default)]
struct Player {
    yaw: f32,
    pitch: f32,
    /// vertical speed, m/s
    fall: f32,
    flying: bool,
    grounded: bool,
}

// Screen-mode movement constants of the 2026 client (docs/agents/rc-spec/player.md in the private notes).
const WALK: f32 = 3.0;
const SPRINT: f32 = 1.5;
const FLY: f32 = 35.0;
const JUMP_HEIGHT: f32 = 0.75;
const GRAVITY: f32 = 9.81;
const MAX_FALL: f32 = 20.0;
/// eyes above the feet
const EYE: f32 = 1.55;

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
    let net = Net::default();
    let net_slot = net.0.clone();
    let [x, y, z, yaw, _] = a.camera;
    let spawn = [x, (y - EYE).max(0.0), z];
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
        let address = a.realtime.unwrap_or_else(|| realtime::default_address(&a.server));
        let room = a.room.unwrap_or(s.room.photon_room);
        match realtime::start(&address, &room, &s.account.display_name, spawn, yaw.to_radians()) {
            Ok(rt) => *net_slot.lock().unwrap() = Some(rt),
            Err(e) => shared.lock().unwrap().push_str(&format!("  |  room server {address}: {e}")),
        }
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
        .insert_resource(net)
        .insert_resource(loading)
        .insert_resource(Start(a.camera))
        .init_resource::<Solids>()
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (grab_mouse, look_and_walk, show_status, spawn_room, realtime_sync.after(look_and_walk)))
        .run();
}

/// A stand-in dorm: floor, four walls, a bed and a desk, lit from the ceiling.
fn setup(mut commands: Commands, start: Res<Start>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
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

    let [x, y, z, yaw, pitch] = start.0;
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 60f32.to_radians(), near: 0.06, ..default() }),
        // a start view above head height (an overview) starts flying instead of falling to the floor
        Player { yaw, pitch, flying: y > EYE + 0.5, ..default() },
        Transform::from_xyz(x, y, z).with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)),
    ));
    commands.spawn((
        Text::new(""),
        StatusText,
        Node { position_type: PositionType::Absolute, top: px(12.0), left: px(12.0), ..default() },
    ));
    commands.spawn((
        Text::new(""),
        RoomText,
        Node { position_type: PositionType::Absolute, bottom: px(12.0), left: px(12.0), ..default() },
    ));
}

/// Shapes for other players: a 1.7 m capsule and a visor on the side they face.
struct AvatarLook {
    body: Handle<Mesh>,
    visor: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
}

/// Each frame: take the room's news, hand it the local pose, and move (or add, or remove) the other players.
#[allow(clippy::too_many_arguments)]
fn realtime_sync(
    mut commands: Commands,
    time: Res<Time>,
    net: Res<Net>,
    mut player: Single<(&mut Player, &mut Transform), Without<Avatar>>,
    mut avatars: Query<(Entity, &Avatar, &mut Transform), Without<Player>>,
    mut text: Single<&mut Text, (With<RoomText>, Without<StatusText>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut look: Local<Option<AvatarLook>>,
    mut last_feet: Local<Option<Vec3>>,
) {
    let Some(rt) = net.0.lock().unwrap().clone() else { return };
    let dt = time.delta_secs_f64();
    let (p, t) = &mut *player;
    let mut room = rt.room.lock().unwrap();
    while let Some(e) = room.poll_event() {
        if let RoomEvent::Spawned { position, rotation } = e {
            t.translation = Vec3::from(position) + Vec3::Y * EYE;
            (p.yaw, p.pitch, p.fall, p.flying) = (Quat::from_array(rotation).to_euler(EulerRot::YXZ).0, 0.0, 0.0, false);
            t.rotation = Quat::from_rotation_y(p.yaw);
            *last_feet = None;
        }
    }
    let feet = t.translation - Vec3::Y * EYE;
    let velocity = last_feet.map_or(Vec3::ZERO, |l| (feet - l) / dt.max(1e-3) as f32);
    *last_feet = Some(feet);
    let body = Pose { position: feet.into(), rotation: Quat::from_rotation_y(p.yaw).to_array(), velocity: velocity.into() };
    room.set_pose(AvatarData { tracking_space: Some(body), head: Some(Pose { position: t.translation.into(), rotation: t.rotation.to_array(), ..body }), ..default() });
    let shown = room.remotes(rt.now(), dt);
    let line = match room.actor() {
        _ if room.is_closed() => "Room: disconnected".to_string(),
        None => "Room: connecting ...".to_string(),
        Some(actor) => {
            let names: Vec<&str> = shown.iter().map(|s| s.name.as_str()).collect();
            format!("Room: player {actor}{}, {} here{}{}", if room.is_master() { " (master)" } else { "" }, room.players(),
                if names.is_empty() { "" } else { ": " }, names.join(", "))
        }
    };
    drop(room);
    if text.0 != line {
        text.0 = line;
    }

    let look = look.get_or_insert_with(|| AvatarLook {
        body: meshes.add(Capsule3d::new(collide::RADIUS, collide::HEIGHT - 2.0 * collide::RADIUS)),
        visor: meshes.add(Cuboid::new(0.3, 0.1, 0.06)),
        skin: materials.add(Color::srgb(0.95, 0.62, 0.25)),
        glass: materials.add(Color::srgb(0.08, 0.1, 0.14)),
    });
    let mut have = vec![];
    for (e, a, mut tr) in &mut avatars {
        match shown.iter().find(|s| s.actor == a.0) {
            Some(s) => {
                tr.translation = Vec3::from(s.body.position);
                tr.rotation = Quat::from_array(s.body.rotation);
                have.push(a.0);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for s in shown.iter().filter(|s| !have.contains(&s.actor)) {
        let at = Transform::from_translation(Vec3::from(s.body.position)).with_rotation(Quat::from_array(s.body.rotation));
        commands.spawn((Avatar(s.actor), at, Visibility::default(), Name::new(format!("player {} {}", s.actor, s.name)))).with_children(|c| {
            c.spawn((Mesh3d(look.body.clone()), MeshMaterial3d(look.skin.clone()), Transform::from_xyz(0.0, collide::HEIGHT / 2.0, 0.0)));
            c.spawn((Mesh3d(look.visor.clone()), MeshMaterial3d(look.glass.clone()), Transform::from_xyz(0.0, EYE, -collide::RADIUS)));
        });
    }
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
    solids: Res<Solids>,
) {
    let (p, t) = &mut *player;
    if cursor.grab_mode != CursorGrabMode::None {
        p.yaw -= motion.delta.x * 0.002;
        p.pitch = (p.pitch - motion.delta.y * 0.002).clamp(-1.5, 1.5);
        t.rotation = Quat::from_euler(EulerRot::YXZ, p.yaw, p.pitch, 0.0);
    }
    // a frame that took long (the room arriving, a dragged window) moves no more than a tenth of a second's worth
    let dt = time.delta_secs().min(0.1);
    if keys.just_pressed(KeyCode::KeyF) {
        p.flying = !p.flying;
        p.fall = 0.0;
    }
    let forward = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
    let mut go = Vec3::ZERO;
    for (key, dir) in [(KeyCode::KeyW, forward), (KeyCode::KeyS, -forward), (KeyCode::KeyD, right), (KeyCode::KeyA, -right)] {
        if keys.pressed(key) {
            go += dir;
        }
    }
    if p.flying {
        for (key, dir) in [(KeyCode::Space, Vec3::Y), (KeyCode::ControlLeft, -Vec3::Y)] {
            if keys.pressed(key) {
                go += dir;
            }
        }
        // full fly speed is for crossing big rooms; Shift gives it, otherwise walking pace
        let speed = if keys.pressed(KeyCode::ShiftLeft) { FLY } else { WALK };
        t.translation += go.normalize_or_zero() * speed * dt;
        return;
    }
    let speed = if keys.pressed(KeyCode::ShiftLeft) { WALK * SPRINT } else { WALK };
    if p.grounded && keys.just_pressed(KeyCode::Space) {
        p.fall = (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
    }
    p.fall = (p.fall - GRAVITY * dt).max(-MAX_FALL);
    let motion = go.normalize_or_zero() * speed * dt + Vec3::Y * p.fall * dt;
    let feet = t.translation - Vec3::Y * EYE;
    let (mut feet, mut grounded) = collide::move_capsule(&solids.0, feet, motion);
    // the stand-in room has no colliders: its floor is y = 0
    if solids.0.is_empty() && feet.y < 0.0 {
        (feet.y, grounded) = (0.0, true);
    }
    if grounded && p.fall < 0.0 {
        p.fall = 0.0;
    }
    // a head bump ends the jump
    if feet.y < t.translation.y - EYE + motion.y - 1e-4 && p.fall > 0.0 {
        p.fall = 0.0;
    }
    p.grounded = grounded;
    t.translation = feet + Vec3::Y * EYE;
}

fn show_status(status: Res<Status>, mut text: Single<&mut Text, With<StatusText>>) {
    let now = status.0.lock().unwrap().clone();
    if text.0 != now {
        text.0 = now;
    }
}

/// Our placement of the Dorm pieces the game does not author in place (the original layout is in the scrambled
/// level files).
const DORM_LAYOUT: &str = include_str!("../rooms/dorm.layout");

/// The Dorm's pieces, from the Addressables bundle that holds them (found with unity-assets' catalog tool).
fn load_dorm(game_dir: &std::path::Path) -> Result<RoomData, String> {
    let dir = room::bundle_dir(game_dir);
    let mut lib = room::Library::new(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let layout = room::parse_layout(DORM_LAYOUT)?;
    // the Maker Pen textures come from an older install, when there is one
    let mut kit = ShapeKit::new(shapes::old_build_dirs().iter().find(|d| d.exists()).map(|d| d.as_path()));
    room::load_prefabs(&mut lib, &mut kit, &dir, "43ea1176ca25c0333547813ec3298808.bundle", |n| n.starts_with("(Dorm_"), &layout)
}

fn spawn_room(
    mut commands: Commands,
    loading: Res<Loading>,
    status: Res<Status>,
    stand_in: Query<Entity, With<StandIn>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut solids: ResMut<Solids>,
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
            perceptual_roughness: m.roughness.clamp(0.05, 1.0),
            metallic: 0.0,
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
    solids.0 = r.colliders;
}
