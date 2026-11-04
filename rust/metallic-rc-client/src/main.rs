//! metallic-rc-client [--server http://127.0.0.1:4200] [--name NAME] [--game-dir DIR] [--room NAME] [--realtime ADDR]
//!                    [--look skin,shirt,accent[,accessory]] [--headless]
//!
//! Logs into metallic-rc-server on a background thread, then walks you around the room it puts you in (screen mode:
//! WASD, Shift sprint, Space jump, F fly (Space/Ctrl up/down), mouse look after a click, Esc opens or closes the game's menu and frees the mouse). The Dorm's pieces are loaded from the
//! installed game (`--game-dir`, default: the Steam copies in `room::GAME_DIRS`); until they arrive, or without a
//! game folder, a stand-in box is shown.
//! After login it joins the room on the room server (`--realtime`, default: the server's host, UDP port 4201); other
//! players there show as avatars (`avatar`) with their name over the head. `--room` picks the room instance (default:
//! the one matchmaking gave), so two windows given the same `--room` meet.
//! V switches between first and third person; C opens the look panel (saved to `%LOCALAPPDATA%\Metallic-RC\avatar.json`;
//! `--look` sets one for this run only).
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
    avatar::{self, Drive, Kit, LocalLook, Look, LookPanel, Rig},
    collide::{self, Obb},
    net::{self, Login},
    realtime::{self, Realtime},
    room::{self, RoomData},
    shapes::{self, ShapeKit},
    ui::{paint, prefab, theme, view},
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
    look: Option<Look>,
}

fn args() -> Args {
    let mut a = Args { server: "http://127.0.0.1:4200".into(), name: "Player".into(), headless: false, game_dir: None, camera: [0.0, 1.55, 0.0, 0.0, 0.0], room: None, realtime: None, look: None };
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
            "--look" => a.look = Some(Look::parse_arg(&it.next().unwrap_or_default()).unwrap_or_else(|e| die(&e))),
            "--game-dir" => a.game_dir = Some(it.next().unwrap_or_else(|| die("--game-dir needs a folder")).into()),
            "-h" | "--help" => {
                println!("metallic-rc-client [--server URL] [--name NAME] [--game-dir DIR] [--camera x,y,z,yaw,pitch] [--room NAME] [--realtime ADDR] [--look skin,shirt,accent[,accessory]] [--headless]");
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

/// This player's own avatar: only its hands show in first person.
#[derive(Component)]
struct LocalAvatar;

/// The camera, a child of the player (at the eyes): there in first person, behind the shoulder in third.
#[derive(Component)]
struct View;

/// V toggles it.
#[derive(Resource, Default)]
struct ThirdPerson(bool);

/// The Dorm as the loader thread hands it over; taken once by `spawn_room`.
#[derive(Resource, Clone)]
struct Loading(Arc<Mutex<Option<Result<RoomData, String>>>>);

/// The escape menu page, painted by the loader thread; `toggle_menu` takes it once.
#[derive(Resource, Clone, Default)]
struct MenuSlot(Arc<Mutex<Option<Result<paint::Drawn, String>>>>);

/// The escape menu: its page once loaded, and its root while it is open.
#[derive(Resource, Default)]
struct Menu {
    page: Option<paint::Drawn>,
    open: Option<Entity>,
}

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
const FOV: f32 = 60.0;
/// ThirdPersonOverShoulder: from the pivot (1.6 m, so 0.05 above the eyes), right 0.4875 and back 1.8, FOV x0.85
const SHOULDER: Vec3 = Vec3::new(0.4875, 0.05, 1.8);

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

    // --look for this run, else the saved one, else one picked by name
    let look = a.look.or_else(Look::load).unwrap_or_else(|| Look::for_name(&a.name));
    let loading = Loading(Arc::new(Mutex::new(None)));
    let slot = loading.0.clone();
    let menu = MenuSlot::default();
    let menu_slot = menu.0.clone();
    let game_dir = a.game_dir.clone().or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()));
    std::thread::spawn(move || {
        let Some(game) = game_dir else {
            *slot.lock().unwrap() = Some(Err("no game folder found; pass --game-dir".into()));
            return;
        };
        let dir = room::bundle_dir(&game);
        let mut lib = match room::Library::new(&dir) {
            Ok(l) => l,
            Err(e) => return *slot.lock().unwrap() = Some(Err(format!("{}: {e}", dir.display()))),
        };
        // the menu first: it is small, and Esc may come before the room is in
        *menu_slot.lock().unwrap() = Some(load_menu(&mut lib, &dir));
        *slot.lock().unwrap() = Some(load_dorm(&mut lib, &dir));
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
        .insert_resource(menu)
        .init_resource::<Menu>()
        .insert_resource(Start(a.camera))
        .init_resource::<Solids>()
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(LocalLook(look))
        .init_resource::<LookPanel>()
        .init_resource::<ThirdPerson>()
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_menu, grab_mouse.after(toggle_menu), look_and_walk, show_status, spawn_room,
            avatar::panel::look_panel, camera_view.after(look_and_walk), local_avatar.after(camera_view), realtime_sync.after(local_avatar)))
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
        // a start view above head height (an overview) starts flying instead of falling to the floor
        Player { yaw, pitch, flying: y > EYE + 0.5, ..default() },
        Transform::from_xyz(x, y, z).with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)),
        Visibility::default(),
    )).with_child((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: FOV.to_radians(), near: 0.06, ..default() }),
        View,
        Transform::default(),
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

