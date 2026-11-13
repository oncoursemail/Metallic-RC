//! Networked objects in the game: the Bevy side of `rr_net::objects`.
//!
//! For other systems:
//! - [`NetObject`] is on every networked entity: its view id, address, owner, and `mine` (this player moves it).
//! - [`NetObjects`] (a system parameter): `net_spawn(address, transform)` instantiates for everyone,
//!   `request_ownership(entity)` takes an object over at once (call it on grab), `send_rpc(entity, name, bytes,
//!   target)`, `destroy(entity)`, and `register_scene(entity, index, address)` for the room layout's own props.
//! - Incoming RPCs arrive as [`NetRpc`] messages.
//! - Sync: the Transform of an object this player owns is sent (moving it is all a grab or throw has to do); the
//!   others' objects get their Transform from the owner, smoothed. When someone else takes an object, `mine` turns
//!   false: whoever holds it should let go.
//!
//! `spawn_prop` here builds the game's prefab through `world::spawn_prop`; `debug/` and `test/` addresses get a box.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use rr_net::events::tagged::Value;
use rr_net::objects::{scene_view_id, ObjectEvent, RpcTarget};
use rr_net::sync::Pose;

use crate::realtime::Realtime;
use crate::world;

pub struct NetObjectsPlugin;

impl Plugin for NetObjectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomLink>()
            .init_resource::<Views>()
            .add_message::<NetRpc>()
            .add_systems(Update, (object_events, debug_props.after(object_events)))
            .add_systems(PostUpdate, sync_objects.before(TransformSystems::Propagate));
    }
}

/// The room connection the game made at login (main's `Net`); empty while offline.
#[derive(Resource, Clone, Default)]
pub struct RoomLink(pub Arc<Mutex<Option<Realtime>>>);

impl RoomLink {
    pub fn get(&self) -> Option<Realtime> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct NetObject {
    pub view: i32,
    pub address: String,
    /// actor number of the owner
    pub owner: i32,
    /// this player owns it: its Transform is what everyone sees
    pub mine: bool,
}

/// An RPC on a networked object (params as sent by `send_rpc`: one byte array).
#[derive(Message, Clone, Debug)]
pub struct NetRpc {
    pub entity: Option<Entity>,
    pub view: i32,
    pub sender: i32,
    pub method: String,
    pub bytes: Vec<u8>,
}

/// Entities by view id, and which room connection they belong to.
#[derive(Resource, Default)]
pub struct Views {
    by_view: HashMap<i32, Entity>,
    room: usize,
}

impl Views {
    pub fn entity(&self, view: i32) -> Option<Entity> {
        self.by_view.get(&view).copied()
    }
}

/// A networked object with no prefab behind it (test and debug addresses, or no game files): a 0.3 m box.
#[derive(Component)]
pub struct Prop;

/// The object for `address`: the prefab from the game files through the world loader, or a 0.3 m box tinted by its
/// address for `debug/` and `test/` addresses and when there is no loader.
pub fn spawn_prop(commands: &mut Commands, loader: Option<&world::Loader>, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, address: &str, at: Transform) -> Entity {
    // maker objects (`maker::kit`): a prefab address with "#m<uid>" after it, or a shape (the maker module draws it)
    let base = address.split("#m").next().unwrap_or(address);
    if let Some(loader) = loader.filter(|_| !["debug/", "test/", "maker/"].iter().any(|p| base.starts_with(p))) {
        return world::spawn_prop(commands, loader, base, at);
    }
    let hue = address.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    let material = materials.add(StandardMaterial { base_color: Color::hsl(hue as f32, 0.6, 0.55), ..default() });
    commands.spawn((Prop, Mesh3d(meshes.add(Cuboid::new(0.3, 0.3, 0.3))), MeshMaterial3d(material), at, Name::new(format!("prop {address}")))).id()
}

#[derive(SystemParam)]
pub struct NetObjects<'w, 's> {
    commands: Commands<'w, 's>,
    link: Res<'w, RoomLink>,
    views: ResMut<'w, Views>,
    objects: Query<'w, 's, &'static NetObject>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    loader: Option<Res<'w, world::Loader>>,
}

impl NetObjects<'_, '_> {
    /// Instantiates `address` at `at` for everyone in the room; `None` while not in a room.
    pub fn net_spawn(&mut self, address: &str, at: Transform) -> Option<Entity> {
        let rt = self.link.get()?;
        let view = rt.room.lock().unwrap().instantiate(rt.now(), address, at.translation.into(), at.rotation.to_array(), at.scale.into(), vec![])?;
        let e = spawn_prop(&mut self.commands, self.loader.as_deref(), &mut self.meshes, &mut self.materials, address, at);
        let actor = rt.room.lock().unwrap().actor().unwrap_or(0);
        self.commands.entity(e).insert(NetObject { view, address: address.into(), owner: actor, mine: true });
        self.views.by_view.insert(view, e);
        Some(e)
    }

