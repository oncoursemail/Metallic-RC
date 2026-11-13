//! Two avatars in a room, over the live Kiwii realtime transport — the visible end of the `rr_net::kiwii` stack.
//!
//! This is a pure *spectator renderer* of the server's replication frames: it spawns one avatar per entity the
//! server reports (its own included — the server is authoritative and echoes every avatar in range), so it never has
//! to guess which id is "me". It also sends its own position as Input ~30 Hz, so from the server's point of view this
//! process is a real player the other clients can see too.
//!
//! Run the server, then this spectator at the origin and a second (headless) mover a few metres away:
//!   C:\rrt-rt\debug\realtime-capture.exe realtime 0.0.0.0:7777
//!   cargo run --profile fast -p metallic-rc-client --example kiwii_connect -- 3,0,0        # the other avatar
//!   cargo run --profile fast -p metallic-rc-client --example kiwii_room -- 0,0,0 shots     # this window + screenshot
//!
//! Args: `my_x,my_y,my_z` (my position, default 0,0,0) and an optional output dir; with the dir it screenshots once it
//! has two avatars in frame and exits, otherwise it just renders live. `KIWII_SERVER` overrides 127.0.0.1:7777.
use std::collections::HashMap;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, Kit, Look};
use rr_net::kiwii::{Client, DeviceProfile, Event};

/// What the net thread publishes for the render thread: whether we're connected and the latest pose of every avatar
/// the server has put in our replication frames, keyed by entity id. Positions are feet, yaw is radians.
#[derive(Default)]
struct Frame {
    connected: bool,
    ents: HashMap<u32, ([f32; 3], f32)>,
}

#[derive(Resource, Clone)]
struct NetState(Arc<Mutex<Frame>>);

#[derive(Resource)]
struct Out(Option<PathBuf>);

/// One networked avatar in the scene, by its server entity id.
#[derive(Component)]
struct NetAvatar(u32);

fn main() {
    let pos = std::env::args().nth(1).and_then(parse_pos).unwrap_or([0.0, 0.0, 0.0]);
    let out = std::env::args().nth(2).map(|d| {
        let p = PathBuf::from(d);
        std::fs::create_dir_all(&p).unwrap();
        p
    });

    let state = NetState(Arc::new(Mutex::new(Frame::default())));
    spawn_net_thread(state.clone(), pos);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: Kiwii room".into(), resolution: (1100, 720).into(), ..default() }),
            ..default()
        }))
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(GlobalAmbientLight { brightness: 500.0, ..default() })
        .insert_resource(state)
        .insert_resource(Out(out))
        .add_systems(Startup, scene)
        .add_systems(Update, (sync_avatars, maybe_screenshot))
        .run();
}

fn parse_pos(s: String) -> Option<[f32; 3]> {
    let p: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
    (p.len() == 3).then(|| [p[0], p[1], p[2]])
}

