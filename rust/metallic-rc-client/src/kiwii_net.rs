//! The Kiwii realtime net path for the game client: a background thread runs an `rr_net::kiwii::Client` against the
//! realtime server, sending the local player's pose as Input and decoding the other players' avatars into the room.
//!
//! This is the integrated form of the `kiwii_room` example: the same handshake → Input → entity-delta-frame loop, but
//! feeding the game's real avatars (`avatar::spawn`) instead of a standalone spectator. `KIWII_SERVER` overrides the
//! default `127.0.0.1:7777`.
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use rr_net::kiwii::{Client, DeviceProfile, Event};

use crate::avatar::{self, Kit, Look};
use crate::player::{InTitle, Player, PlayerSystems};

/// Shared between the net thread and the game: whether we're connected, every other player's pose by entity id, and
/// the local player's pose for the thread to send as Input.
#[derive(Default)]
struct Shared {
    connected: bool,
    ents: HashMap<u32, ([f32; 3], f32)>,
    my_pose: ([f32; 3], f32),
}

#[derive(Resource, Clone)]
pub struct KiwiiNet(Arc<Mutex<Shared>>);

/// Another player's avatar in the room, by its server entity id.
#[derive(Component)]
pub struct KiwiiAvatar(u32);

pub struct KiwiiPlugin;

impl Plugin for KiwiiPlugin {
    fn build(&self, app: &mut App) {
        let net = KiwiiNet(Arc::default());
        spawn_net_thread(net.clone());
        app.insert_resource(net).add_systems(Update, kiwii_sync.after(PlayerSystems::Avatar));
    }
}

/// The net thread: Kiwii handshake, then ~30 Hz Input (the local pose) and decode every replication frame into the
/// shared pose map. Mirrors the `kiwii_room` example's socket handling (loopback bind, send_to/recv_from).
fn spawn_net_thread(net: KiwiiNet) {
    std::thread::spawn(move || {
        let server = std::env::var("KIWII_SERVER").unwrap_or_else(|_| "127.0.0.1:7777".into());
        let Ok(addr) = server.parse::<std::net::SocketAddr>() else { return };
        let bind = if addr.ip().is_loopback() { "127.0.0.1:0" } else { "0.0.0.0:0" };
        let Ok(socket) = UdpSocket::bind(bind) else { return };
        socket.set_read_timeout(Some(Duration::from_millis(20))).ok();

        let mut client = Client::new(DeviceProfile::PC);
        let _ = socket.send_to(&client.connect_request(), addr);
        std::thread::sleep(Duration::from_millis(80));
        let _ = socket.send_to(&client.client_hello(), addr);

        let mut buf = [0u8; 2048];
        let mut last_input = Instant::now();
        loop {
            if client.is_connected() && last_input.elapsed() > Duration::from_millis(33) {
                let (pos, yaw) = net.0.lock().unwrap().my_pose;
                let _ = socket.send_to(&client.input_datagram(pos, yaw), addr);
                last_input = Instant::now();
            }
            if let Ok((n, _src)) = socket.recv_from(&mut buf) {
                if let Some(ev) = client.on_datagram(&buf[..n]) {
                    match ev {
                        Event::Connected => net.0.lock().unwrap().connected = true,
                        Event::Frame(f) => {
                            let mut s = net.0.lock().unwrap();
                            for e in &f.entities {
                                let slot = s.ents.entry(e.id).or_insert(([0.0; 3], 0.0));
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
                    let _ = socket.send_to(&dg, addr);
                }
            }
        }
    });
}

/// Each frame: publish the local player's pose for the thread to send, then spawn/move/despawn the other players'
/// avatars from the latest replication frame. Our own avatar is echoed back by the server; we skip the entity sitting
/// on our own feet so it doesn't double the local avatar. No remote avatars show while the title screen is up.
#[allow(clippy::type_complexity)]
fn kiwii_sync(
    mut commands: Commands,
    net: Res<KiwiiNet>,
    kit: Option<Res<Kit>>,
    in_title: Res<InTitle>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player: Query<(&Player, &Transform), Without<KiwiiAvatar>>,
    mut avatars: Query<(Entity, &KiwiiAvatar, &mut Transform), Without<Player>>,
) {
    let Ok((p, t)) = player.single() else { return };
    let feet = p.feet(t);
    let ents = {
        let mut s = net.0.lock().unwrap();
        s.my_pose = ([feet.x, feet.y, feet.z], p.yaw);
        s.ents.clone()
    };
    let playing = !in_title.0;
    // our own echoed avatar: the entity within 0.6 m of our feet (don't render it over the local avatar)
    let mine = ents
        .iter()
        .map(|(id, v)| (*id, Vec3::from(v.0).distance(feet)))
        .filter(|(_, d)| *d < 0.6)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id);
    let wanted = |id: u32| playing && Some(id) != mine;

    let mut have = vec![];
    for (e, a, mut tr) in &mut avatars {
        match ents.get(&a.0).filter(|_| wanted(a.0)) {
            Some(&(pos, yaw)) => {
                *tr = Transform::from_translation(Vec3::from(pos)).with_rotation(Quat::from_rotation_y(yaw));
                have.push(a.0);
            }
            None => commands.entity(e).despawn(),
        }
    }
    let Some(kit) = kit.filter(|_| playing) else { return };
    for (&id, &(pos, yaw)) in ents.iter().filter(|(id, _)| wanted(**id) && !have.contains(id)) {
        let look = Look::for_name(&format!("kiwii{id}"));
        let at = Transform::from_translation(Vec3::from(pos)).with_rotation(Quat::from_rotation_y(yaw));
        let e = avatar::spawn(&mut commands, &kit, &mut materials, look, at, Some(&format!("P{id}")), id);
        commands.entity(e).insert((KiwiiAvatar(id), Name::new(format!("kiwii player {id}"))));
    }
}
