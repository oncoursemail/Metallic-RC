//! A player's side of a room, without I/O: connect, join, get a spawn point from the master client, announce the
//! avatar (event 29, cached), send its movement 15 times a second (event 28) and follow everyone else's.
//!
//! Spawning follows the 2026 R1 order (realtime.md 5.6): after the cached-event replay the player asks the master
//! with RPC `RpcMasterRequestPlayerSpawn`; the master answers `RpcSpawnNewPlayer` to that player alone. The master
//! spawns itself. RPCs name their method (key 2): the RpcList index table (key 4) is not known yet.
//!
//! A player's look (our avatar's colours and accessory, opaque text to this layer) goes out as RPC `RpcSetLook`,
//! kept in the room cache so late joiners get it; a change replaces the cached one.
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

use crate::events::ops::{Op, PlayerData, RoomData};
use crate::events::relay::{self, Options, Proxied, Raise, Receivers};
use crate::events::tagged::Value;
use crate::events::{RrDelivery, OP_DOWN, OP_UP, RELAY_DOWN, RELAY_UP};
use crate::ngo::session::TICK_RATE;
use crate::ngo::{Client, ClientEvent, NgoDelivery};
use crate::sync::{self, AvatarData, Pose, Smoother};

pub const PLAYER_PREFAB: &str = "Player";
const REQUEST_SPAWN: &str = "RpcMasterRequestPlayerSpawn";
const SPAWN: &str = "RpcSpawnNewPlayer";
/// our own RPC, not the stock client's
const LOOK: &str = "RpcSetLook";
/// metres between spawn places round the master's spawn point
const SPAWN_SPACING: f32 = 1.2;

/// The player avatar's view id: actor * 100000 plus a sub id (the PUN scheme).
pub fn view_id(actor: i32) -> i32 {
    actor * 100_000 + 1
}

/// The connection data a room client sends: which room instance it joins.
pub fn connection_data(room: &str) -> Vec<u8> {
    let escaped: String = room.chars().flat_map(|c| match c {
        '"' | '\\' => vec!['\\', c],
        c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32).chars().collect(),
        c => vec![c],
    }).collect();
    format!(r#"{{"RI":"{escaped}"}}"#).into_bytes()
}

#[derive(Clone, Debug, PartialEq)]
pub enum RoomEvent {
    Joined { actor: i32, master: i32 },
    /// where the master put this player (feet position, rotation)
    Spawned { position: [f32; 3], rotation: [f32; 4] },
    PlayerJoined { actor: i32, name: String },
    PlayerLeft { actor: i32 },
    MasterChanged { actor: i32 },
    Refused(String),
    Disconnected,
}

/// Another player as drawn this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub actor: i32,
    pub name: String,
    /// tracking space: the feet, turned with the body
    pub body: Pose,
    pub head: Option<Pose>,
    pub left_hand: Option<Pose>,
    pub right_hand: Option<Pose>,
    /// the look the player last announced, if any
    pub look: Option<String>,
}

struct Remote {
    view: i32,
    body: Smoother,
    head: Smoother,
    left: Smoother,
    right: Smoother,
}

pub struct RoomClient {
    ngo: Client,
    name: String,
    spawn: [f32; 3],
    spawn_yaw: f32,
    actor: Option<i32>,
    master: Option<i32>,
    names: HashMap<i32, String>,
    remotes: HashMap<i32, Remote>,
    looks: HashMap<i32, String>,
    look: Option<String>,
    awaiting_spawn: bool,
    spawned: bool,
    closed: bool,
    pose: AvatarData,
    last_sync: f64,
    next_request: u8,
    events: VecDeque<RoomEvent>,
}

impl RoomClient {
    /// Starts connecting to the room server at `server` to join `room` as `name`. `spawn` is the feet position this
    /// player hands out when it is the master.
    pub fn connect(now: f64, server: SocketAddr, room: &str, name: &str, spawn: [f32; 3]) -> Self {
        RoomClient {
            ngo: Client::connect(now, server, connection_data(room)),
            name: name.into(),
            spawn,
            spawn_yaw: 0.0,
            actor: None,
            master: None,
            names: HashMap::new(),
            remotes: HashMap::new(),
            looks: HashMap::new(),
            look: None,
            awaiting_spawn: false,
            spawned: false,
            closed: false,
            pose: AvatarData::default(),
            last_sync: 0.0,
            next_request: 0,
            events: VecDeque::new(),
        }
    }

