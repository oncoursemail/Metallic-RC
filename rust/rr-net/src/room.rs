//! A player's side of a room, without I/O: connect, join, get a spawn point from the master client, announce the
//! avatar (event 29, cached), send its movement 15 times a second (event 28) and follow everyone else's.
//!
//! Spawning follows the 2026 R1 order (realtime.md 5.6): after the cached-event replay the player asks the master
//! with RPC `RpcMasterRequestPlayerSpawn`; the master answers `RpcSpawnNewPlayer` to that player alone. The master
//! spawns itself. RPCs name their method (key 2): the RpcList index table (key 4) is not known yet.
//!
//! A player's look (our avatar's colours and accessory, opaque text to this layer) goes out as RPC `RpcSetLook`,
//! kept in the room cache so late joiners get it; a change replaces the cached one.
//!
//! Networked objects (see [`crate::objects`]): `instantiate`, `destroy`, `request_ownership`, `send_rpc`; the
//! owner's poses go out with the avatar in event 28, everyone else's come back smoothed from `object_poses`, and
//! what happens to objects comes out of `poll_object_event`, apart from the player events.
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;

use crate::events::ops::{Op, PlayerData, RoomData};
use crate::events::relay::{self, Options, Proxied, Raise, Receivers};
use crate::events::tagged::Value;
use crate::events::{RrDelivery, OP_DOWN, OP_UP, RELAY_DOWN, RELAY_UP};
use crate::ngo::session::TICK_RATE;
use crate::ngo::{Client, ClientEvent, NgoDelivery};
use crate::objects::{self, moved, Object, ObjectEvent, RpcTarget};
use crate::sync::{self, AvatarData, Pose, Smoother};
use crate::voice::{self, VoicePacket};

