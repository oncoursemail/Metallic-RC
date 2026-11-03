//! metallic-rc-client [--server http://127.0.0.1:4200] [--name NAME] [--headless]
//!
//! Logs into metallic-rc-server on a background thread, then walks you around the room it puts you in (screen mode:
//! WASD, mouse look after a click, Space/Ctrl up/down, Esc frees the mouse). The room is a stand-in box until the
//! asset reader can load the real dorm from the installed game.
//! `--headless` only runs the network start-up and prints where it ended up (for scripts and checks).
use std::sync::{Arc, Mutex};

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use metallic_rc_client::net::{self, Login};

struct Args {
    server: String,
    name: String,
    headless: bool,
}

fn args() -> Args {
    let mut a = Args { server: "http://127.0.0.1:4200".into(), name: "Player".into(), headless: false };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        match k.as_str() {
            "--server" => a.server = it.next().unwrap_or_else(|| die("--server needs a URL")),
            "--name" => a.name = it.next().unwrap_or_else(|| die("--name needs a value")),
            "--headless" => a.headless = true,
            "-h" | "--help" => {
                println!("metallic-rc-client [--server URL] [--name NAME] [--headless]");
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
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (grab_mouse, look_and_walk, show_status))
        .run();
}

/// A stand-in dorm: floor, four walls, a bed and a desk, lit from the ceiling.
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let floor = materials.add(Color::srgb(0.55, 0.42, 0.30));
    let wall = materials.add(Color::srgb(0.82, 0.84, 0.88));
    let bed = materials.add(Color::srgb(0.25, 0.45, 0.80));
    let wood = materials.add(Color::srgb(0.45, 0.30, 0.18));
    let (w, d, h) = (6.0, 8.0, 3.0);

    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(w, d))), MeshMaterial3d(floor)));
    for (size, pos) in [
        (Vec3::new(w, h, 0.1), Vec3::new(0.0, h / 2.0, -d / 2.0)),
        (Vec3::new(w, h, 0.1), Vec3::new(0.0, h / 2.0, d / 2.0)),
        (Vec3::new(0.1, h, d), Vec3::new(-w / 2.0, h / 2.0, 0.0)),
        (Vec3::new(0.1, h, d), Vec3::new(w / 2.0, h / 2.0, 0.0)),
    ] {
        commands.spawn((Mesh3d(meshes.add(Cuboid::from_size(size))), MeshMaterial3d(wall.clone()), Transform::from_translation(pos)));
    }
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(1.2, 0.5, 2.1))), MeshMaterial3d(bed), Transform::from_xyz(-2.2, 0.25, -2.6)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(1.4, 0.75, 0.7))), MeshMaterial3d(wood), Transform::from_xyz(2.2, 0.375, -3.4)));
    commands.spawn((PointLight { intensity: 400_000.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(0.0, h - 0.2, 0.0)));

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