    /// Makes one of the room layout's props networked: `index` is its place in the layout, the same for every
    /// player. The master owns it until someone takes it.
    pub fn register_scene(&mut self, entity: Entity, index: u32, address: &str, at: Transform) {
        let Some(rt) = self.link.get() else { return };
        let view = scene_view_id(index);
        let mut room = rt.room.lock().unwrap();
        room.register_scene_object(view, address, at.translation.into(), at.rotation.to_array());
        let (owner, mine) = (room.owner_of(view).unwrap_or(0), room.is_mine(view));
        self.commands.entity(entity).insert(NetObject { view, address: address.into(), owner, mine });
        self.views.by_view.insert(view, entity);
    }

    /// Takes the object over at once; false if it is not a networked object of this room.
    pub fn request_ownership(&mut self, entity: Entity) -> bool {
        let (Some(rt), Ok(o)) = (self.link.get(), self.objects.get(entity)) else { return false };
        let taken = rt.room.lock().unwrap().request_ownership(o.view);
        taken
    }

    pub fn send_rpc(&mut self, entity: Entity, name: &str, bytes: Vec<u8>, target: RpcTarget) {
        let (Some(rt), Ok(o)) = (self.link.get(), self.objects.get(entity)) else { return };
        rt.room.lock().unwrap().send_rpc(rt.now(), o.view, name, vec![Value::ByteArray(bytes)], target);
    }

