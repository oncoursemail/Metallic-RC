//! The room server: a UDP listener beside the HTTP API that does what the 2026 room server does for players in one
//! room instance (realtime.md section 8): join and leave, the master client, and the Photon-style relay of events
//! with its event cache. It never reads event content, only the envelope; gameplay authority is the master client.
//!
//! It runs plain (marker 0x11; encryption comes with the 2026 handshake). A connection names its room instance and
//! carries the access token from login: `{"RI":"<room>","AT":"<token>"}`. With an [`Auth`] the token must belong to
//! an account, whose name is then the player's name; without one (tests, tools) any connection naming a room is let
//! in. Every client is held to rate limits (see [`Limits`]); the room cache has a size limit. [`Live`] is the list of
//! open rooms and their player counts, served over HTTP by [`routes`].
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rr_net::events::ops::{Op, PlayerData, RoomData};
use rr_net::events::relay::{self, Proxied, Raise, Receivers};
use rr_net::events::tagged::Value;
use rr_net::events::{OP_DOWN, OP_UP, RELAY_DOWN, RELAY_UP};
use rr_net::ngo::{NgoDelivery, Server, ServerEvent};
use rr_net::voice::{self, VoicePacket};

/// The approval payload the 2026 client takes as success.
const APPROVED: &str = r#"{"DR":"Carrot"}"#;
const MAX_ROOM_NAME: usize = 128;
/// Cached events a room keeps at most, and at most per sender; beyond that events are still relayed, not kept.
const MAX_CACHE: usize = 4096;
const MAX_CACHE_PER_SENDER: usize = 1024;

/// Who an access token belongs to: account id and display name.
pub type Auth = Arc<dyn Fn(&str) -> Option<(i64, String)> + Send + Sync>;

/// The open rooms, as the HTTP API lists them.
pub type Live = Arc<Mutex<Vec<LiveRoom>>>;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LiveRoom {
    pub room: String,
    pub players: usize,
}

fn field(data: &[u8], key: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(data).ok()?;
    Some(v.get(key)?.as_str()?.to_string())
}

fn room_name(data: &[u8]) -> Option<String> {
    field(data, "RI").filter(|n| !n.is_empty() && n.len() <= MAX_ROOM_NAME)
}

/// A token bucket: `rate` a second, up to `burst`.
#[derive(Clone, Copy)]
struct Bucket {
    level: f64,
    at: f64,
}

impl Default for Bucket {
    fn default() -> Self {
        // full at first use
        Bucket { level: 0.0, at: f64::NEG_INFINITY }
    }
}

impl Bucket {
    fn take(&mut self, now: f64, rate: f64, burst: f64, cost: f64) -> bool {
        self.level = (self.level + (now - self.at) * rate).min(burst);
        self.at = now;
        let ok = self.level >= cost;
        if ok {
            self.level -= cost;
        }
        ok
    }
}

/// What one client may send, per second (burst): events 60 (120), movement 45 (60; 15 Hz plus split buffers),
/// voice 60 (100; 50 frames a second), server operations 10 (20), and 128 KB (256 KB) in all. More is dropped.
#[derive(Default)]
pub struct Limits {
    events: Bucket,
    sync: Bucket,
    voice: Bucket,
    ops: Bucket,
    bytes: Bucket,
    dropped: u64,
    told: f64,
}

impl Limits {
    fn allow(&mut self, now: f64, hash: u16, code: Option<u8>, len: usize) -> bool {
        let kind = match (hash, code) {
            (RELAY_UP, Some(relay::NETWORK_SYNCHRONIZATION)) => self.sync.take(now, 45.0, 60.0, 1.0),
            (RELAY_UP, _) => self.events.take(now, 60.0, 120.0, 1.0),
            (voice::TO_SERVER, _) => self.voice.take(now, 60.0, 100.0, 1.0),
            (rr_net::games::TO_SERVER, _) => self.events.take(now, 60.0, 120.0, 1.0),
            _ => self.ops.take(now, 10.0, 20.0, 1.0),
        };
        let ok = kind && self.bytes.take(now, 128.0 * 1024.0, 256.0 * 1024.0, len as f64);
        if !ok {
            self.dropped += 1;
        }
        ok
    }
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
    /// approved connections that have not sent JoinRequest yet: the room they asked for and their account
    pending: HashMap<u64, (String, Option<(i64, String)>)>,
    room_of: HashMap<u64, String>,
    auth: Option<Auth>,
    limits: HashMap<u64, Limits>,
    live: Live,
    /// built-in game modes (Paintball): the server runs the match
    games: crate::games::Games,
}