    /// The way the master faces when it spawns itself (radians about +y; 0 looks along -z).
    pub fn facing(mut self, yaw: f32) -> Self {
        self.spawn_yaw = yaw;
        self
    }

    pub fn actor(&self) -> Option<i32> {
        self.actor
    }
    pub fn master(&self) -> Option<i32> {
        self.master
    }
    pub fn is_master(&self) -> bool {
        self.actor.is_some() && self.actor == self.master
    }
    pub fn is_spawned(&self) -> bool {
        self.spawned
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn players(&self) -> usize {
        self.names.len() + self.actor.is_some() as usize
    }
    pub fn rtt(&self) -> f64 {
        self.ngo.rtt()
    }

    /// The local avatar's pose; sent with the next movement event.
    pub fn set_pose(&mut self, pose: AvatarData) {
        self.pose = pose;
    }

    /// This player's look; announced at spawn, or now (replacing the cached one) when already spawned.
    pub fn set_look(&mut self, now: f64, look: String) {
        if self.look.as_ref() == Some(&look) {
            return;
        }
        self.look = Some(look);
        if self.spawned {
            self.send_look(now);
        }
    }

    fn send_look(&mut self, now: f64) {
        let Some(look) = self.look.clone() else { return };
        // drop the cached one (sent to nobody), then cache the new one
        let remove = Options { cache: relay::REMOVE_FROM_ROOM_CACHE, targets: Some(vec![]), ..Default::default() };
        self.rpc(now, LOOK, vec![], remove);
        self.rpc(now, LOOK, vec![Value::String(look)], Options { cache: relay::ADD_TO_ROOM_CACHE, ..Default::default() });
    }

    pub fn receive(&mut self, now: f64, datagram: &[u8]) {
        self.ngo.receive(now, datagram);
    }

    pub fn poll_transmit(&mut self) -> Option<Vec<u8>> {
        self.ngo.poll_transmit()
    }

    pub fn poll_event(&mut self) -> Option<RoomEvent> {
        self.events.pop_front()
    }

    /// Leaves the room and closes the connection.
    pub fn leave(&mut self, now: f64) {
        self.op(&Op::Leave);
        self.ngo.disconnect(now);
    }

    fn tick(&self, now: f64) -> i32 {
        (self.ngo.server_time(now).unwrap_or(0.0) * TICK_RATE) as i32
    }

    fn op(&mut self, op: &Op) {
        self.ngo.send_named(OP_UP, &op.encode(), NgoDelivery::ReliableFragmentedSequenced);
    }

    fn raise(&mut self, code: u8, delivery: RrDelivery, options: Options, content: &Value) {
        let r = Raise { code, delivery, options, content: content.encode() };
        self.ngo.send_named(RELAY_UP, &r.encode(), delivery.ngo());
    }

    fn rpc(&mut self, now: f64, method: &str, params: Vec<Value>, options: Options) {
        let content = Value::Dictionary(vec![
            (Value::Byte(0), Value::Int(0)),
            (Value::Byte(1), Value::Int(self.tick(now))),
            (Value::Byte(2), Value::String(method.into())),
            (Value::Byte(3), Value::Array(params)),
        ]);
        self.raise(relay::RPC, RrDelivery::Reliable, options, &content);
    }

    /// Where the master puts an actor: its own spawn point, the others on a ring round it, facing the middle.
    fn place(&self, actor: i32) -> ([f32; 3], [f32; 4]) {
        if actor <= 1 {
            return (self.spawn, [0.0, (self.spawn_yaw / 2.0).sin(), 0.0, (self.spawn_yaw / 2.0).cos()]);
        }
        let a = (actor - 2) as f32 * std::f32::consts::TAU / 6.0;
        let (s, c) = a.sin_cos();
        let p = [self.spawn[0] + s * SPAWN_SPACING, self.spawn[1], self.spawn[2] + c * SPAWN_SPACING];
        // facing -z is forward at yaw 0; turn to look back toward the spawn point
        let yaw = (s).atan2(c);
        ([p[0], p[1], p[2]], [0.0, (yaw / 2.0).sin(), 0.0, (yaw / 2.0).cos()])
    }

    fn request_spawn(&mut self, now: f64) {
        self.awaiting_spawn = true;
        if self.is_master() {
            let (p, r) = self.place(self.actor.unwrap());
            self.spawn_self(now, p, r);
        } else {
            self.next_request = self.next_request.wrapping_add(1);
            let req = self.next_request;
            self.rpc(now, REQUEST_SPAWN, vec![Value::Byte(req)], Options { receivers: Receivers::Master, ..Default::default() });
        }
    }

    fn spawn_self(&mut self, now: f64, position: [f32; 3], rotation: [f32; 4]) {
        let Some(actor) = self.actor else { return };
        if self.spawned {
            return;
        }
        self.awaiting_spawn = false;
        self.spawned = true;
        self.pose = AvatarData { tracking_space: Some(Pose { position, rotation, velocity: [0.0; 3] }), ..Default::default() };
        let content = Value::Dictionary(vec![
            (Value::Byte(0), Value::String(PLAYER_PREFAB.into())),
            (Value::Byte(1), Value::Vector3(position)),
            (Value::Byte(2), Value::Quaternion(rotation)),
            (Value::Byte(5), Value::IntArray(vec![view_id(actor)])),
            (Value::Byte(7), Value::Int(self.tick(now))),
        ]);
        let options = Options { cache: relay::ADD_TO_ROOM_CACHE, ..Default::default() };
        self.raise(relay::NETWORK_INSTANTIATE, RrDelivery::Reliable, options, &content);
        self.events.push_back(RoomEvent::Spawned { position, rotation });
        self.send_look(now);
    }

    fn set_master(&mut self, master: Option<u64>, now: f64) {
        let Some(m) = master.map(|m| m as i32) else { return };
        if self.master != Some(m) {
            self.master = Some(m);
            self.events.push_back(RoomEvent::MasterChanged { actor: m });
            // a spawn request to a master that left is answered by nobody: ask the new one
            if self.awaiting_spawn {
                self.request_spawn(now);
            }
        }
    }

    fn on_op(&mut self, now: f64, op: Op) {
        match op {
            Op::JoinResponse { success: false, .. } => self.events.push_back(RoomEvent::Refused("join refused".into())),
            Op::JoinResponse { success: true, room } => {
                self.master = room.master.map(|m| m as i32);
                for p in room.players.unwrap_or_default() {
                    if Some(p.client_id as i32) != self.actor {
                        self.names.insert(p.client_id as i32, p.name.unwrap_or_default());
                    }
                }
                if let (Some(actor), Some(master)) = (self.actor, self.master) {
                    self.events.push_back(RoomEvent::Joined { actor, master });
                }
            }
            Op::SelfJoinMarker => self.request_spawn(now),
            Op::RemotePlayerJoined(p) => {
                let name = p.name.unwrap_or_default();
                self.names.insert(p.client_id as i32, name.clone());
                self.events.push_back(RoomEvent::PlayerJoined { actor: p.client_id as i32, name });
            }
            Op::RemotePlayerLeft { left, update } => {
                let actor = left as i32;
                self.names.remove(&actor);
                self.remotes.remove(&actor);
                self.looks.remove(&actor);
                self.events.push_back(RoomEvent::PlayerLeft { actor });
                self.set_master(update.master, now);
            }
            Op::RoomUpdate(RoomData { master, .. }) => self.set_master(master, now),
            Op::PlayerUpdate(PlayerData { client_id, name: Some(name), .. }) => {
                self.names.insert(client_id as i32, name);
            }
            _ => {}
        }
    }

    fn on_event(&mut self, now: f64, p: Proxied) {
        let Ok(content) = Value::decode(&p.content) else { return };
        match p.code {
            relay::NETWORK_INSTANTIATE if content.get(0).and_then(Value::as_str) == Some(PLAYER_PREFAB) => {
                let (Some(Value::Vector3(position)), Some(Value::Quaternion(rotation)), Some(Value::IntArray(views))) =
                    (content.get(1), content.get(2), content.get(5)) else { return };
                let Some(&view) = views.first() else { return };
                let sent = match content.get(7) {
                    Some(Value::Int(t)) => *t as f64 / TICK_RATE,
                    _ => p.tick as f64 / TICK_RATE,
                };
                let mut r = Remote { view, body: Smoother::default(), head: Smoother::default(), left: Smoother::default(), right: Smoother::default() };
                r.body.push(sent, Pose { position: *position, rotation: *rotation, velocity: [0.0; 3] });
                self.remotes.insert(p.sender, r);
            }
            relay::NETWORK_SYNCHRONIZATION => {
                let Value::ByteArray(buf) = content else { return };
                let Ok((tick, _, records)) = sync::read_buffer(&buf) else { return };
                let Some(r) = self.remotes.get_mut(&p.sender) else { return };
                let sent = tick as f64 / TICK_RATE;
                for (view, kind, bytes) in records {
                    if view != r.view || kind != sync::PLAYER_AVATAR_DATA {
                        continue;
                    }
                    if let Ok(a) = AvatarData::decode(bytes) {
                        if let Some(b) = a.tracking_space {
                            r.body.push(sent, b);
                        }
                        for (s, p) in [(&mut r.head, a.head), (&mut r.left, a.left_hand), (&mut r.right, a.right_hand)] {
                            if let Some(p) = p {
                                s.push(sent, p);
                            }
                        }
                    }
                }
            }
            relay::RPC => {
                let method = content.get(2).and_then(Value::as_str).unwrap_or_default().to_string();
                let params = match content.get(3) {
                    Some(Value::Array(v)) => v.clone(),
                    _ => vec![],
                };
                match (method.as_str(), params.as_slice()) {
                    (REQUEST_SPAWN, [Value::Byte(req), ..]) if self.is_master() => {
                        let (pos, rot) = self.place(p.sender);
                        let params = vec![Value::Byte(*req), Value::Vector3(pos), Value::Quaternion(rot), Value::Null];
                        self.rpc(now, SPAWN, params, Options { targets: Some(vec![p.sender]), ..Default::default() });
                    }
                    (SPAWN, [_, Value::Vector3(pos), Value::Quaternion(rot), ..]) => self.spawn_self(now, *pos, *rot),
                    (LOOK, [Value::String(look)]) => {
                        self.looks.insert(p.sender, look.clone());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn update(&mut self, now: f64) {
        self.ngo.update(now);
        while let Some(e) = self.ngo.poll_event() {
            match e {
                ClientEvent::Approved { client_id, .. } => {
                    self.actor = Some(client_id as i32);
                    let name = self.name.clone();
                    self.op(&Op::JoinRequest { name, platform_id: String::new(), properties: Value::Null });
                }
                ClientEvent::Refused(r) => self.events.push_back(RoomEvent::Refused(r)),
                ClientEvent::Disconnected(_) => {
                    self.closed = true;
                    self.events.push_back(RoomEvent::Disconnected);
                }
                ClientEvent::Named { hash: OP_DOWN, payload } => {
                    if let Ok(op) = Op::decode(&payload) {
                        self.on_op(now, op);
                    }
                }
                ClientEvent::Named { hash: RELAY_DOWN, payload } => {
                    if let Ok(p) = Proxied::decode(&payload) {
                        self.on_event(now, p);
                    }
                }
                ClientEvent::Named { .. } => {}
            }
        }
        if self.spawned && now - self.last_sync >= sync::SEND_INTERVAL {
            // keep the 15 Hz beat; after a stall start again rather than send a burst
            self.last_sync = if now - self.last_sync > 2.0 * sync::SEND_INTERVAL { now } else { self.last_sync + sync::SEND_INTERVAL };
            let actor = self.actor.unwrap_or(0);
            let buf = sync::write_buffer(self.tick(now), actor, &[(view_id(actor), sync::PLAYER_AVATAR_DATA, self.pose.encode())]);
            self.raise(relay::NETWORK_SYNCHRONIZATION, RrDelivery::Unreliable, Options::default(), &Value::ByteArray(buf));
        }
    }

    /// The other players' avatars to draw at local time `now`, `dt` seconds after the previous call.
    pub fn remotes(&mut self, now: f64, dt: f64) -> Vec<Shown> {
        let at = self.ngo.server_time(now).unwrap_or(now);
        let mut out: Vec<Shown> = self.remotes.iter_mut().filter_map(|(&actor, r)| {
            Some(Shown {
                actor,
                name: self.names.get(&actor).cloned().unwrap_or_default(),
                body: r.body.step(at, dt)?,
                head: r.head.step(at, dt),
                left_hand: r.left.step(at, dt),
                right_hand: r.right.step(at, dt),
                look: self.looks.get(&actor).cloned(),
            })
        }).collect();
        out.sort_by_key(|s| s.actor);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_data_is_json() {
        assert_eq!(connection_data("dorm-7"), br#"{"RI":"dorm-7"}"#.to_vec());
        assert_eq!(connection_data("a\"b\\c\n"), br#"{"RI":"a\"b\\c\u000a"}"#.to_vec());
    }
}
