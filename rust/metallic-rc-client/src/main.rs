//! metallic-rc-client [--server http://127.0.0.1:4200] [--name NAME] [--game-dir DIR] [--room NAME] [--realtime ADDR]
//!                    [--look skin,shirt,accent[,accessory]] [--world ADDRESS] [--ref-install DIR|none] [--headless]
//!
//! Logs into metallic-rc-server on a background thread, then walks you around the room it puts you in (screen mode:
//! `player`: WASD, Shift sprint, Space jump, Ctrl crouch, F fly, mouse look after a click; Esc opens or closes the game's menu and frees the mouse). The Dorm's pieces are loaded from the
//! installed game (`--game-dir`, default: the Steam copies in `room::GAME_DIRS`); until they arrive, or without a
//! game folder, a stand-in box is shown.
//! After login it joins the room on the room server (`--realtime`, default: the server's host, UDP port 4201); other
//! players there show as avatars (`avatar`) with their name over the head. `--room` picks the room instance (default:
//! the one matchmaking gave), so two windows given the same `--room` meet.
//! V switches between first and third person; C opens the avatar editor (saved to `%LOCALAPPDATA%\Metallic-RC\avatar.json`;
//! `--look` sets one for this run only).
//! `--headless` only runs the network start-up and prints where it ended up (for scripts and checks).
//! The escape menu's pages work with the mouse: settings (kept in `%LOCALAPPDATA%\Metallic-RC\settings.json`),
//! switching account (logs in again under another name) and quitting (leaves the room first).
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::{
    input::{
        keyboard::{Key, KeyboardInput},
        mouse::{AccumulatedMouseScroll, MouseScrollUnit},
        ButtonState,
    },
    prelude::*,
    window::{CursorGrabMode, CursorOptions, MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use metallic_rc_client::{
    avatar::{self, Drive, Kit, LocalLook, Look, Rig},
    net::{self, Login},
    netobj,
    player::{self, Controls, LocalAvatar, Player, PlayerSystems},
    voice,
    realtime::{self, Realtime},
    room::{self, RoomData},
    settings::{Settings, Volumes, RESOLUTIONS},
    world,
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
    /// `--world`: a room by Addressables address (world::load)
    world: Option<String>,
}

fn args() -> Args {
    let mut a = Args { server: "http://127.0.0.1:4200".into(), name: "Player".into(), headless: false, game_dir: None, camera: [0.0, 1.55, 0.0, 0.0, 0.0], room: None, realtime: None, look: None, world: None };
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
            "--world" => a.world = Some(it.next().unwrap_or_else(|| die("--world needs an address"))),
            "--game-dir" => a.game_dir = Some(it.next().unwrap_or_else(|| die("--game-dir needs a folder")).into()),
            "--ref-install" => metallic_rc_client::levels::set_ref_install(Some(&it.next().unwrap_or_else(|| die("--ref-install needs a folder or none")))),
            "--scenes-dir" => metallic_rc_client::levels::set_scenes_dir(Some(&it.next().unwrap_or_else(|| die("--scenes-dir needs a folder or none")))),
            "-h" | "--help" => {
                println!("metallic-rc-client [--server URL] [--name NAME] [--game-dir DIR] [--camera x,y,z,yaw,pitch] [--room NAME] [--realtime ADDR] [--look skin,shirt,accent[,accessory]] [--world ADDRESS|SCENE] [--ref-install DIR|none] [--scenes-dir DIR|none] [--headless]");
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

/// What the network thread has to say, shown on screen: the login's line, then notes from the rest of the game (the
/// room's pieces, menu errors), which a new login keeps.
#[derive(Resource, Clone)]
struct Status(Arc<Mutex<String>>, Arc<Mutex<String>>);

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
    /// the dropdown setting whose choice dialog is open
    choosing: Option<String>,
    /// the name server, for the rooms browser's list
    server: String,
    /// the login token the pages were last painted with
    token: String,
}

impl Menu {
    fn open(&self) -> bool {
        self.router.top().is_some()
    }

    /// Has the page on top painted as things are now.
    fn ask(&self, settings: &Settings, account: &str) {
        let (Some(p), Some(page)) = (&self.painter, self.router.top()) else { return };
        p.ask(menu::Request { page: page.into(), back_stack: self.router.back_stack(), settings: settings.clone(), account: account.into(),
            typed: self.typed.clone(), focused: self.focused, scroll: self.scroll.clone(), choosing: self.choosing.clone(), server: self.server.clone(),
            token: self.token.clone() });
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
    /// the room to go to after logging in (the rooms browser), else the one the login gives (the dorm)
    goto: Option<String>,
    gen: Arc<AtomicUsize>,
    /// the newest login's access token (empty until it is signed in), for the menu's server calls
    token: Arc<Mutex<String>>,
}

#[derive(Resource)]
struct Start([f32; 5]);

/// The stand-in room, removed when the real one arrives.
#[derive(Component)]
struct StandIn;

/// The camera's field of view before the settings apply (the game's camera component).
const FOV: f32 = 60.0;

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
    let player = player::PlayerPlugin { game_dir: game_dir.clone() };
    let menu = Menu { painter: game_dir.as_ref().map(|g| Painter::start(room::bundle_dir(g))), server: a.server.clone(), ..default() };
    let loader = world::Loader::start(game_dir, a.world.clone(), slot);

    let status = Status(Arc::new(Mutex::new(format!("Connecting to {} ...", a.server))), Arc::default());
    let net = Net::default();
    let [x, y, z, yaw, _] = a.camera;
    let account = Account { goto: None, server: a.server.clone(), realtime: a.realtime.clone(), room: a.room.clone(), spawn: [x, (y - player::EYE).max(0.0), z],
        yaw: yaw.to_radians(), name: a.name.clone(), gen: Arc::default(), token: Arc::default() };
    log_in(&account, &status, &net);
    let mut prefs = Prefs { s: Settings::path().map(|p| Settings::load(&p)).unwrap_or_default(), changed: true };
    prefs.s.remember(&a.name);

    // METALLIC_HIDDEN=1: create the window but don't show it. The ECS (net, rules, bots) runs exactly the same, so a
    // net self-test can drive a game to a result and print its log without popping a window over whatever the owner is
    // doing (they're often on a call). The window still exists, so the systems that take `Single<&Window>` are fine.
    let hidden = std::env::var("METALLIC_HIDDEN").is_ok();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC".into(), visible: !hidden, ..default() }),
            ..default()
        }))
        .insert_resource(status)
        .insert_resource(netobj::RoomLink(net.0.clone()))
        .add_plugins((netobj::NetObjectsPlugin, voice::VoicePlugin))
        .insert_resource(net)
        .insert_resource(loading)
        .insert_resource(loader)
        .add_plugins(world::WorldPlugin)
        .insert_resource(menu)
        .insert_resource(Volumes::from(&prefs.s))
        .insert_resource(prefs)
        .insert_resource(account)
        .init_resource::<view::Uploaded>()
        .insert_resource(Start(a.camera))
        .add_plugins(avatar::AvatarPlugin)
        .add_plugins(metallic_rc_client::social::SocialPlugin)
        .add_plugins(metallic_rc_client::drive::DrivePlugin)
        // built-in game modes (Paintball), run by the room server
        .add_plugins(metallic_rc_client::games::GamesPlugin)
        .add_plugins(player)
        .add_plugins(metallic_rc_client::maker::MakerPlugin)
        // engine lane: quality tiers, texture mips/compression, platform defaults (touch, mobile)
        .add_plugins(metallic_rc_client::particles::ParticlePlugin)
        .add_plugins((metallic_rc_client::quality::QualityPlugin, metallic_rc_client::texprep::TexPrepPlugin, metallic_rc_client::platform::PlatformPlugin::default()))
        // dynamic-prop rigidbody physics foundation (bevy_rapier): mirrors the room's static colliders + lets games
        // spawn dynamic props (bowling pins, projectiles, karts) that a real solver simulates
        .add_plugins(metallic_rc_client::physics::RapierFoundationPlugin)
        // Dodgeball game mode: active when the Dodgeball arena is the loaded world (`--world dodgeball`)
        .add_plugins(metallic_rc_client::dodgeball::DodgeballPlugin)
        .insert_resource(metallic_rc_client::dodgeball::DodgeballActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("dodgeball"))))
        .add_plugins(metallic_rc_client::paddleball::PaddleballPlugin)
        .insert_resource(metallic_rc_client::paddleball::PaddleballActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("paddleball"))))
        .add_plugins(metallic_rc_client::discgolf::DiscGolfPlugin)
        .insert_resource(metallic_rc_client::discgolf::DiscGolfActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("discgolf"))))
        .add_plugins(metallic_rc_client::soccer::SoccerPlugin)
        .insert_resource(metallic_rc_client::soccer::SoccerActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("soccer"))))
        .add_plugins(metallic_rc_client::bowling::BowlingPlugin)
        .insert_resource(metallic_rc_client::bowling::BowlingActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("bowlingalley") || w.eq_ignore_ascii_case("bowling"))))
        .add_plugins(metallic_rc_client::charades::CharadesPlugin)
        .insert_resource(metallic_rc_client::charades::CharadesActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("charades"))))
        .add_plugins(metallic_rc_client::stuntrunner::StuntRunnerPlugin)
        .insert_resource(metallic_rc_client::stuntrunner::StuntRunnerActive(a.world.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("stuntrunner") || w.eq_ignore_ascii_case("level93") || w.eq_ignore_ascii_case("course1"))))
        .insert_resource(LocalLook(look))
        // the Customize screen edits this; `avatar::real::redress` applies it live to the real body
        .insert_resource(avatar::Dressing::default())
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (use_menu, show_menu.after(use_menu), grab_mouse.after(use_menu), apply_settings.after(use_menu), show_status, spawn_room,
            feed_player.after(use_menu).before(PlayerSystems::Move), realtime_sync.after(PlayerSystems::Avatar), follow_room, auto_shot))
        .run();
}