impl Default for Rooms {
    fn default() -> Self {
        Self::new()
    }
}

impl Rooms {
    /// Rooms open to any connection that names a room.
    pub fn new() -> Self {
        Self::with_auth(None)
    }

    /// Rooms that, given an `auth`, let in only connections whose token it accepts.
    pub fn with_auth(auth: Option<Auth>) -> Self {
        let check = auth.clone();
        let approve = Box::new(move |data: &[u8]| {
            room_name(data).ok_or("no room instance in the connection data")?;
            if let Some(a) = &check {
                field(data, "AT").and_then(|t| a(&t)).ok_or("not signed in: the room server needs the token from login")?;
            }
            Ok(APPROVED.to_string())
        });
        Rooms { ngo: Server::new(approve), rooms: HashMap::new(), pending: HashMap::new(), room_of: HashMap::new(), auth, limits: HashMap::new(), live: Live::default(), games: Default::default() }
    }

    /// The open rooms and their player counts, kept up to date as players come and go.
    pub fn live(&self) -> Live {
        self.live.clone()
    }

    fn update_live(&self) {
        let mut l: Vec<LiveRoom> = self.rooms.iter().map(|(n, r)| LiveRoom { room: n.clone(), players: r.members.len() }).collect();
        l.sort_by(|a, b| b.players.cmp(&a.players).then_with(|| a.room.cmp(&b.room)));
        *self.live.lock().unwrap() = l;
    }