/// V switches first and third person. The camera eases to its place: at the eyes, over the shoulder (pulled in
/// where a wall is in the way), or, while the look panel is open, in front of the player looking back at it.
fn camera_view(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut third: ResMut<ThirdPerson>,
    panel: Res<LookPanel>,
    solids: Res<Solids>,
    player: Single<(&Player, &Transform), Without<View>>,
    mut view: Single<(&mut Transform, &mut Projection), With<View>>,
) {
    if keys.just_pressed(KeyCode::KeyV) {
        third.0 = !third.0;
    }
    let (p, eye) = *player;
    let (tr, projection) = &mut *view;
    let (want, fov) = if panel.open.is_some() {
        // world place first: 2.2 m in front, a little above the chest, looking back at the whole body
        let body = Quat::from_rotation_y(p.yaw);
        let feet = eye.translation - Vec3::Y * EYE;
        let at = Transform::from_translation(feet + body * Vec3::new(0.0, 1.3, -2.2)).looking_at(feet + Vec3::Y * 0.95, Vec3::Y);
        (Transform::from_matrix(eye.to_matrix().inverse() * at.to_matrix()), FOV * 0.85)
    } else if third.0 {
        let far = eye.transform_point(SHOULDER);
        // the furthest place along the way where a 0.15 m ball touches no wall
        let clear = (1..=20).map(|i| i as f32 / 20.0).take_while(|f| collide::sphere_clear(&solids.0, eye.translation.lerp(far, *f), 0.15)).last();
        (Transform::from_translation(SHOULDER * clear.unwrap_or(0.0)), FOV * 0.85)
    } else {
        (Transform::default(), FOV)
    };
    // CameraBlendTime 0.1 s
    let k = 1.0 - (-time.delta_secs() / 0.05).exp();
    tr.translation = tr.translation.lerp(want.translation, k);
    tr.rotation = tr.rotation.slerp(want.rotation, k);
    if let Projection::Perspective(pp) = &mut **projection {
        pp.fov += (fov.to_radians() - pp.fov) * k;
    }
}

/// The local player's own avatar: spawned once the kit is made, kept at the feet, driven by the movement, with the
/// screen-mode hands low in front of the eyes (swaying as the view turns and bobbing with the stride).
#[allow(clippy::too_many_arguments)]
fn local_avatar(
    mut commands: Commands,
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    look: Res<LocalLook>,
    third: Res<ThirdPerson>,
    panel: Res<LookPanel>,
    motion: Res<AccumulatedMouseMotion>,
    player: Single<(&Player, &Transform), Without<LocalAvatar>>,
    mut mine: Query<(&mut Transform, &mut Drive, &mut Rig), With<LocalAvatar>>,
    mut last_feet: Local<Option<Vec3>>,
    mut sway: Local<Vec2>,
) {
    let (p, eye) = *player;
    let feet = eye.translation - Vec3::Y * EYE;
    let at = Transform::from_translation(feet).with_rotation(Quat::from_rotation_y(p.yaw));
    let Ok((mut tr, mut drive, mut rig)) = mine.single_mut() else {
        if let Some(kit) = kit {
            let e = avatar::spawn(&mut commands, &kit, &mut materials, look.0, at, None, 0);
            commands.entity(e).insert((LocalAvatar, Name::new("local avatar")));
        }
        return;
    };
    let dt = time.delta_secs().max(1e-3);
    drive.velocity = last_feet.map_or(Vec3::ZERO, |l| (feet - l) / dt);
    *last_feet = Some(feet);
    (drive.grounded, drive.flying, drive.head) = (Some(p.grounded), p.flying, Some(eye.rotation));
    // the hands lag a little behind the turning view
    let target = (-motion.delta * 0.0004).clamp(Vec2::splat(-0.04), Vec2::splat(0.04)) * Vec2::new(1.0, -1.0);
    *sway = sway.lerp(target, 1.0 - (-dt * 8.0).exp());
    let bob = -0.015 * rig.anim.phase.sin().abs() * (rig.anim.speed / 3.0).min(1.0);
    drive.hands = avatar::screen_hands(eye, *sway, bob).map(Some);
    *tr = at;
    let first_person = !third.0 && panel.open.is_none();
    if rig.first_person != first_person {
        rig.first_person = first_person;
    }
    if rig.look != look.0 {
        rig.look = look.0;
    }
}

