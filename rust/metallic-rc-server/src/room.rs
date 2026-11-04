//! The room server: a UDP listener beside the HTTP API that does what the 2026 room server does for players in one
//! room instance (realtime.md section 8): join and leave, the master client, and the Photon-style relay of events
//! with its event cache. It never reads event content, only the envelope; gameplay authority is the master client.
//!
//! Phase 3 runs plain (marker 0x11) and takes any connection that names a room instance in its data
//! (`{"RI":"<room>"}`); token checks and encryption come with the 2026 handshake.
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use rr_net::events::ops::{Op, PlayerData, RoomData};
use rr_net::events::relay::{self, Proxied, Raise, Receivers};
use rr_net::events::{OP_DOWN, OP_UP, RELAY_DOWN, RELAY_UP};
use rr_net::ngo::{NgoDelivery, Server, ServerEvent};

/// The approval payload the 2026 client takes as success.
const APPROVED: &str = r#"{"DR":"Carrot"}"#;
const MAX_ROOM_NAME: usize = 128;

fn room_name(data: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(data).ok()?;
    let name = v.get("RI")?.as_str()?;
    (!name.is_empty() && name.len() <= MAX_ROOM_NAME).then(|| name.to_string())
}

/// An event kept for players who join later.
struct Cached {
    sender: i32,
    code: u8,
    content: Vec<u8>,
    /// AddToRoomCacheGlobal: kept when the sender leaves
    global: bool,
}

#[derive(Default)]
struct Room {
    /// in join order; the first is the master when the master leaves
    members: Vec<PlayerData>,
    master: Option<u64>,
    cache: Vec<Cached>,
}

impl Room {
    fn has(&self, client: u64) -> bool {
        self.members.iter().any(|p| p.client_id == client)
    }
}

pub struct Rooms {
    ngo: Server,
    rooms: HashMap<String, Room>,
    /// approved connections that have not sent JoinRequest yet, and the room they asked for
    pending: HashMap<u64, String>,
    room_of: HashMap<u64, String>,
}

impl Default for Rooms {
    fn default() -> Self {
        Self::new()
    }
}

impl Rooms {
    pub fn new() -> Self {
        let approve = Box::new(|data: &[u8]| match room_name(data) {
            Some(_) => Ok(APPROVED.to_string()),
            None => Err("no room instance in the connection data".to_string()),
        });
        Rooms { ngo: Server::new(approve), rooms: HashMap::new(), pending: HashMap::new(), room_of: HashMap::new() }
    }

    /// Client ids in a room, in join order.
    pub fn members(&self, room: &str) -> Vec<u64> {
        self.rooms.get(room).map(|r| r.members.iter().map(|p| p.client_id).collect()).unwrap_or_default()
    }

    pub fn master(&self, room: &str) -> Option<u64> {
        self.rooms.get(room)?.master
    }

    pub fn receive(&mut self, now: f64, from: SocketAddr, datagram: &[u8]) {
        self.ngo.receive(now, from, datagram);
    }

    pub fn poll_transmit(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.ngo.poll_transmit()
    }

    fn op(&mut self, to: u64, op: &Op) {
        self.ngo.send_named(to, OP_DOWN, &op.encode(), NgoDelivery::ReliableFragmentedSequenced);
    }

    pub fn update(&mut self, now: f64) {
        self.ngo.update(now);
        while let Some(e) = self.ngo.poll_event() {
            match e {
                ServerEvent::Joined { client_id, data } => {
                    if let Some(room) = room_name(&data) {
                        self.pending.insert(client_id, room);
                    }
                }
                ServerEvent::Left { client_id } => self.leave(client_id),
                ServerEvent::Named { from, hash: OP_UP, payload } => match Op::decode(&payload) {
                    Ok(Op::JoinRequest { name, platform_id, .. }) => self.join(now, from, name, platform_id),
                    Ok(Op::Leave) => self.leave(from),
                    _ => {}
                },
                ServerEvent::Named { from, hash: RELAY_UP, payload } => {
                    if let Ok(r) = Raise::decode(&payload) {
                        self.relay(now, from, r);
                    }
                }
                ServerEvent::Named { .. } => {}
            }
        }
    }