    /// Messages dropped so far for going over a client's limits.
    pub fn dropped(&self, client: u64) -> u64 {
        self.limits.get(&client).map_or(0, |l| l.dropped)
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
            if let ServerEvent::Named { from, hash, payload } = &e {
                let l = self.limits.entry(*from).or_default();
                if !l.allow(now, *hash, payload.first().copied(), payload.len()) {
                    if now - l.told > 5.0 {
                        l.told = now;
                        println!("room: client {from} is over its rate limits ({} messages dropped)", l.dropped);
                    }
                    continue;
                }
            }
            match e {
                ServerEvent::Joined { client_id, data } => {
                    if let Some(room) = room_name(&data) {
                        let account = self.auth.as_ref().and_then(|a| field(&data, "AT").and_then(|t| a(&t)));
                        self.pending.insert(client_id, (room, account));
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
                ServerEvent::Named { from, hash: voice::TO_SERVER, mut payload } => {
                    if VoicePacket::stamp(&mut payload, from as u16) {
                        self.voice(from, &payload);
                    }
                }
                ServerEvent::Named { from, hash: rr_net::games::TO_SERVER, payload } => {
                    if let Some(room) = self.room_of.get(&from) {
                        self.games.message(now, room, from, &payload);
                    }
                }
                ServerEvent::Named { .. } => {}
            }
        }
        for s in self.games.update(now) {
            self.ngo.send_named(s.to, rr_net::games::TO_CLIENT, &s.payload, s.delivery);
        }
    }

    /// The game modes, for tests.
    pub fn games(&self) -> &crate::games::Games {
        &self.games
    }

    /// Passes a voice packet (stamped with the speaker) to the rest of the speaker's room.
    fn voice(&mut self, from: u64, payload: &[u8]) {
        let Some(room) = self.room_of.get(&from).and_then(|n| self.rooms.get(n)) else { return };
        let to: Vec<u64> = room.members.iter().map(|p| p.client_id).filter(|&c| c != from).collect();
        for c in to {
            self.ngo.send_named(c, voice::TO_CLIENT, payload, NgoDelivery::Unreliable);
        }
    }

    /// JoinResponse with the room as it is, the cached events, SelfJoinMarker; RemotePlayerJoined to the others.
    fn join(&mut self, now: f64, client: u64, name: String, platform_id: String) {
        let Some((room_name, account)) = self.pending.remove(&client) else { return };
        // a signed-in player is called by its account's name, not by what it says
        let (name, account_id) = match account {
            Some((id, n)) => (n, i32::try_from(id).ok()),
            None => (name, None),
        };
        let player = PlayerData { client_id: client, name: Some(name), account_id, platform_id: Some(platform_id), properties: None };
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
        self.games.joined(now, &room_name, client, player.name.as_deref().unwrap_or(""));
        for o in others {
            self.op(o, &Op::RemotePlayerJoined(player.clone()));
        }
        self.update_live();
    }

    /// Passes an event on to its receivers, never back to the sender, and keeps the room cache.
    fn relay(&mut self, now: f64, from: u64, r: Raise) {
        if r.code == relay::NETWORK_SYNCHRONIZATION {
            if let Some(room) = self.room_of.get(&from) {
                self.games.sync(room, from, &r.content);
            }
        }
        let Some(room) = self.room_of.get(&from).and_then(|n| self.rooms.get_mut(n)) else { return };
        let actor = from as i32;
        // room loads are the master's
        if matches!(r.code, relay::BUFFERED_ROOM_EVENT | relay::CLEAR_BUFFERED_ROOM_EVENT) && room.master != Some(from) {
            return;
        }
        match r.options.cache {
            relay::ADD_TO_ROOM_CACHE | relay::ADD_TO_ROOM_CACHE_GLOBAL
                if room.cache.len() >= MAX_CACHE || room.cache.iter().filter(|c| c.sender == actor).count() >= MAX_CACHE_PER_SENDER => {}
            relay::ADD_TO_ROOM_CACHE | relay::ADD_TO_ROOM_CACHE_GLOBAL => {
                room.cache.push(Cached { sender: actor, code: r.code, content: r.content.clone(), global: r.options.cache == relay::ADD_TO_ROOM_CACHE_GLOBAL });
            }
            relay::REMOVE_FROM_ROOM_CACHE => {
                // Photon's rule: events of this code from the senders in the target list (else from the requester)
                // whose content holds every entry of the filter dictionary sent; a cache operation, relayed to nobody
                let senders = r.options.targets.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| vec![actor]);
                let filter = match Value::decode(&r.content) {
                    Ok(Value::Dictionary(f)) => f,
                    _ => vec![],
                };
                room.cache.retain(|c| !(c.code == r.code && senders.contains(&c.sender) && matches(&c.content, &filter)));
                return;
            }
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
        self.limits.remove(&client);
        let Some(name) = self.room_of.remove(&client) else { return };
        self.games.left(&name, client);
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
        self.update_live();
    }
}

/// Whether tagged `content` is a dictionary holding every entry of `filter` (an empty filter matches anything).
fn matches(content: &[u8], filter: &[(Value, Value)]) -> bool {
    if filter.is_empty() {
        return true;
    }
    let Ok(Value::Dictionary(entries)) = Value::decode(content) else { return false };
    filter.iter().all(|f| entries.contains(f))
}

/// Binds an open room listener (no token checks) and serves it on its own thread; returns the bound address.
pub fn listen(bind: &str) -> std::io::Result<SocketAddr> {
    Ok(start(bind, None)?.0)
}

/// Binds the room listener, checking tokens with `auth` if given, and serves it on its own thread; returns the bound
/// address and the live room list.
pub fn start(bind: &str, auth: Option<Auth>) -> std::io::Result<(SocketAddr, Live)> {
    let socket = UdpSocket::bind(bind)?;
    socket.set_read_timeout(Some(Duration::from_millis(5)))?;
    let addr = socket.local_addr()?;
    let rooms = Rooms::with_auth(auth);
    let live = rooms.live();
    std::thread::spawn(move || serve(socket, rooms));
    Ok((addr, live))
}

/// `GET /realtime/rooms` (signed in): the open rooms, fullest first, as `[{"room":..,"players":..}]`.
pub fn routes(app: crate::Shared, live: Live) -> axum::Router {
    use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, routing::get, Json};
    let list = move |State(app): State<crate::Shared>, headers: HeaderMap| {
        let live = live.clone();
        async move {
            if crate::auth::caller(&app, &headers).is_none() {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            let rooms = live.lock().unwrap().clone();
            Json(rooms).into_response()
        }
    };
    axum::Router::new().route("/realtime/rooms", get(list)).with_state(app)
}

fn serve(socket: UdpSocket, mut rooms: Rooms) {
    let start = Instant::now();
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