/// A clean lit room and a spectator camera framed on the stretch of floor the two avatars stand on.
fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let floor = materials.add(StandardMaterial { base_color: Color::srgb(0.82, 0.80, 0.78), perceptual_roughness: 0.95, ..default() });
    let wall = materials.add(StandardMaterial { base_color: Color::srgb(0.90, 0.89, 0.86), perceptual_roughness: 0.95, ..default() });
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))), MeshMaterial3d(floor)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(30.0, 8.0, 0.15))), MeshMaterial3d(wall.clone()), Transform::from_xyz(0.0, 4.0, -4.0)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(0.15, 8.0, 30.0))), MeshMaterial3d(wall), Transform::from_xyz(-6.0, 4.0, 0.0)));
    commands.spawn((DirectionalLight { illuminance: 6500.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((PointLight { intensity: 220_000.0, range: 24.0, ..default() }, Transform::from_xyz(1.5, 3.0, 4.0)));
    // Look down the +Z axis at the midpoint of the two spawn spots (origin and ~3m along +X).
    commands.spawn((Camera3d::default(), Transform::from_xyz(1.5, 1.7, 6.5).looking_at(Vec3::new(1.5, 1.0, 0.0), Vec3::Y)));
}

/// Each frame: spawn an avatar for every entity the server reports, move the ones we have, and drop the ones that left.
fn sync_avatars(
    mut commands: Commands,
    state: Res<NetState>,
    kit: Option<Res<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut avatars: Query<(Entity, &NetAvatar, &mut Transform)>,
) {
    let Some(kit) = kit else { return };
    let ents = state.0.lock().unwrap().ents.clone();
    let mut have = vec![];
    for (e, a, mut tr) in &mut avatars {
        match ents.get(&a.0) {
            Some(&(pos, yaw)) => {
                *tr = Transform::from_translation(Vec3::from(pos)).with_rotation(Quat::from_rotation_y(yaw));
                have.push(a.0);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for (&id, &(pos, yaw)) in ents.iter().filter(|(id, _)| !have.contains(id)) {
        // A distinct robot per id so the two avatars read apart at a glance; feet at `pos`, turned by `yaw`.
        let look = Look::for_name(&format!("kiwii-{id}"));
        let at = Transform::from_translation(Vec3::from(pos)).with_rotation(Quat::from_rotation_y(yaw));
        let e = avatar::spawn(&mut commands, &kit, &mut materials, look, at, Some(&format!("P{id}")), id);
        commands.entity(e).insert((NetAvatar(id), Name::new(format!("kiwii avatar {id}"))));
    }
}

/// Once we're connected and at least two avatars are in frame, let them settle, screenshot, and exit (only when an
/// output dir was given — otherwise this renders live and never exits).
fn maybe_screenshot(
    mut commands: Commands,
    out: Res<Out>,
    time: Res<Time>,
    state: Res<NetState>,
    avatars: Query<&NetAvatar>,
    mut exit: MessageWriter<AppExit>,
    mut settle: Local<f32>,
    mut shot: Local<bool>,
) {
    let Some(dir) = out.0.clone() else { return };
    let (connected, n) = { let f = state.0.lock().unwrap(); (f.connected, f.ents.len()) };
    let rendered = avatars.iter().count();
    if *shot {
        *settle += time.delta_secs();
        if *settle > 1.5 {
            exit.write(AppExit::Success);
        }
        return;
    }
    if connected && n >= 2 && rendered >= 2 {
        *settle += time.delta_secs();
        if *settle > 2.5 {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join("kiwii_room.png")));
            println!("kiwii_room: screenshot with {rendered} avatars ({n} entities in frame)");
            *shot = true;
            *settle = 0.0;
        }
    } else {
        // Safety valve: if the second avatar never shows up, bail after 25 s so a run can't hang forever.
        *settle += time.delta_secs();
        if *settle > 25.0 {
            eprintln!("kiwii_room: gave up waiting (connected={connected}, entities={n}, rendered={rendered})");
            exit.write(AppExit::Success);
        }
    }
}

/// The Kiwii net client on its own thread: handshake, then ~30 Hz Input and decode every replication frame into the
/// shared pose map. Mirrors the `kiwii_connect` example's socket handling (loopback bind, send_to/recv_from).
fn spawn_net_thread(state: NetState, my_pos: [f32; 3]) {
    std::thread::spawn(move || {
        let server = std::env::var("KIWII_SERVER").unwrap_or_else(|_| "127.0.0.1:7777".into());
        let server_addr: std::net::SocketAddr = server.parse().expect("server addr");
        let bind = if server_addr.ip().is_loopback() { "127.0.0.1:0" } else { "0.0.0.0:0" };
        let socket = UdpSocket::bind(bind).expect("bind");
        socket.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
        eprintln!("NET: bound {} -> {server_addr}", socket.local_addr().unwrap());

        let mut client = Client::new(DeviceProfile::PC);
        let n = socket.send_to(&client.connect_request(), server_addr);
        eprintln!("NET: sent ConnectRequest {n:?}");
        std::thread::sleep(Duration::from_millis(80));
        let n = socket.send_to(&client.client_hello(), server_addr);
        eprintln!("NET: sent ClientHello {n:?}");

        let mut buf = [0u8; 2048];
        let mut last_input = Instant::now();
        let mut last_log = Instant::now();
        let mut frames = 0u32;
        loop {
            if last_log.elapsed() > Duration::from_secs(2) {
                let st = state.0.lock().unwrap();
                eprintln!("NET: connected={} frames={frames} entities={:?}", st.connected, st.ents.keys().collect::<Vec<_>>());
                last_log = Instant::now();
            }
            if client.is_connected() && last_input.elapsed() > Duration::from_millis(33) {
                let _ = socket.send_to(&client.input_datagram(my_pos, 0.0), server_addr);
                last_input = Instant::now();
            }
            if let Ok((n, _src)) = socket.recv_from(&mut buf) {
                if let Some(ev) = client.on_datagram(&buf[..n]) {
                    match ev {
                        Event::Connected => {
                            state.0.lock().unwrap().connected = true;
                            eprintln!("NET: CONNECTED");
                        }
                        Event::Frame(f) => {
                            frames += 1;
                            let mut st = state.0.lock().unwrap();
                            for e in &f.entities {
                                let slot = st.ents.entry(e.id).or_insert(([0.0; 3], 0.0));
                                if let Some(p) = e.pos {
                                    slot.0 = p;
                                }
                                if let Some(y) = e.yaw {
                                    slot.1 = y;
                                }
                            }
                        }
                        Event::Transport => {}
                    }
                }
                for dg in client.outbox.drain(..) {
                    let _ = socket.send_to(&dg, server_addr);
                }
            }
        }
    });
}