    /// JoinResponse with the room as it is, the cached events, SelfJoinMarker; RemotePlayerJoined to the others.
    fn join(&mut self, now: f64, client: u64, name: String, platform_id: String) {
        let Some(room_name) = self.pending.remove(&client) else { return };
        let player = PlayerData { client_id: client, name: Some(name), account_id: None, platform_id: Some(platform_id), properties: None };
        let room = self.rooms.entry(room_name.clone()).or_default();
        room.master.get_or_insert(client);
        room.members.push(player.clone());
        let response = Op::JoinResponse { success: true, room: RoomData { master: room.master, players: Some(room.members.clone()), ..Default::default() } };
        let tick = Server::tick(now);
        let replay: Vec<Vec<u8>> = room.cache.iter().map(|c| Proxied { sender: c.sender, code: c.code, tick, content: c.content.clone() }.encode()).collect();
        let others: Vec<u64> = room.members.iter().map(|p| p.client_id).filter(|&c| c != client).collect();
        self.room_of.insert(client, room_name.clone());
        println!("room {room_name}: client {client} ({}) joined, {} in the room", player.name.as_deref().unwrap_or(""), others.len() + 1);
        self.op(client, &response);
        // the replay goes on the same ordered channel as the marker, so the marker arrives after it
        for p in replay {
            self.ngo.send_named(client, RELAY_DOWN, &p, NgoDelivery::ReliableFragmentedSequenced);
        }
        self.op(client, &Op::SelfJoinMarker);
        for o in others {
            self.op(o, &Op::RemotePlayerJoined(player.clone()));
        }
    }

    /// Passes an event on to its receivers, never back to the sender, and keeps the room cache.
    fn relay(&mut self, now: f64, from: u64, r: Raise) {
        let Some(room) = self.room_of.get(&from).and_then(|n| self.rooms.get_mut(n)) else { return };
        let actor = from as i32;
        // room loads are the master's
        if matches!(r.code, relay::BUFFERED_ROOM_EVENT | relay::CLEAR_BUFFERED_ROOM_EVENT) && room.master != Some(from) {
            return;
        }
        match r.options.cache {
            relay::ADD_TO_ROOM_CACHE | relay::ADD_TO_ROOM_CACHE_GLOBAL => {
                room.cache.push(Cached { sender: actor, code: r.code, content: r.content.clone(), global: r.options.cache == relay::ADD_TO_ROOM_CACHE_GLOBAL });
            }
            relay::REMOVE_FROM_ROOM_CACHE => room.cache.retain(|c| !(c.sender == actor && c.code == r.code)),
            relay::REMOVE_FROM_ROOM_CACHE_FOR_ACTORS_LEFT => {
                let members: Vec<i32> = room.members.iter().map(|p| p.client_id as i32).collect();
                room.cache.retain(|c| c.global || members.contains(&c.sender));
            }
            _ => {}
        }
        let to: Vec<u64> = match (&r.options.targets, r.options.receivers) {
            (Some(t), _) => room.members.iter().map(|p| p.client_id).filter(|&c| t.contains(&(c as i32))).collect(),
            (None, Receivers::Master) => room.master.into_iter().collect(),
            (None, Receivers::Others | Receivers::All) => room.members.iter().map(|p| p.client_id).collect(),
        };
        let p = Proxied { sender: actor, code: r.code, tick: Server::tick(now), content: r.content }.encode();
        for c in to.into_iter().filter(|&c| c != from) {
            self.ngo.send_named(c, RELAY_DOWN, &p, r.delivery.ngo());
        }
    }

    /// Takes a player out of its room: its cached events go (unless global), the master passes to the longest
    /// present, and the others hear RemotePlayerLeft.
    fn leave(&mut self, client: u64) {
        self.pending.remove(&client);
        let Some(name) = self.room_of.remove(&client) else { return };
        let Some(room) = self.rooms.get_mut(&name) else { return };
        if !room.has(client) {
            return;
        }
        room.members.retain(|p| p.client_id != client);
        room.cache.retain(|c| c.global || c.sender != client as i32);
        let mut update = RoomData::default();
        if room.master == Some(client) {
            room.master = room.members.first().map(|p| p.client_id);
            update = RoomData { master: room.master, previous_master: Some(client), ..Default::default() };
        }
        let others: Vec<u64> = room.members.iter().map(|p| p.client_id).collect();
        println!("room {name}: client {client} left, {} in the room", others.len());
        if others.is_empty() {
            self.rooms.remove(&name);
        }
        for o in others {
            self.op(o, &Op::RemotePlayerLeft { left: client, update: update.clone() });
        }
    }
}

/// Binds the room listener and serves it on its own thread; returns the bound address.
pub fn listen(bind: &str) -> std::io::Result<SocketAddr> {
    let socket = UdpSocket::bind(bind)?;
    socket.set_read_timeout(Some(Duration::from_millis(5)))?;
    let addr = socket.local_addr()?;
    std::thread::spawn(move || serve(socket));
    Ok(addr)
}

fn serve(socket: UdpSocket) {
    let start = Instant::now();
    let mut rooms = Rooms::new();
    let mut buf = [0u8; 2048];
    loop {
        // errors include Windows' reset reports for datagrams a closed client did not take: nothing to do
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            rooms.receive(start.elapsed().as_secs_f64(), from, &buf[..n]);
        }
        rooms.update(start.elapsed().as_secs_f64());
        while let Some((to, d)) = rooms.poll_transmit() {
            let _ = socket.send_to(&d, to);
        }
    }
}