    pub fn destroy(&mut self, entity: Entity) {
        let (Some(rt), Ok(o)) = (self.link.get(), self.objects.get(entity)) else { return };
        rt.room.lock().unwrap().destroy(o.view);
        self.views.by_view.remove(&o.view);
        self.commands.entity(entity).despawn();
    }
}

/// The room's object news: others' new objects appear, destroyed ones go, owners change, RPCs become messages.
/// A new room connection (another login or room) starts with none.
fn object_events(
    mut commands: Commands,
    link: Res<RoomLink>,
    mut views: ResMut<Views>,
    mut objects: Query<&mut NetObject>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    loader: Option<Res<world::Loader>>,
    mut rpcs: MessageWriter<NetRpc>,
) {
    let rt = link.get();
    let room_id = rt.as_ref().map_or(0, |rt| Arc::as_ptr(&rt.room) as usize);
    if room_id != views.room {
        for (_, e) in views.by_view.drain() {
            commands.entity(e).despawn();
        }
        views.room = room_id;
    }
    let Some(rt) = rt else { return };
    let mut room = rt.room.lock().unwrap();
    while let Some(e) = room.poll_object_event() {
        match e {
            ObjectEvent::Spawned { view, address, position, rotation, scale, owner, .. } => {
                if views.by_view.contains_key(&view) {
                    continue;
                }
                let at = Transform { translation: position.into(), rotation: Quat::from_array(rotation), scale: scale.into() };
                let e = spawn_prop(&mut commands, loader.as_deref(), &mut meshes, &mut materials, &address, at);
                let mine = room.is_mine(view);
                commands.entity(e).insert(NetObject { view, address, owner, mine });
                views.by_view.insert(view, e);
            }
            ObjectEvent::Destroyed { view } => {
                if let Some(e) = views.by_view.remove(&view) {
                    commands.entity(e).despawn();
                }
            }
            ObjectEvent::OwnerChanged { view, owner } => {
                if let Some(mut o) = views.entity(view).and_then(|e| objects.get_mut(e).ok()) {
                    (o.owner, o.mine) = (owner, room.is_mine(view));
                }
            }
            ObjectEvent::Rpc { view, sender, method, params } => {
                let bytes = match params.first() {
                    Some(Value::ByteArray(b)) => b.clone(),
                    _ => vec![],
                };
                rpcs.write(NetRpc { entity: views.entity(view), view, sender, method, bytes });
            }
        }
    }
}

/// Sends the Transforms of the objects this player owns and moves the others' to where their owners have them.
fn sync_objects(time: Res<Time>, link: Res<RoomLink>, mut objects: Query<(Entity, &mut NetObject, &mut Transform)>, mut last: Local<HashMap<Entity, Vec3>>) {
    let Some(rt) = link.get() else { return };
    let dt = time.delta_secs_f64();
    let mut room = rt.room.lock().unwrap();
    let shown: HashMap<i32, Pose> = room.object_poses(rt.now(), dt).into_iter().collect();
    let mut seen = HashMap::new();
    for (e, mut o, mut t) in &mut objects {
        let mine = room.is_mine(o.view);
        if o.mine != mine {
            o.mine = mine;
            o.owner = room.owner_of(o.view).unwrap_or(0);
        }
        if mine {
            let v = last.get(&e).map_or(Vec3::ZERO, |l| (t.translation - *l) / dt.max(1e-3) as f32);
            room.set_object_pose(o.view, Pose { position: t.translation.into(), rotation: t.rotation.to_array(), velocity: v.into() });
            seen.insert(e, t.translation);
        } else if let Some(p) = shown.get(&o.view) {
            t.translation = Vec3::from(p.position);
            t.rotation = Quat::from_array(p.rotation);
        }
    }
    *last = seen;
}

/// Debugging without the player lane's grab, by `METALLIC_NET_DEBUG`: `1` gives keys (P drops a networked box in
/// front of the view, G picks up the nearest networked object within 3 m, taking it over, or lets go); `demo` runs
/// a script for two windows: the master drops a box, anyone else takes the nearest object and carries it round.
#[allow(clippy::too_many_arguments)]
fn debug_props(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut net: NetObjects,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut held: Local<Option<(Entity, Option<Vec3>)>>,
    mut positions: Query<(Entity, &NetObject, &mut Transform)>,
    mut mode: Local<Option<String>>,
    mut joined: Local<Option<f32>>,
) {
    let mode = mode.get_or_insert_with(|| std::env::var("METALLIC_NET_DEBUG").unwrap_or_default()).as_str();
    let (Ok(eye), Some(rt)) = (camera.single(), net.link.get()) else { return };
    if mode != "1" && mode != "demo" {
        return;
    }
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    if actor.is_none() {
        return;
    }
    let since = time.elapsed_secs() - *joined.get_or_insert(time.elapsed_secs());
    let front = eye.translation() + eye.forward() * 1.5;
    let demo = mode == "demo";
    let (drop, grab) = if demo {
        let step = |t: f32| since >= t && since - time.delta_secs() < t;
        (master && step(3.0), !master && step(5.0) && held.is_none())
    } else {
        (keys.just_pressed(KeyCode::KeyP), keys.just_pressed(KeyCode::KeyG))
    };
    if drop {
        net.net_spawn("debug/Box", Transform::from_translation(front));
    }
    if grab && held.take().is_none() {
        let reach = if demo { f32::MAX } else { 3.0 };
        let near = positions.iter().map(|(e, _, t)| (e, t.translation, t.translation.distance(eye.translation()))).filter(|n| n.2 < reach).min_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((e, at, _)) = near {
            if net.request_ownership(e) {
                *held = Some((e, demo.then_some(at)));
            }
        }
    }
    if let Some((e, centre)) = *held {
        let view = positions.get(e).map(|(_, o, _)| o.view).ok();
        let mine = view.is_some_and(|v| rt.room.lock().unwrap().is_mine(v));
        let (s, c) = (since * 1.5).sin_cos();
        match positions.get_mut(e) {
            Ok((_, _, mut t)) if mine => t.translation = centre.map_or(front, |m| m + Vec3::new(c * 0.6, 0.4 + 0.3 * s, s * 0.6)),
            // gone, or someone else took it: let go
            _ => *held = None,
        }
    }
}