/// QA capture: with `METALLIC_SHOT=<path>` set, wait `METALLIC_SHOT_AT` seconds (default 7, to let the room and its
/// particles settle), save a screenshot of the window to `<path>`, then exit. No effect when the env var is unset.
#[allow(clippy::too_many_arguments)]
fn auto_shot(time: Res<Time>, mut commands: Commands, mut third: ResMut<player::ThirdPerson>, mut elapsed: Local<f32>, mut shot: Local<bool>, mut exit: MessageWriter<AppExit>, mut cam: Query<&mut Transform, With<Camera3d>>, bounds: Res<world::RoomBounds>) {
    let Ok(path) = std::env::var("METALLIC_SHOT") else { return };
    // METALLIC_DEMO_CAM: a demo camera framed from the loaded room's own bounds (the same 3/4 overview the offscreen
    // rasterizer uses), detached from the player and overriding the player-follow each frame. One rule frames every
    // arena cleanly — the ±14/±18 shell (Dodgeball/Paddleball) and the large open DiscGolf course alike.
    if std::env::var("METALLIC_DEMO_CAM").is_ok() {
        if let (Some(mut t), Some((lo, hi))) = (cam.iter_mut().next(), bounds.0) {
            let center = (lo + hi) * 0.5;
            let radius = (hi - lo).length() * 0.5;
            let eye = center + Vec3::new(0.9, 0.55, 0.9).normalize() * radius * 1.6;
            *t = Transform::from_translation(eye).looking_at(center, Vec3::Y);
        }
    } else {
        third.0 = true; // show the avatar in the room (not the first-person face) for the capture
    }
    *elapsed += time.delta_secs();
    let at = std::env::var("METALLIC_SHOT_AT").ok().and_then(|s| s.parse().ok()).unwrap_or(7.0);
    if !*shot && *elapsed >= at {
        *shot = true;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(std::path::PathBuf::from(path)));
    }
    if *shot && *elapsed >= at + 2.0 {
        exit.write(AppExit::Success);
    }
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
    commands.spawn((PointLight { intensity: 400_000.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(0.0, h - 0.2, 0.0), StandIn));
    commands.spawn((DirectionalLight { illuminance: 3000.0, ..default() }, Transform::from_xyz(2.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y), StandIn));

    player::spawn(&mut commands, start.0, FOV);
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
            p.respawn(Quat::from_array(rotation).to_euler(EulerRot::YXZ).0);
            t.translation = Vec3::from(position) + Vec3::Y * p.eye;
            t.rotation = Quat::from_rotation_y(p.yaw);
            *last_feet = None;
        }
    }
    let feet = p.feet(t);
    let velocity = last_feet.map_or(Vec3::ZERO, |l| (feet - l) / dt.max(1e-3) as f32);
    *last_feet = Some(feet);
    let body = Pose { position: feet.into(), rotation: Quat::from_rotation_y(p.yaw).to_array(), velocity: velocity.into() };
    let hand = |h: Option<Transform>| h.map(|h| Pose { position: h.translation.into(), rotation: h.rotation.to_array(), ..body });
    let (hands, grips) = mine.single().map_or(([None, None], [avatar::anim::Grip::Open; 2]), |d| (d.hands, d.grip));
    room.set_pose(AvatarData {
        tracking_space: Some(body),
        head: Some(Pose { position: t.translation.into(), rotation: t.rotation.to_array(), ..body }),
        left_hand: hand(hands[0]),
        right_hand: hand(hands[1]),
        grips: Some(grips.map(avatar::anim::Grip::to_net)),
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
                drive.head_height = s.head.map(|h| h.position[1] - s.body.position[1]);
                drive.grip = s.grips.map_or([avatar::anim::Grip::Open; 2], |g| g.map(avatar::anim::Grip::from_net));
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
    let goto = account.goto.clone();
    let (shared, net_slot, token) = (status.0.clone(), net.0.clone(), account.token.clone());
    token.lock().unwrap().clear();
    *shared.lock().unwrap() = format!("Connecting to {server} as {name} ...");
    std::thread::spawn(move || {
        let mut s = match net::connect(&server, &Login::Name(name)) {
            Ok(s) => s,
            Err(e) => return *shared.lock().unwrap() = format!("Offline: {e}"),
        };
        let went = goto.as_ref().map(|g| s.goto(g).map(|_| ()));
        let current = || gen.load(Ordering::SeqCst) == mine;
        if !current() {
            return;
        }
        *token.lock().unwrap() = s.access_token.clone();
        metallic_rc_client::social::attach(&s);
        let hub = s.open_hub();
        *shared.lock().unwrap() = format!("{}  |  {} ({})  |  {}  |  notifications {}", s.motd, s.account.display_name,
            s.account.username, if s.room.private { format!("{} (private)", s.room.name) } else { s.room.name.clone() },
            if hub.is_ok() { "on" } else { "off" });
        if let Some(Err(e)) = &went {
            shared.lock().unwrap().push_str(&format!("  |  {e}"));
        }
        let address = address.unwrap_or_else(|| realtime::default_address(&server));
        let room = if went.is_some() { s.room.photon_room.clone() } else { room.unwrap_or(s.room.photon_room.clone()) };
        metallic_rc_client::maker::attach(&s, &room);
        match realtime::start(&address, &room, &s.access_token, &s.account.display_name, spawn, yaw) {
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

/// The server moved this player (joined a friend, or the party leader moved the party): rejoin in that room.
fn follow_room(
    mut moved: MessageReader<metallic_rc_client::social::FollowRoom>,
    mut commands: Commands,
    mut account: ResMut<Account>,
    (net, status): (Res<Net>, Res<Status>),
    avatars: Query<Entity, With<Avatar>>,
) {
    let Some(metallic_rc_client::social::FollowRoom(to)) = moved.read().last().cloned() else { return };
    leave_room(&net);
    for a in &avatars {
        commands.entity(a).despawn();
    }
    account.room = Some(to.photon_room);
    log_in(&account, &status, &net);
    go_to_scene(&to.name, "");
}

/// Shows the room the player is going to (after leaving the old realtime room and starting the login that joins the
/// new one): `scene` from the room list, empty when not known (then the room's name says which).
/// STUB: the Rooms lane adds `world::go_to(scene)`; until that is merged here the old room stays on screen.
/// Integrator: replace the body with that call.
fn go_to_scene(room: &str, scene: &str) {
    info!("going to {room} (scene {scene:?}): world::go_to is not merged yet, the room on screen stays");
}

/// Leaves the room, if in one; returns the connection, which closes once the server has seen the goodbye.
fn leave_room(net: &Net) -> Option<Realtime> {
    let rt = net.0.lock().unwrap().take()?;
    rt.room.lock().unwrap().leave(rt.now());
    Some(rt)
}

/// Parses a `#rrggbb` (or `rrggbb`) colour into bytes.
fn hex_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() < 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

/// Applies a Customize-screen click to the local [`avatar::Dressing`]: a colour swatch (outfit type >= 1000 carries the
/// hex and `1000 + avatarColorType`) or a wardrobe item (the game's `OutfitType` and the item's style name).
fn wear(d: &mut avatar::Dressing, outfit_type: i32, val: &str) {
    if outfit_type >= 1000 {
        let Some(c) = hex_rgb(val) else { return };
        match outfit_type - 1000 {
            0 => d.skin = c,       // avatarColorType 0 = skin
            _ => d.hair_color = c, // 1 / 2 = hair primary / highlight
        }
        return;
    }
    let Some(slot) = avatar::real::Slot::from_outfit_type(outfit_type) else { return };
    use avatar::real::Slot;
    match slot {
        // the hair swap wants the bare style name (`redress` appends `_Hair`); the others want the full wardrobe stem
        Slot::Hair => d.hair_style = val.to_string(),
        Slot::Top => d.top = format!("{val}_{}", slot.suffix()),
        Slot::Bottom => d.bottom = format!("{val}_{}", slot.suffix()),
        Slot::Shoes => d.shoes = format!("{val}_{}", slot.suffix()),
        Slot::Hat => d.hat = format!("{val}_{}", slot.suffix()),
        Slot::Beard => d.beard = format!("{val}_{}", slot.suffix()),
        Slot::Glasses => d.glasses = format!("{val}_{}", slot.suffix()),
    }
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
    mut dress: ResMut<avatar::Dressing>,
    (net, status): (Res<Net>, Res<Status>),
    window: Single<&Window, With<PrimaryWindow>>,
    avatars: Query<Entity, With<Avatar>>,
    mut exit: MessageWriter<AppExit>,
    driven: Res<metallic_rc_client::drive::DrivenPointer>,
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
        } else if menu.choosing.is_some() {
            menu.choosing = None;
        } else if menu.open() {
            menu.router.close();
        } else {
            menu.router.open(menu::ESCAPE);
        }
        repaint = true;
    }
    // Tab: the watch (INFERRED key; the game opens its menu from a watch button)
    if keys.just_pressed(KeyCode::Tab) && !menu.focused {
        if menu.open() { menu.router.close() } else { menu.router.open(metallic_rc_client::ui::routes::HOME) }
        repaint = true;
    }
    if let (Some((p, _, _)), Some(cursor)) = (&menu.shown, driven.0.or(window.cursor_position())) {
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
    let (mut switch_to, mut rejoin, mut go_to) = (None, false, None);
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
        Some(Action::Choose(key, v)) => {
            prefs.s.set(&key, v);
            (prefs.changed, menu.choosing) = (true, None);
        }
        Some(Action::Dropdown(key)) => menu.choosing = Some(key),
        Some(Action::CloseDialog) => menu.choosing = None,
        Some(Action::Field) => menu.focused = true,
        Some(Action::Login) => switch_to = Some(menu.typed.trim().to_string()).filter(|n| !n.is_empty()),
        Some(Action::Account(name)) => switch_to = Some(name),
        Some(Action::GoToRoom(room, scene)) => {
            go_to = Some((room.clone(), scene));
            (account.goto, rejoin) = (Some(room), true);
        }
        // a Customize click: recolour or re-equip the live avatar (`avatar::real::redress` applies the change)
        Some(Action::WearItem(ot, val)) => wear(&mut dress, ot, &val),
        Some(Action::Slider(_)) | None => {}
    }
    // a new account goes to its dorm; a room from the rooms browser is joined by logging in again and going there
    let new_account = switch_to.as_ref().is_some_and(|n| *n != account.name);
    if new_account || rejoin {
        leave_room(&net);
        for a in &avatars {
            commands.entity(a).despawn();
        }
        if let Some(name) = switch_to.clone().filter(|_| new_account) {
            (account.name, account.goto) = (name, None);
            prefs.s.remember(&account.name);
            prefs.changed = true;
        }
        log_in(&account, &status, &net);
        let (room, scene) = go_to.unwrap_or_else(|| ("DormRoom".into(), String::new()));
        go_to_scene(&room, &scene);
    }
    if switch_to.is_some() || rejoin {
        menu.router.close();
        (menu.typed, menu.focused) = (String::new(), false);
    }
    if menu.router.top().map(str::to_string) != page {
        (menu.pointer, menu.choosing) = (Pointer::default(), None);
    }
    // the token arriving (or changing with the account) repaints: room lists are fetched with it
    let token = account.token.lock().unwrap().clone();
    if token != menu.token {
        menu.token = token;
        repaint |= menu.open();
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
            Err(e) => status.1.lock().unwrap().push_str(&format!("  |  menu: {e}")),
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

/// Hands the player the settings and whether the menu's name field takes the keys.
fn feed_player(prefs: Res<Prefs>, menu: Res<Menu>, mut controls: ResMut<Controls>) {
    if prefs.is_changed() {
        controls.settings = prefs.s.clone();
    }
    if controls.typing != menu.focused {
        controls.typing = menu.focused;
    }
}

/// Puts changed window settings and volumes into effect and saves the settings (`camera_view` follows the field of
/// view).
fn apply_settings(mut prefs: ResMut<Prefs>, mut window: Single<&mut Window, With<PrimaryWindow>>, mut volumes: ResMut<Volumes>) {
    if !prefs.changed {
        return;
    }
    prefs.changed = false;
    let s = &prefs.s;
    volumes.set_if_neq(Volumes::from(s));
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

/// The status line is a toast: up for a few seconds after it changes, then faded out. F3 keeps it and the room
/// line up (debug view).
#[allow(clippy::too_many_arguments)]
fn show_status(status: Res<Status>, time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, mut debug: Local<bool>, mut since: Local<f32>,
               mut text: Single<(&mut Text, &mut TextColor), With<StatusText>>, mut room: Query<&mut Visibility, With<RoomText>>) {
    let now = format!("{}{}", status.0.lock().unwrap(), status.1.lock().unwrap());
    *debug ^= keys.just_pressed(KeyCode::F3);
    if text.0 .0 != now {
        text.0 .0 = now;
        *since = 0.0;
    }
    *since += time.delta_secs();
    text.1 .0.set_alpha(if *debug { 1.0 } else { (6.0 - *since).clamp(0.0, 1.0) });
    for mut v in &mut room {
        *v = if *debug { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn spawn_room(mut commands: Commands, loading: Res<Loading>, status: Res<Status>, stand_in: Query<Entity, With<StandIn>>, mut arrived: ResMut<world::Arrived>) {
    let Some(result) = loading.0.lock().unwrap().take() else { return };
    let r = match result {
        Ok(r) => r,
        Err(e) => {
            status.1.lock().unwrap().push_str(&format!("  |  room: {e}"));
            return;
        }
    };
    for e in &stand_in {
        commands.entity(e).despawn();
    }
    status.1.lock().unwrap().push_str(&format!("  |  {} room pieces from the game files", r.roots.len()));
    arrived.0 = Some(r);
}