pub const PLAYER_PREFAB: &str = "Player";
const REQUEST_SPAWN: &str = "RpcMasterRequestPlayerSpawn";
const SPAWN: &str = "RpcSpawnNewPlayer";
/// our own RPC, not the stock client's
const LOOK: &str = "RpcSetLook";
/// metres between spawn places round the master's spawn point
const SPAWN_SPACING: f32 = 1.2;
/// most ownership claims and samples kept for objects not registered yet
const MAX_EARLY: usize = 4096;
/// voice packets held for the game to take (about 1 s per speaker for a few speakers)
const MAX_VOICE: usize = 256;

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
    objects: HashMap<i32, Object>,
    next_sub: i32,
    object_events: VecDeque<ObjectEvent>,
    /// ownership claims and the newest sample for views not registered yet (scene objects of a room still loading)
    early_owner: HashMap<i32, (Option<i32>, i32)>,
    early_pose: HashMap<i32, (f64, i32, Pose)>,
    /// scene objects destroyed before this player registered them
    early_destroyed: HashSet<i32>,
    /// send every owned object at the next beat (someone joined)
    resend: bool,
    voice_in: VecDeque<VoicePacket>,
    voice_sequence: u16,
    /// the other players as last drawn
    shown: Vec<Shown>,
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
            objects: HashMap::new(),
            next_sub: objects::RUNTIME_SUB_OFFSET,
            object_events: VecDeque::new(),
            early_owner: HashMap::new(),
            early_pose: HashMap::new(),
            early_destroyed: HashSet::new(),
            resend: false,
            voice_in: VecDeque::new(),
            voice_sequence: 0,
            shown: vec![],
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
        // drop this player's cached one (filtered by method name), then cache the new one
        let remove = Options { cache: relay::REMOVE_FROM_ROOM_CACHE, ..Default::default() };
        self.raise(relay::RPC, RrDelivery::Reliable, remove, &Value::Dictionary(vec![(Value::Byte(2), Value::String(LOOK.into()))]));
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
        self.rpc_on(now, 0, method, params, options);
    }

    fn rpc_on(&mut self, now: f64, view: i32, method: &str, params: Vec<Value>, options: Options) {
        let content = Value::Dictionary(vec![
            (Value::Byte(0), Value::Int(view)),
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
            let before: Vec<(i32, bool)> = self.objects.keys().map(|&v| (v, self.is_mine(v))).collect();
            self.master = Some(m);
            self.events.push_back(RoomEvent::MasterChanged { actor: m });
            // the master's objects (scene objects, and those whose owner left) go with it
            for (view, was) in before {
                if self.objects[&view].owner.is_none() {
                    self.owner_moved(view, was);
                }
            }
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
                let actor = p.client_id as i32;
                self.names.insert(actor, name.clone());
                self.events.push_back(RoomEvent::PlayerJoined { actor, name });
                // ownership is not cached: the master tells the newcomer; every owner sends its objects again
                if self.is_master() && !self.objects.is_empty() {
                    let all: Vec<_> = self.objects.iter().map(|(&v, o)| (v, o.owner, o.version)).collect();
                    let options = Options { targets: Some(vec![actor]), ..Default::default() };
                    self.raise(objects::OWNERSHIP_UPDATE, RrDelivery::Reliable, options, &objects::owners_content(&all));
                }
                self.resend = true;
            }
            Op::RemotePlayerLeft { left, update } => {
                let actor = left as i32;
                self.names.remove(&actor);
                self.remotes.remove(&actor);
                self.looks.remove(&actor);
                self.events.push_back(RoomEvent::PlayerLeft { actor });
                // its objects stay, and pass to the master (set_master announces them if the master changes too)
                let left: Vec<i32> = self.objects.iter().filter(|(_, o)| o.owner == Some(actor)).map(|(&v, _)| v).collect();
                for v in &left {
                    self.objects.get_mut(v).unwrap().owner = None;
                }
                let master = self.master;
                self.set_master(update.master, now);
                if self.master == master {
                    for v in left {
                        self.owner_moved(v, false);
                    }
                }
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
            relay::NETWORK_INSTANTIATE => self.on_instantiate(p.sender, &content),
            relay::NETWORK_SYNCHRONIZATION => {
                let Value::ByteArray(buf) = content else { return };
                let Ok((tick, _, records)) = sync::read_buffer(&buf) else { return };
                let sent = tick as f64 / TICK_RATE;
                let master = self.master;
                for (view, kind, bytes) in records {
                    if kind == sync::NETWORK_TRANSFORM {
                        let Ok(pose) = sync::decode_transform(bytes) else { continue };
                        match self.objects.get_mut(&view) {
                            // only the owner's samples: a former owner's late ones are dropped
                            Some(o) if o.owner.or(master) == Some(p.sender) && Some(p.sender) != self.actor => o.shown.push(sent, pose),
                            Some(_) => {}
                            None if self.early_pose.len() < MAX_EARLY || self.early_pose.contains_key(&view) => {
                                if self.early_pose.get(&view).is_none_or(|e| e.0 <= sent) {
                                    self.early_pose.insert(view, (sent, p.sender, pose));
                                }
                            }
                            None => {}
                        }
                        continue;
                    }
                    let Some(r) = self.remotes.get_mut(&p.sender) else { continue };
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
                    (REQUEST_SPAWN | SPAWN | LOOK, _) | ("", _) => {}
                    _ => {
                        let view = match content.get(0) {
                            Some(Value::Int(v)) => *v,
                            _ => 0,
                        };
                        self.object_events.push_back(ObjectEvent::Rpc { view, sender: p.sender, method, params });
                    }
                }
            }
            objects::DESTROY => {
                if let Some(Value::Int(view)) = content.get(0) {
                    if self.objects.remove(view).is_some() {
                        self.object_events.push_back(ObjectEvent::Destroyed { view: *view });
                    } else if objects::creator(*view) == 0 && self.early_destroyed.len() < MAX_EARLY {
                        self.early_destroyed.insert(*view);
                    }
                }
            }
            objects::TRANSFER_OWNERSHIP | objects::OWNERSHIP_UPDATE => {
                for (view, owner, version) in objects::read_owners(&content) {
                    self.claim(view, owner, version);
                }
            }
            _ => {}
        }
    }

    /// Another player's object (or, from the cache after a rejoin, an old one of this player's).
    fn on_instantiate(&mut self, sender: i32, content: &Value) {
        let (Some(address), Some(Value::Vector3(position)), Some(Value::Quaternion(rotation)), Some(Value::IntArray(views))) =
            (content.get(0).and_then(Value::as_str), content.get(1), content.get(2), content.get(5)) else { return };
        let Some(&view) = views.first() else { return };
        if self.objects.contains_key(&view) {
            return;
        }
        let scale = match content.get(3) {
            Some(Value::Vector3(s)) => *s,
            _ => [1.0; 3],
        };
        let data = match content.get(6) {
            Some(Value::Array(d)) => d.clone(),
            _ => vec![],
        };
        // the creator owns it, unless it has left already (an object from the cache outlives its creator)
        let creator = objects::creator(view);
        let present = self.names.contains_key(&creator) || Some(creator) == self.actor;
        let owner = (present && creator == sender).then_some(creator);
        let pose = Pose { position: *position, rotation: *rotation, velocity: [0.0; 3] };
        self.add_object(view, address.to_string(), owner, pose);
        let owner = self.owner_of(view).unwrap_or(0);
        self.object_events.push_back(ObjectEvent::Spawned { view, address: address.into(), position: *position, rotation: *rotation, scale, owner, data });
    }

    fn add_object(&mut self, view: i32, address: String, owner: Option<i32>, pose: Pose) {
        let mut o = Object::new(address, owner, pose);
        o.shown.seed(pose);
        if let Some((owner, version)) = self.early_owner.remove(&view) {
            o.claim(owner, version);
        }
        if let Some((sent, sender, p)) = self.early_pose.remove(&view) {
            if o.owner.or(self.master) == Some(sender) {
                // shown where the owner has it, not eased there from the layout's place
                o.shown.seed(p);
                o.shown.push(sent, p);
            }
        }
        self.objects.insert(view, o);
    }

    fn claim(&mut self, view: i32, owner: Option<i32>, version: i32) {
        if !self.objects.contains_key(&view) {
            if self.early_owner.len() < MAX_EARLY && self.early_owner.get(&view).is_none_or(|e| (version, owner.unwrap_or(0)) > (e.1, e.0.unwrap_or(0))) {
                self.early_owner.insert(view, (owner, version));
            }
            return;
        }
        let was = self.is_mine(view);
        if self.objects.get_mut(&view).unwrap().claim(owner, version) {
            self.owner_moved(view, was);
        }
    }

    /// After `view`'s owner changed: hands the pose over between "this player moves it" and "shown from samples",
    /// and tells the game.
    fn owner_moved(&mut self, view: i32, was_mine: bool) {
        let mine = self.is_mine(view);
        let owner = self.owner_of(view).unwrap_or(0);
        let Some(o) = self.objects.get_mut(&view) else { return };
        if mine && !was_mine {
            // carry on from where it is shown, so taking it over does not make it jump
            if let Some(p) = o.shown.current() {
                o.pose = p;
            }
            o.sent = None;
        } else if was_mine && !mine {
            o.shown.seed(o.pose);
        }
        self.object_events.push_back(ObjectEvent::OwnerChanged { view, owner });
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
                ClientEvent::Named { hash: voice::TO_CLIENT, payload } => {
                    if let Ok(v) = VoicePacket::decode(&payload) {
                        if self.voice_in.len() >= MAX_VOICE {
                            self.voice_in.pop_front();
                        }
                        self.voice_in.push_back(v);
                    }
                }
                ClientEvent::Named { .. } => {}
            }
        }
        if self.spawned && now - self.last_sync >= sync::SEND_INTERVAL {
            // keep the 15 Hz beat; after a stall start again rather than send a burst
            self.last_sync = if now - self.last_sync > 2.0 * sync::SEND_INTERVAL { now } else { self.last_sync + sync::SEND_INTERVAL };
            let actor = self.actor.unwrap_or(0);
            let mut records = vec![(view_id(actor), sync::PLAYER_AVATAR_DATA, self.pose.encode())];
            // owned objects while they move, for a moment after they stop, and all of them when someone joined
            let (master, resend) = (self.master, std::mem::take(&mut self.resend));
            for (&view, o) in self.objects.iter_mut() {
                if o.owner.or(master) != Some(actor) {
                    continue;
                }
                if o.sent.is_none_or(|s| moved(&s, &o.pose)) {
                    o.busy_until = now + objects::SETTLE;
                } else if now >= o.busy_until && !resend {
                    continue;
                }
                records.push((view, sync::NETWORK_TRANSFORM, sync::encode_transform(&o.pose)));
                o.sent = Some(o.pose);
            }
            for buf in sync::write_buffers(self.tick(now), actor, &records) {
                self.raise(relay::NETWORK_SYNCHRONIZATION, RrDelivery::Unreliable, Options::default(), &Value::ByteArray(buf));
            }
        }
    }

    /// Instantiates `address` for everyone (event 29, kept in the room cache after this player leaves); returns its
    /// view id, owned by this player. `None` before the room is joined.
    pub fn instantiate(&mut self, now: f64, address: &str, position: [f32; 3], rotation: [f32; 4], scale: [f32; 3], data: Vec<Value>) -> Option<i32> {
        let actor = self.actor?;
        self.master?;
        let view = objects::view_id(actor, self.next_sub);
        self.next_sub = if self.next_sub + 1 >= objects::MAX_VIEW_IDS { objects::RUNTIME_SUB_OFFSET } else { self.next_sub + 1 };
        let mut content = vec![
            (Value::Byte(0), Value::String(address.into())),
            (Value::Byte(1), Value::Vector3(position)),
            (Value::Byte(2), Value::Quaternion(rotation)),
            (Value::Byte(3), Value::Vector3(scale)),
            (Value::Byte(5), Value::IntArray(vec![view])),
            (Value::Byte(7), Value::Int(self.tick(now))),
        ];
        if !data.is_empty() {
            content.push((Value::Byte(6), Value::Array(data)));
        }
        let options = Options { cache: relay::ADD_TO_ROOM_CACHE_GLOBAL, ..Default::default() };
        self.raise(relay::NETWORK_INSTANTIATE, RrDelivery::Reliable, options, &Value::Dictionary(content));
        self.add_object(view, address.into(), Some(actor), Pose { position, rotation, velocity: [0.0; 3] });
        Some(view)
    }

    /// Registers one of the room layout's own networked objects (`objects::scene_view_id`), which every player
    /// loads itself; the master owns it until someone takes it. One destroyed before is not registered.
    pub fn register_scene_object(&mut self, view: i32, address: &str, position: [f32; 3], rotation: [f32; 4]) {
        if !self.objects.contains_key(&view) && !self.early_destroyed.contains(&view) {
            self.add_object(view, address.into(), None, Pose { position, rotation, velocity: [0.0; 3] });
        }
    }

    /// Destroys an object for everyone, and takes it (and the RPCs buffered on it) out of the room cache.
    pub fn destroy(&mut self, view: i32) {
        if self.objects.remove(&view).is_none() {
            return;
        }
        let creator = objects::creator(view);
        let remove = |targets: Vec<i32>| Options { cache: relay::REMOVE_FROM_ROOM_CACHE, targets: Some(targets), ..Default::default() };
        if creator != 0 {
            let filter = Value::Dictionary(vec![(Value::Byte(5), Value::IntArray(vec![view]))]);
            self.raise(relay::NETWORK_INSTANTIATE, RrDelivery::Reliable, remove(vec![creator]), &filter);
        }
        let everyone: Vec<i32> = self.names.keys().copied().chain(self.actor).collect();
        let filter = Value::Dictionary(vec![(Value::Byte(0), Value::Int(view))]);
        self.raise(relay::RPC, RrDelivery::Reliable, remove(everyone), &filter);
        // a scene object stays in every player's layout, so its destruction is kept for late joiners
        let cache = if creator == 0 { relay::ADD_TO_ROOM_CACHE_GLOBAL } else { relay::DO_NOT_CACHE };
        let content = Value::Dictionary(vec![(Value::Byte(0), Value::Int(view))]);
        self.raise(objects::DESTROY, RrDelivery::Reliable, Options { cache, ..Default::default() }, &content);
        self.object_events.push_back(ObjectEvent::Destroyed { view });
    }

    /// The actor that owns `view` (the master for scene objects nobody took).
    pub fn owner_of(&self, view: i32) -> Option<i32> {
        self.objects.get(&view)?.owner.or(self.master)
    }

    pub fn is_mine(&self, view: i32) -> bool {
        self.actor.is_some() && self.owner_of(view) == self.actor
    }

    /// Takes `view` over at once (a grab does not wait for the owner); false if there is no such object.
    pub fn request_ownership(&mut self, view: i32) -> bool {
        let Some(actor) = self.actor else { return false };
        self.transfer_ownership(view, actor)
    }

    /// Gives `view` to `to`; every player settles on the same owner even when two claims cross.
    pub fn transfer_ownership(&mut self, view: i32, to: i32) -> bool {
        let Some(o) = self.objects.get(&view) else { return false };
        if self.owner_of(view) == Some(to) {
            return true;
        }
        let version = o.version + 1;
        self.claim(view, Some(to), version);
        let content = objects::owners_content(&[(view, Some(to), version)]);
        self.raise(objects::TRANSFER_OWNERSHIP, RrDelivery::Reliable, Options::default(), &content);
        true
    }

    /// Where this player has an object it owns now; sent with the next movement beat if it moved.
    pub fn set_object_pose(&mut self, view: i32, pose: Pose) {
        if let Some(o) = self.objects.get_mut(&view) {
            o.pose = pose;
        }
    }

    /// Objects other players own, as to draw them at local time `now`, `dt` seconds after the previous call.
    pub fn object_poses(&mut self, now: f64, dt: f64) -> Vec<(i32, Pose)> {
        let at = self.ngo.server_time(now).unwrap_or(now);
        let (me, master) = (self.actor, self.master);
        self.objects.iter_mut()
            .filter(|(_, o)| me.is_none() || o.owner.or(master) != me)
            .filter_map(|(&v, o)| Some((v, o.shown.step(at, dt)?)))
            .collect()
    }

    /// Calls `method` on `view` (0: the room) for `target`. "All" targets run it here at once too, as an `Rpc`
    /// object event from this player.
    pub fn send_rpc(&mut self, now: f64, view: i32, method: &str, params: Vec<Value>, target: RpcTarget) {
        let (options, local) = target.options();
        if local || target == RpcTarget::Actor(self.actor.unwrap_or(-1)) {
            self.object_events.push_back(ObjectEvent::Rpc { view, sender: self.actor.unwrap_or(0), method: method.into(), params: params.clone() });
        }
        self.rpc_on(now, view, method, params, options);
    }

    pub fn poll_object_event(&mut self) -> Option<ObjectEvent> {
        self.object_events.pop_front()
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
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
        self.shown = out.clone();
        out
    }

    /// The other players as `remotes` last returned them.
    pub fn shown(&self) -> &[Shown] {
        &self.shown
    }

    /// Sends one Opus frame of this player's voice to the room (unreliable, as Dissonance sends voice).
    pub fn send_voice(&mut self, frame: Vec<u8>, amplitude: u8) {
        if self.actor.is_none() {
            return;
        }
        self.voice_sequence = self.voice_sequence.wrapping_add(1);
        let p = VoicePacket { sender: 0, sequence: self.voice_sequence, amplitude, frame };
        self.ngo.send_named(voice::TO_SERVER, &p.encode(), NgoDelivery::Unreliable);
    }

    /// The next voice packet another player sent.
    pub fn poll_voice(&mut self) -> Option<VoicePacket> {
        self.voice_in.pop_front()
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