/// Each frame: take the room's news, hand it the local pose and look, and move (or add, or remove) the other players'
/// avatars.
#[allow(clippy::too_many_arguments)]
fn realtime_sync(
    mut commands: Commands,
    time: Res<Time>,
    net: Res<Net>,
    kit: Option<Res<Kit>>,
    look: Res<LocalLook>,
    mut player: Single<(&mut Player, &mut Transform), (Without<Avatar>, Without<LocalAvatar>)>,
    mine: Query<&Drive, (With<LocalAvatar>, Without<Avatar>)>,
    mut avatars: Query<(Entity, &Avatar, &mut Transform, &mut Drive, &mut Rig), (Without<Player>, Without<LocalAvatar>)>,
    mut text: Single<&mut Text, (With<RoomText>, Without<StatusText>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last_feet: Local<Option<Vec3>>,
    mut sent_look: Local<Option<Look>>,
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
    let hand = |h: Option<Transform>| h.map(|h| Pose { position: h.translation.into(), rotation: h.rotation.to_array(), ..body });
    let hands = mine.single().map_or([None, None], |d| d.hands);
    room.set_pose(AvatarData {
        tracking_space: Some(body),
        head: Some(Pose { position: t.translation.into(), rotation: t.rotation.to_array(), ..body }),
        left_hand: hand(hands[0]),
        right_hand: hand(hands[1]),
    });
    if *sent_look != Some(look.0) {
        room.set_look(rt.now(), look.0.to_json());
        *sent_look = Some(look.0);
    }
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

    let Some(kit) = kit else { return };
    let look_of = |s: &rr_net::room::Shown| s.look.as_deref().and_then(Look::from_json).unwrap_or_else(|| Look::for_name(&s.name));
    let pose = |p: Pose| Transform::from_translation(Vec3::from(p.position)).with_rotation(Quat::from_array(p.rotation));
    let mut have = vec![];
    for (e, a, mut tr, mut drive, mut rig) in &mut avatars {
        match shown.iter().find(|s| s.actor == a.0) {
            Some(s) => {
                *tr = pose(s.body);
                drive.velocity = Vec3::from(s.body.velocity);
                drive.head = s.head.map(|h| Quat::from_array(h.rotation));
                drive.hands = [s.left_hand.map(pose), s.right_hand.map(pose)];
                let l = look_of(s);
                if rig.look != l {
                    rig.look = l;
                }
                have.push(a.0);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for s in shown.iter().filter(|s| !have.contains(&s.actor)) {
        let e = avatar::spawn(&mut commands, &kit, &mut materials, look_of(s), pose(s.body), Some(&s.name), s.actor as u32);
        commands.entity(e).insert((Avatar(s.actor), Name::new(format!("player {} {}", s.actor, s.name))));
    }
}

/// Esc opens the escape menu over the room, or closes it.
fn toggle_menu(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    slot: Res<MenuSlot>,
    mut menu: ResMut<Menu>,
    status: Res<Status>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
) {
    if let Some(r) = slot.0.lock().unwrap().take() {
        match r {
            Ok(page) => menu.page = Some(page),
            Err(e) => status.0.lock().unwrap().push_str(&format!("  |  menu: {e}")),
        }
    }
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if let Some(root) = menu.open.take() {
        commands.entity(root).despawn();
    } else if let Some(page) = &menu.page {
        menu.open = Some(view::spawn(&mut commands, &mut images, &mut fonts, page, window.size()));
    }
}

fn grab_mouse(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, menu: Res<Menu>) {
    // clicks on an open menu are the menu's
    if mouse.just_pressed(MouseButton::Left) && menu.open.is_none() {
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

/// The escape menu page, themed and laid out on its canvas.
fn load_menu(lib: &mut room::Library, dir: &std::path::Path) -> Result<paint::Drawn, String> {
    let pages = prefab::Pages::open(dir)?;
    let mut page = prefab::load(lib, &pages, dir, "(EscapeMenu_standalone)")?;
    theme::Theme::load(lib, &pages, dir)?.apply(&mut page);
    // INFERRED: the canvas that hosts the standalone escape menu is not in its prefab; this size fits its content
    Ok(paint::page(lib, &page, [1000.0, 1150.0]))
}

/// The Dorm's pieces, from the Addressables bundle that holds them (found with unity-assets' catalog tool).
fn load_dorm(lib: &mut room::Library, dir: &std::path::Path) -> Result<RoomData, String> {
    let layout = room::parse_layout(DORM_LAYOUT)?;
    // the Maker Pen textures come from an older install, when there is one
    let mut kit = ShapeKit::new(shapes::old_build_dirs().iter().find(|d| d.exists()).map(|d| d.as_path()));
    room::load_prefabs(lib, &mut kit, dir, "43ea1176ca25c0333547813ec3298808.bundle", |n| n.starts_with("(Dorm_"), &layout)
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
