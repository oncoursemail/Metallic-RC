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
//! The escape menu's pages work with the mouse: settings (kept in `%LOCALAPPDATA%\Metallic-RC\settings.json`),
//! switching account (logs in again under another name) and quitting (leaves the room first).
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    input::{
        keyboard::{Key, KeyboardInput},
        mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
        ButtonState,
    },
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::{CursorGrabMode, CursorOptions, MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};
use metallic_rc_client::{
    avatar::{self, Drive, Kit, LocalLook, Look, LookPanel, Rig},
    collide::{self, Obb},
    net::{self, Login},
    netobj,
    realtime::{self, Realtime},
    room::{self, RoomData},
    settings::{Settings, RESOLUTIONS},
    shapes::{self, ShapeKit},
    ui::{input::{self, Pointer}, menu::{self, Action, Painted, Painter, Router}, view},
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

/// The escape menu: the pages open, the one on screen (with its root and the window size it was fitted to), the
/// pointer on it and the login page's name field.
#[derive(Resource, Default)]
struct Menu {
    painter: Option<Painter>,
    router: Router,
    shown: Option<(Painted, Entity, Vec2)>,
    pointer: Pointer,
    typed: String,
    focused: bool,
    scroll: HashMap<i64, f32>,
}

impl Menu {
    fn open(&self) -> bool {
        self.router.top().is_some()
    }

    /// Has the page on top painted as things are now.
    fn ask(&self, settings: &Settings, account: &str) {
        let (Some(p), Some(page)) = (&self.painter, self.router.top()) else { return };
        p.ask(menu::Request { page: page.into(), back_stack: self.router.back_stack(), settings: settings.clone(), account: account.into(),
            typed: self.typed.clone(), focused: self.focused, scroll: self.scroll.clone() });
    }
}

/// The player's settings; `changed` until `apply_settings` has applied and saved them.
#[derive(Resource)]
struct Prefs {
    s: Settings,
    changed: bool,
}

/// How to log in, and as whom. Each login thread runs while its number is the newest (`gen`).
#[derive(Resource)]
struct Account {
    server: String,
    realtime: Option<String>,
    room: Option<String>,
    /// feet position and facing (radians) handed out when this player is the room's master
    spawn: [f32; 3],
    yaw: f32,
    name: String,
    gen: Arc<AtomicUsize>,
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
    let game_dir = a.game_dir.clone().or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()));
    let menu = Menu { painter: game_dir.as_ref().map(|g| Painter::start(room::bundle_dir(g))), ..default() };
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
        *slot.lock().unwrap() = Some(load_dorm(&mut lib, &dir));
    });

    let status = Status(Arc::new(Mutex::new(format!("Connecting to {} ...", a.server))));
    let net = Net::default();
    let [x, y, z, yaw, _] = a.camera;
    let account = Account { server: a.server.clone(), realtime: a.realtime.clone(), room: a.room.clone(), spawn: [x, (y - EYE).max(0.0), z],
        yaw: yaw.to_radians(), name: a.name.clone(), gen: Arc::default() };
    log_in(&account, &status, &net);
    let mut prefs = Prefs { s: Settings::path().map(|p| Settings::load(&p)).unwrap_or_default(), changed: true };
    prefs.s.remember(&a.name);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC".into(), ..default() }),
            ..default()
        }))
        .insert_resource(status)
        .insert_resource(netobj::RoomLink(net.0.clone()))
        .add_plugins(netobj::NetObjectsPlugin)
        .insert_resource(net)
        .insert_resource(loading)
        .insert_resource(menu)
        .insert_resource(prefs)
        .insert_resource(account)
        .init_resource::<view::Uploaded>()
        .insert_resource(Start(a.camera))
        .init_resource::<Solids>()
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(LocalLook(look))
        .init_resource::<LookPanel>()
        .init_resource::<ThirdPerson>()
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (use_menu, show_menu.after(use_menu), grab_mouse.after(use_menu), apply_settings.after(use_menu), look_and_walk, show_status, spawn_room,
            avatar::panel::look_panel.run_if(not_typing), camera_view.after(look_and_walk), local_avatar.after(camera_view), realtime_sync.after(local_avatar)))
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
    (prefs, menu): (Res<Prefs>, Res<Menu>),
) {
    // the Field of View setting in place of the default
    let fov_setting = (prefs.s.get("fov") as f32).clamp(30.0, 120.0);
    if keys.just_pressed(KeyCode::KeyV) && !menu.focused {
        third.0 = !third.0;
    }
    let (p, eye) = *player;
    let (tr, projection) = &mut *view;
    let (want, fov) = if panel.open.is_some() {
        // world place first: 2.2 m in front, a little above the chest, looking back at the whole body
        let body = Quat::from_rotation_y(p.yaw);
        let feet = eye.translation - Vec3::Y * EYE;
        let at = Transform::from_translation(feet + body * Vec3::new(0.0, 1.3, -2.2)).looking_at(feet + Vec3::Y * 0.95, Vec3::Y);
        (Transform::from_matrix(eye.to_matrix().inverse() * at.to_matrix()), fov_setting * 0.85)
    } else if third.0 {
        let far = eye.transform_point(SHOULDER);
        // the furthest place along the way where a 0.15 m ball touches no wall
        let clear = (1..=20).map(|i| i as f32 / 20.0).take_while(|f| collide::sphere_clear(&solids.0, eye.translation.lerp(far, *f), 0.15)).last();
        (Transform::from_translation(SHOULDER * clear.unwrap_or(0.0)), fov_setting * 0.85)
    } else {
        (Transform::default(), fov_setting)
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

/// Logs in as `account.name` on a thread of its own, which then joins the room and keeps the notification hub open
/// until a newer login replaces it.
fn log_in(account: &Account, status: &Status, net: &Net) {
    let mine = account.gen.fetch_add(1, Ordering::SeqCst) + 1;
    let gen = account.gen.clone();
    let (server, address, room, spawn, yaw, name) = (account.server.clone(), account.realtime.clone(), account.room.clone(), account.spawn, account.yaw, account.name.clone());
    let (shared, net_slot) = (status.0.clone(), net.0.clone());
    *shared.lock().unwrap() = format!("Connecting to {server} as {name} ...");
    std::thread::spawn(move || {
        let s = match net::connect(&server, &Login::Name(name)) {
            Ok(s) => s,
            Err(e) => return *shared.lock().unwrap() = format!("Offline: {e}"),
        };
        let current = || gen.load(Ordering::SeqCst) == mine;
        if !current() {
            return;
        }
        let hub = s.open_hub();
        *shared.lock().unwrap() = format!("{}  |  {} ({})  |  {}  |  notifications {}", s.motd, s.account.display_name,
            s.account.username, if s.room.private { format!("{} (private)", s.room.name) } else { s.room.name.clone() },
            if hub.is_ok() { "on" } else { "off" });
        let address = address.unwrap_or_else(|| realtime::default_address(&server));
        let room = room.unwrap_or(s.room.photon_room);
        match realtime::start(&address, &room, &s.account.display_name, spawn, yaw) {
            Ok(rt) if current() => *net_slot.lock().unwrap() = Some(rt),
            Ok(rt) => rt.room.lock().unwrap().leave(rt.now()),
            Err(e) => shared.lock().unwrap().push_str(&format!("  |  room server {address}: {e}")),
        }
        while current() {
            std::thread::park_timeout(Duration::from_millis(500));
        }
        drop(hub);
    });
}

/// Leaves the room, if in one; returns the connection, which closes once the server has seen the goodbye.
fn leave_room(net: &Net) -> Option<Realtime> {
    let rt = net.0.lock().unwrap().take()?;
    rt.room.lock().unwrap().leave(rt.now());
    Some(rt)
}

/// The escape menu's input: Esc opens it, closes it or leaves the name field; the pointer presses and drags its
/// selectables, the wheel scrolls its lists, typing fills the name field, and clicks do what the page's controllers
/// say.
#[allow(clippy::too_many_arguments)]
fn use_menu(
    mut commands: Commands,
    (keys, mouse, wheel): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<AccumulatedMouseScroll>),
    mut typing: MessageReader<KeyboardInput>,
    mut menu: ResMut<Menu>,
    mut prefs: ResMut<Prefs>,
    mut account: ResMut<Account>,
    (net, status): (Res<Net>, Res<Status>),
    window: Single<&Window, With<PrimaryWindow>>,
    avatars: Query<Entity, With<Avatar>>,
    mut exit: MessageWriter<AppExit>,
) {
    let menu = &mut *menu;
    let page = menu.router.top().map(str::to_string);
    let mut repaint = false;
    let mut act = None;
    for k in typing.read().filter(|k| menu.focused && k.state == ButtonState::Pressed) {
        match &k.logical_key {
            Key::Backspace => drop(menu.typed.pop()),
            Key::Enter => act = Some(Action::Login),
            _ => menu.typed.extend(k.text.iter().flat_map(|t| t.chars()).filter(|c| !c.is_control()).take(32 - menu.typed.chars().count().min(32))),
        }
        repaint = true;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if menu.focused {
            menu.focused = false;
        } else if menu.open() {
            menu.router.close();
        } else {
            menu.router.open(menu::ESCAPE);
        }
        repaint = true;
    }
    if let (Some((p, _, _)), Some(cursor)) = (&menu.shown, window.cursor_position()) {
        let at = view::to_canvas(&p.drawn, window.size(), cursor);
        let over = input::hit(&p.drawn, at).map(|i| i.id);
        let click = menu.pointer.update(over, mouse.just_pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
        // a pressed slider follows the pointer
        let held = menu.pointer.down.and_then(|d| input::selectables(&p.drawn).into_iter().find(|i| i.id == d));
        if let (Some(i), Some(Action::Slider(key))) = (held, menu.pointer.down.and_then(|d| p.actions.get(&d))) {
            if let Some(v) = i.slider_value(at).filter(|v| *v as f64 != prefs.s.get(key)) {
                prefs.s.set(key, v as f64);
                (prefs.changed, repaint) = (true, true);
            }
        }
        act = act.or_else(|| click.and_then(|c| p.actions.get(&c).cloned()));
        if mouse.just_pressed(MouseButton::Left) && menu.focused && act != Some(Action::Field) {
            (menu.focused, repaint) = (false, true);
        }
        let lines = match wheel.unit { MouseScrollUnit::Line => wheel.delta.y * 60.0, MouseScrollUnit::Pixel => wheel.delta.y };
        for &(id, content, viewport) in p.scrolls.iter().filter(|_| lines != 0.0) {
            let (Some(c), Some(v)) = (input::placed(&p.drawn, content), input::placed(&p.drawn, viewport)) else { continue };
            if input::contains(&v, at) {
                let y = menu.scroll.entry(id).or_default();
                *y = (*y - lines).clamp(0.0, (c.h - v.h).max(0.0));
                repaint = true;
            }
        }
    }
    let mut switch_to = None;
    match act.clone() {
        Some(Action::Back) => drop(menu.router.back()),
        Some(Action::Close) => menu.router.close(),
        Some(Action::Go { page, tab: true }) => menu.router.tab(&page),
        Some(Action::Go { page, tab: false }) => menu.router.open(&page),
        Some(Action::SwitchAccount) => menu.router.open(menu::SWITCH_ACCOUNT),
        Some(Action::Quit) => {
            // out of the room before the window goes, so the others see this player leave at once
            if let Some(rt) = leave_room(&net) {
                let since = Instant::now();
                while !rt.room.lock().unwrap().is_closed() && since.elapsed() < Duration::from_secs(1) {
                    std::thread::yield_now();
                }
            }
            exit.write(AppExit::Success);
        }
        Some(Action::Toggle(key)) => {
            let on = prefs.s.on(&key);
            prefs.s.set(&key, if on { 0.0 } else { 1.0 });
            prefs.changed = true;
        }
        Some(Action::Field) => menu.focused = true,
        Some(Action::Login) => switch_to = Some(menu.typed.trim().to_string()).filter(|n| !n.is_empty()),
        Some(Action::Account(name)) => switch_to = Some(name),
        Some(Action::Slider(_)) | None => {}
    }
    if let Some(name) = switch_to {
        if name != account.name {
            leave_room(&net);
            for a in &avatars {
                commands.entity(a).despawn();
            }
            account.name = name;
            log_in(&account, &status, &net);
            prefs.s.remember(&account.name);
            prefs.changed = true;
        }
        menu.router.close();
        (menu.typed, menu.focused) = (String::new(), false);
    }
    if menu.router.top().map(str::to_string) != page {
        menu.pointer = Pointer::default();
    }
    if repaint || act.is_some() {
        menu.ask(&prefs.s, &account.name);
    }
}

/// Puts each page the painter finishes on screen (if it is still the one on top), fits it again when the window
/// changes size, and tints its selectables by the pointer.
#[allow(clippy::too_many_arguments)]
fn show_menu(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    status: Res<Status>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
    mut uploaded: ResMut<view::Uploaded>,
    mut graphics: Query<view::Tinted>,
) {
    let menu = &mut *menu;
    let size = window.size();
    let mut next = None;
    while let Some(r) = menu.painter.as_ref().and_then(Painter::take) {
        match r {
            Ok(p) => next = Some(p),
            Err(e) => status.0.lock().unwrap().push_str(&format!("  |  menu: {e}")),
        }
    }
    let resized = menu.shown.as_ref().is_some_and(|s| s.2 != size);
    let next = next.filter(|p| menu.router.top() == Some(p.page.as_str())).or_else(|| resized.then(|| menu.shown.take().map(|s| {
        commands.entity(s.1).despawn();
        s.0
    })).flatten());
    if !menu.open() || next.is_some() {
        if let Some((_, root, _)) = menu.shown.take() {
            commands.entity(root).despawn();
        }
    }
    if let Some(p) = next.filter(|_| menu.open()) {
        let root = view::spawn(&mut commands, &mut images, &mut fonts, &mut uploaded, &p.drawn, size);
        menu.shown = Some((p, root, size));
    }
    if let Some((p, _, _)) = &menu.shown {
        view::tint(&p.drawn, &menu.pointer, &mut graphics);
    }
}

/// Typing into the menu's name field takes the keys.
fn not_typing(menu: Res<Menu>) -> bool {
    !menu.focused
}

/// Puts changed window settings into effect and saves the settings (`camera_view` follows the field of view).
fn apply_settings(mut prefs: ResMut<Prefs>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    if !prefs.changed {
        return;
    }
    prefs.changed = false;
    let s = &prefs.s;
    let present = if s.on("vsync") { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    if window.present_mode != present {
        window.present_mode = present;
    }
    let mode = if s.on("fullscreen") { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
    if window.mode != mode {
        window.mode = mode;
    }
    let (w, h) = RESOLUTIONS[(s.get("resolution").max(0.0) as usize).min(RESOLUTIONS.len() - 1)];
    if mode == WindowMode::Windowed && (window.resolution.width(), window.resolution.height()) != (w as f32, h as f32) {
        window.resolution.set(w as f32, h as f32);
    }
    if let Some(path) = Settings::path() {
        if let Err(e) = s.save(&path) {
            warn!("settings not saved to {}: {e}", path.display());
        }
    }
}

fn grab_mouse(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, menu: Res<Menu>) {
    // clicks on an open menu are the menu's
    if mouse.just_pressed(MouseButton::Left) && !menu.open() {
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
    (prefs, menu): (Res<Prefs>, Res<Menu>),
) {
    let (p, t) = &mut *player;
    // typing into the menu does not walk
    let idle = ButtonInput::default();
    let keys = if menu.focused { &idle } else { &*keys };
    if cursor.grab_mode != CursorGrabMode::None {
        // 0.002 rad per pixel at the game's default sensitivity, 50; INFERRED: the look sliders scale each axis the same way
        let s = &prefs.s;
        let k = 0.002 * s.get("mouse_sensitivity") as f32 / 50.0;
        let axis = |look: &str, invert: &str| k * s.get(look) as f32 / 50.0 * if s.on(invert) { -1.0 } else { 1.0 };
        p.yaw -= motion.delta.x * axis("look_horizontal", "invert_x");
        p.pitch = (p.pitch - motion.delta.y * axis("look_vertical", "invert_y")).clamp(-1.5, 1.5);
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
