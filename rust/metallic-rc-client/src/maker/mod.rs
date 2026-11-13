//! The Maker Pen: build in a room with the game's shapes and props, for everyone in it, saved on the server.
//!
//! M takes the pen out or puts it away. With it out (mouse taken for looking):
//! - left click on nothing: makes the palette's current item where the view points and holds it; left click again
//!   puts it down. Left click on a maker object picks it up (it follows the view at its distance).
//! - while holding: wheel = nearer/further, Shift+wheel = turn, Ctrl+wheel = tilt, Alt+wheel = size.
//! - right click: copy of the held or aimed-at object, held. X or Delete: delete. `[` `]`: colour (the held or
//!   aimed-at shape, and the next ones). G: snap to grid (0.25 m, 15 degrees). Z / Y: undo / redo. B: the palette
//!   (shapes, props, colours, materials, snap, save, publish, builders).
//!
//! Network: every maker object is a `netobj` object whose address says what it is (`kit::Kind`); moving one takes
//! it over (`request_ownership`) and its owner's Transform is what everyone sees. A change of colour, material or
//! size respawns it with the new address (late joiners get it from the room cache); while held, others see such
//! changes at once through a "MkLook" RPC. The holder tells the others with "MkSel" so nobody takes an object
//! somebody holds (`EditedBy`); if two pick one up at the same moment, the ownership rule keeps one and the other
//! lets go. Saving and permissions: `save` (the server's `creations.rs`): only the room's owner and their builders
//! may build; the room's master loads the save when the room starts.
mod demo;
pub mod edit;
pub mod kit;
pub mod panel;
pub mod save;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use rr_net::objects::{creator, RpcTarget};

use crate::{
    avatar::LookPanel,
    collide::{Obb, Solids},
    netobj::{NetObject, NetObjects, NetRpc, RoomLink},
    player::{Controls, Player, PlayerSystems, View},
    world::PropColliders,
};
use edit::{Change, History, ObjState};
use kit::{Finish, Kind, Shape, Swatch, PROPS};
pub use save::attach;

/// How many maker objects a second may be spawned (the room server lets 60 events a second through).
const SPAWNS_PER_SECOND: f32 = 40.0;
/// Seconds after joining before the master loads the room's save (the room cache arrives first).
const LOAD_AFTER: f64 = 2.0;
const REACH: f32 = 40.0;

/// A palette item: a shape at a starting size, or a prop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Shape(Shape, &'static str, [f32; 3]),
    Prop(usize),
}

pub const SHAPE_ITEMS: [Item; 9] = [
    Item::Shape(Shape::Cube, "Cube", [0.5; 3]),
    Item::Shape(Shape::Cube, "Slab", [1.0, 0.1, 1.0]),
    Item::Shape(Shape::Sphere, "Sphere", [0.5; 3]),
    Item::Shape(Shape::Cylinder, "Cylinder", [0.5; 3]),
    Item::Shape(Shape::Cone, "Cone", [0.5; 3]),
    Item::Shape(Shape::Wedge, "Wedge", [1.0; 3]),
    Item::Shape(Shape::Pyramid, "Pyramid", [0.5; 3]),
    Item::Shape(Shape::Torus, "Torus", [0.6; 3]),
    Item::Shape(Shape::Capsule, "Capsule", [0.5; 3]),
];

impl Item {
    pub fn label(self) -> &'static str {
        match self {
            Item::Shape(_, l, _) => l,
            Item::Prop(i) => PROPS[i].label,
        }
    }
}

/// On every maker object: what it is and its uid (from its address).
#[derive(Component, Clone, Debug)]
pub struct Made {
    pub kind: Kind,
    pub uid: u32,
}

/// Another player holds this maker object (their actor number).
#[derive(Component)]
pub struct EditedBy(pub i32);

/// A shape whose colour or material changed here: `recolor` gives it the material.
#[derive(Component)]
struct Recolor(i32, i32);

#[derive(Component)]
struct PenModel;

#[derive(Component)]
struct Hud;

/// The held object.
struct Sel {
    entity: Entity,
    uid: u32,
    dist: f32,
    /// as it was when picked up (`None`: made now)
    before: Option<ObjState>,
    /// what it is now (colour, material, size may be changed and not yet respawned)
    kind: Kind,
    look_changed: bool,
    /// frames to wait for a just spawned entity
    fresh: u8,
}

#[derive(Resource)]
pub struct Pen {
    pub equipped: bool,
    pub item: Item,
    pub color: i32,
    pub material: i32,
    pub snap: bool,
    pub colors: Vec<Swatch>,
    pub finishes: Vec<Finish>,
    pub info: Option<save::RoomInfo>,
    /// the palette panel, while open
    pub panel: Option<Entity>,
    /// what the panel asked for
    pub request: Option<Request>,
    sel: Option<Sel>,
    history: History,
    status: (String, f64),
    room: usize,
    joined_at: Option<f64>,
    loaded: bool,
    queue: VecDeque<ObjState>,
    budget: f32,
    /// when the room's rights were last asked for again (after a refusal)
    asked: f64,
}

/// Panel actions that need the room's objects or the network.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    Save,
    Publish,
    Builders,
    Undo,
    Redo,
}

impl Default for Pen {
    fn default() -> Self {
        let (colors, finishes) = kit::palette();
        Pen {
            equipped: false, item: SHAPE_ITEMS[0], color: 13, material: 9, snap: true, colors, finishes, info: None, panel: None,
            request: None, sel: None, history: History::default(), status: (String::new(), 0.0), room: 0, joined_at: None, loaded: false,
            queue: VecDeque::new(), budget: 0.0, asked: 0.0,
        }
    }
}

impl Pen {
    pub fn say(&mut self, msg: impl Into<String>, now: f64) {
        self.status = (msg.into(), now);
    }

    /// Whether this player may build in the room it is in (as the server said).
    pub fn can_edit(&self) -> bool {
        let room = save::link().map(|l| l.room);
        self.info.as_ref().is_some_and(|i| i.can_edit && Some(&i.room) == room.as_ref())
    }

    fn refusal(&self) -> String {
        match self.info.as_ref().and_then(|i| i.owner.clone()) {
            Some(o) => format!("This is {o}'s room: only they and their builders can build here"),
            None if self.info.is_some() => "Nobody builds in this room: make one of your own (B, Publish)".into(),
            None => "Not connected to the server: building is off".into(),
        }
    }

    pub fn color_name(&self, id: i32) -> &str {
        self.colors.iter().find(|c| c.id == id).map_or("?", |c| c.name.as_str())
    }

    pub fn finish_name(&self, id: i32) -> &str {
        self.finishes.iter().find(|f| f.id == id).map_or("?", |f| f.name.as_str())
    }

    /// The next colour in palette order (`by` = +1 / -1).
    pub fn step_color(&self, id: i32, by: i32) -> i32 {
        step_color(&self.colors, id, by)
    }

    /// What the current item makes, with the current colour and material.
    pub fn kind(&self) -> (Kind, Vec3) {
        match self.item {
            Item::Shape(shape, _, s) => (Kind::Shape { shape, color: self.color, material: self.material }, Vec3::from(s)),
            Item::Prop(i) => (Kind::Prop(PROPS[i].address.into()), Vec3::ONE),
        }
    }
}

/// Meshes and materials of the shapes, made once.
#[derive(Resource, Default)]
struct Looks {
    meshes: HashMap<Shape, Handle<Mesh>>,
    mats: HashMap<(i32, i32), Handle<StandardMaterial>>,
}

impl Looks {
    fn mesh(&mut self, meshes: &mut Assets<Mesh>, s: Shape) -> Handle<Mesh> {
        self.meshes.entry(s).or_insert_with(|| meshes.add(s.mesh())).clone()
    }

    /// The palette colour with the material's smoothness (INFERRED: plain PBR values for the game's textured AG/MakerPen
    /// materials; Neon glows, the metals shine).
    fn material(&mut self, mats: &mut Assets<StandardMaterial>, pen: &Pen, color: i32, material: i32) -> Handle<StandardMaterial> {
        self.mats.entry((color, material)).or_insert_with(|| {
            let rgb = pen.colors.iter().find(|c| c.id == color).map_or([0.8; 3], |c| c.rgb);
            let smooth = pen.finishes.iter().find(|f| f.id == material).map_or(0.5, |f| f.smoothness);
            let base = Color::srgb(rgb[0], rgb[1], rgb[2]);
            mats.add(StandardMaterial {
                base_color: base,
                perceptual_roughness: (1.0 - smooth * 0.75).clamp(0.08, 1.0),
                metallic: if kit::METALS.contains(&material) { 0.85 } else { 0.0 },
                emissive: if material == kit::NEON { base.to_linear() * 4.0 } else { LinearRgba::BLACK },
                ..default()
            })
        }).clone()
    }
}

pub struct MakerPlugin;

impl Plugin for MakerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pen>()
            .init_resource::<Looks>()
            .add_observer(tag_made)
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (follow_room, receive_rpcs, use_pen, recolor, drain_queue, serve_requests, show).chain().after(PlayerSystems::Move).before(PlayerSystems::Interact))
            .add_systems(Update, demo::demo.after(use_pen).before(drain_queue))
            .add_systems(Update, panel::panel.run_if(|c: Res<Controls>| !c.typing).before(use_pen))
            .add_systems(PostUpdate, panel::free_cursor);
    }
}

static UIDS: AtomicU32 = AtomicU32::new(0);

/// A new object uid: the clock mixed with a counter (32 bits; a clash would only make undo pick the wrong one).
fn new_uid() -> u32 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let n = UIDS.fetch_add(1, Ordering::Relaxed) as u64;
    let mut h = nanos ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (h ^ (h >> 32)) as u32 | 1
}

/// A networked object whose address is a maker object's gets `Made`, its shape's mesh, colour and colliders (props
/// get theirs from the world loader). Runs as the object appears, before the interaction can treat it as a prop.
#[allow(clippy::too_many_arguments)]
fn tag_made(add: On<Add<NetObject>>, objects: Query<&NetObject>, mut commands: Commands, mut looks: ResMut<Looks>, mut meshes: ResMut<Assets<Mesh>>,
            mut mats: ResMut<Assets<StandardMaterial>>, pen: Res<Pen>) {
    let Ok(o) = objects.get(add.entity) else { return };
    let Some((kind, uid)) = Kind::parse(&o.address) else { return };
    let mut e = commands.entity(add.entity);
    if let Kind::Shape { shape, color, material } = &kind {
        e.insert((Mesh3d(looks.mesh(&mut meshes, *shape)), MeshMaterial3d(looks.material(&mut mats, &pen, *color, *material)),
                  PropColliders(shape.colliders()), Name::new(format!("maker {}", o.address))));
    }
    e.insert(Made { kind, uid });
}

/// A new room connection: nothing held, no history, the save not loaded yet; the server's answers.
fn follow_room(time: Res<Time>, link: Res<RoomLink>, mut pen: ResMut<Pen>, made: Query<(), With<Made>>) {
    let now = time.elapsed_secs_f64();
    for a in save::take() {
        match a {
            save::Answer::Info(Ok(i)) => pen.info = Some(i),
            save::Answer::Info(Err(e)) => pen.say(format!("Room save: {e}"), now),
            save::Answer::Saved(Ok(v)) => {
                if let Some(i) = pen.info.as_mut() {
                    i.version = v;
                }
                pen.say(format!("Room saved (version {v})"), now);
            }
            save::Answer::Saved(Err(e)) => {
                // "saved meanwhile: version N": take N, the next Save overwrites
                if let (Some(v), Some(i)) = (e.rsplit(' ').next().and_then(|v| v.parse().ok()), pen.info.as_mut()) {
                    i.version = v;
                }
                pen.say(format!("Not saved: {e}"), now);
            }
            save::Answer::Published(Ok(name)) => pen.say(format!("Published \"{name}\": it is in the rooms list"), now),
            save::Answer::Published(Err(e)) => pen.say(format!("Not published: {e}"), now),
            save::Answer::Builders(Ok(ids)) => pen.say(format!("{} player(s) may build here now", ids.len()), now),
            save::Answer::Builders(Err(e)) => pen.say(format!("Builders not changed: {e}"), now),
        }
    }
    let rt = link.get();
    let id = rt.as_ref().map_or(0, |rt| Arc::as_ptr(&rt.room) as usize);
    if id != pen.room {
        pen.room = id;
        pen.sel = None;
        pen.history.clear();
        pen.queue.clear();
        (pen.joined_at, pen.loaded) = (None, false);
    }
    let Some(rt) = rt else { return };
    let (actor, master) = {
        let r = rt.room.lock().unwrap();
        (r.actor(), r.is_master())
    };
    if actor.is_none() || pen.loaded {
        return;
    }
    let joined = *pen.joined_at.get_or_insert(now);
    let info_here = pen.info.as_ref().is_some_and(|i| Some(&i.room) == save::link().map(|l| l.room).as_ref());
    if now - joined < LOAD_AFTER || !info_here {
        return;
    }
    pen.loaded = true;
    // the first player in a room instance builds the save; anyone joining later gets it from the room
    if master && made.is_empty() {
        let objects = pen.info.as_ref().map(|i| i.objects.clone()).unwrap_or_default();
        if !objects.is_empty() {
            pen.say(format!("Loading {} saved objects", objects.len()), now);
        }
        pen.queue.extend(objects);
    }
}

/// Spawns queued objects (a save being loaded, undo) at the rate the room server allows.
fn drain_queue(time: Res<Time>, mut pen: ResMut<Pen>, mut net: NetObjects) {
    if pen.queue.is_empty() {
        pen.budget = 0.0;
        return;
    }
    pen.budget = (pen.budget + time.delta_secs() * SPAWNS_PER_SECOND).min(SPAWNS_PER_SECOND);
    while pen.budget >= 1.0 {
        let Some(o) = pen.queue.pop_front() else { break };
        pen.budget -= 1.0;
        net.net_spawn(&o.address, o.at);
    }
}

/// "MkSel" (someone picked up / put down a maker object) and "MkLook" (its colour, material, size while held).
fn receive_rpcs(mut commands: Commands, mut rpcs: MessageReader<NetRpc>, mut made: Query<(Entity, &mut Made, &mut Transform, Option<&EditedBy>)>, link: Res<RoomLink>) {
    for r in rpcs.read() {
        let Some(e) = r.entity else { continue };
        let Ok((_, mut m, mut t, _)) = made.get_mut(e) else { continue };
        match r.method.as_str() {
            "MkSel" if r.bytes.first() == Some(&1) => drop(commands.entity(e).insert(EditedBy(r.sender))),
            "MkSel" => drop(commands.entity(e).remove::<EditedBy>()),
            "MkLook" => {
                let Some((color, material, scale)) = decode_look(&r.bytes) else { continue };
                if let Kind::Shape { color: c, material: mt, .. } = &mut m.kind {
                    (*c, *mt) = (color, material);
                    commands.entity(e).insert(Recolor(color, material));
                }
                t.scale = scale;
            }
            _ => {}
        }
    }
    // the holder left the room
    let Some(rt) = link.get() else { return };
    let here: Vec<i32> = rt.room.lock().unwrap().shown().iter().map(|s| s.actor).collect();
    for (e, _, _, by) in &made {
        if by.is_some_and(|b| !here.contains(&b.0)) {
            commands.entity(e).remove::<EditedBy>();
        }
    }
}

/// Shapes whose colour or material changed get that material.
fn recolor(mut commands: Commands, pen: Res<Pen>, changed: Query<(Entity, &Recolor)>, mut looks: ResMut<Looks>, mut mats: ResMut<Assets<StandardMaterial>>) {
    for (e, r) in &changed {
        commands.entity(e).insert(MeshMaterial3d(looks.material(&mut mats, &pen, r.0, r.1))).remove::<Recolor>();
    }
}

fn encode_look(color: i32, material: i32, scale: Vec3) -> Vec<u8> {
    let mut b = Vec::with_capacity(20);
    b.extend(color.to_le_bytes());
    b.extend(material.to_le_bytes());
    for v in scale.to_array() {
        b.extend(v.to_le_bytes());
    }
    b
}

fn decode_look(b: &[u8]) -> Option<(i32, i32, Vec3)> {
    let f = |i: usize| Some(f32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?));
    let i = |i: usize| Some(i32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?));
    Some((i(0)?, i(4)?, Vec3::new(f(8)?, f(12)?, f(16)?)))
}

/// The aiming boxes of a maker object in the world: its colliders (loaded ones for props) under its transform.
fn boxes(m: &Made, cols: Option<&PropColliders>, g: &GlobalTransform) -> Vec<Obb> {
    let own;
    let list = match cols.filter(|c| !c.0.is_empty()) {
        Some(c) => &c.0,
        None => {
            own = m.kind.colliders();
            &own
        }
    };
    let mat = g.to_matrix();
    list.iter().map(|b| b.transformed(mat)).collect()
}

type MadeQuery<'w, 's> = Query<'w, 's, (Entity, &'static Made, &'static NetObject, &'static mut Transform, &'static GlobalTransform, Option<&'static PropColliders>, Option<&'static EditedBy>), (Without<Player>, Without<View>)>;

/// The pen's input: pick up, move, turn, size, colour, copy, delete, put down, undo/redo.
#[allow(clippy::too_many_arguments)]
fn use_pen(
    time: Res<Time>,
    (keys, mouse, wheel): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, Res<AccumulatedMouseScroll>),
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    (controls, look_panel, solids, link): (Res<Controls>, Res<LookPanel>, Res<Solids>, Res<RoomLink>),
    player: Single<&Transform, (With<Player>, Without<Made>, Without<View>)>,
    view: Single<&Transform, (With<View>, Without<Made>, Without<Player>)>,
    mut pen: ResMut<Pen>,
    mut net: NetObjects,
    mut made: MadeQuery,
    mut commands: Commands,
) {
    let pen = &mut *pen;
    let now = time.elapsed_secs_f64();
    let free = !controls.typing && look_panel.open.is_none();
    if free && keys.just_pressed(KeyCode::KeyM) {
        pen.equipped = !pen.equipped;
        if !pen.equipped {
            put_down(pen, &mut net, &made, now);
        } else if !pen.can_edit() {
            let r = pen.refusal();
            pen.say(r, now);
        }
    }
    if !pen.equipped {
        return;
    }
    let active = free && pen.panel.is_none() && cursor.grab_mode != CursorGrabMode::None;
    if pen.panel.is_some() && pen.sel.is_some() {
        put_down(pen, &mut net, &made, now);
    }
    let me = link.get().and_then(|rt| rt.room.lock().unwrap().actor());
    let cam = player.mul_transform(**view);
    let (from, dir) = (cam.translation, *cam.forward());

    // the held object follows the view
    if let Some(sel) = pen.sel.as_mut() {
        match made.get(sel.entity) {
            Ok((_, _, o, ..)) if o.mine || sel.fresh > 0 => {}
            Ok((_, _, o, ..)) => {
                let who = link.get().and_then(|rt| rt.room.lock().unwrap().shown().iter().find(|s| s.actor == o.owner).map(|s| s.name.clone()));
                pen.sel = None;
                pen.say(format!("{} took it", who.unwrap_or_else(|| "Someone".into())), now);
            }
            Err(_) if sel.fresh > 0 => {}
            Err(_) => {
                pen.sel = None;
                pen.say("It was deleted", now);
            }
        }
    }
    if let Some(sel) = pen.sel.as_mut() {
        sel.fresh = sel.fresh.saturating_sub(1);
    }
    let notches = match wheel.unit {
        MouseScrollUnit::Line => wheel.delta.y,
        MouseScrollUnit::Pixel => wheel.delta.y / 40.0,
    };
    let (shift, ctrl, alt) = (
        keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
    );
    let snap = pen.snap;
    let turn_step = if snap { edit::TURN_SNAP } else { edit::TURN_FREE }.to_radians();
    let color_by = keys.just_pressed(KeyCode::BracketRight) as i32 - keys.just_pressed(KeyCode::BracketLeft) as i32;
    if let Some(sel) = pen.sel.as_mut().filter(|_| active) {
        if let Ok((e, _, _, mut t, ..)) = made.get_mut(sel.entity) {
            let mut look = false;
            if notches != 0.0 {
                if shift {
                    t.rotation = Quat::from_rotation_y(notches * turn_step) * t.rotation;
                } else if ctrl {
                    t.rotation = Quat::from_axis_angle(*cam.right(), notches * turn_step) * t.rotation;
                } else if alt {
                    t.scale = edit::clamp_scale(t.scale * edit::SCALE_STEP.powf(notches));
                    look = true;
                } else {
                    sel.dist = (sel.dist + notches * 0.25).clamp(0.5, 30.0);
                }
                if snap && (shift || ctrl) {
                    t.rotation = edit::snap_rotation(t.rotation);
                }
            }
            if let (Kind::Shape { color, .. }, true) = (&mut sel.kind, color_by != 0) {
                *color = step_color(&pen.colors, *color, color_by);
                pen.color = *color;
                look = true;
            }
            let at = from + dir * sel.dist;
            t.translation = if snap { edit::snap_position(at) } else { at };
            if look {
                sel.look_changed = true;
                if let Kind::Shape { color, material, .. } = sel.kind {
                    commands.entity(e).insert(Recolor(color, material));
                    net.send_rpc(e, "MkLook", encode_look(color, material, t.scale), RpcTarget::Others);
                } else {
                    net.send_rpc(e, "MkLook", encode_look(0, 0, t.scale), RpcTarget::Others);
                }
            }
        }
    }
    let undo = (active && keys.just_pressed(KeyCode::KeyZ)) || pen.request == Some(Request::Undo);
    let redo = (active && keys.just_pressed(KeyCode::KeyY)) || pen.request == Some(Request::Redo);
    if matches!(pen.request, Some(Request::Undo | Request::Redo)) {
        pen.request = None;
    }
    let name_of = |actor: i32| link.get().and_then(|rt| rt.room.lock().unwrap().shown().iter().find(|s| s.actor == actor).map(|s| s.name.clone())).unwrap_or_else(|| "Someone".into());
    if undo || redo {
        if !pen.can_edit() {
            let r = pen.refusal();
            return pen.say(r, now);
        }
        put_down(pen, &mut net, &made, now);
        let step = if undo { pen.history.undo() } else { pen.history.redo() };
        match step {
            Some((uid, want)) => {
                let cur = made.iter().find(|m| m.1.uid == uid).map(|m| (m.0, m.6.map(|b| b.0)));
                if let Some((_, Some(by))) = cur.filter(|c| c.1.is_some_and(|b| Some(b) != me)) {
                    pen.say(format!("{} is holding it", name_of(by)), now);
                } else {
                    apply(pen, &mut net, &mut made, cur.map(|c| c.0), want);
                    pen.say(if undo { "Undone" } else { "Redone" }, now);
                }
            }
            None => pen.say(if undo { "Nothing to undo" } else { "Nothing to redo" }, now),
        }
        return;
    }
    if !active {
        return;
    }

    // what the view is on
    let target = made
        .iter()
        .filter(|m| pen.sel.as_ref().is_none_or(|s| s.entity != m.0))
        .filter_map(|(e, m, _, _, g, c, _)| edit::ray_boxes(&boxes(m, c, g), from, dir).map(|d| (e, d)))
        .filter(|&(_, d)| d < REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let wall = edit::ray_boxes(&solids.0, from, dir);
    let (click, copy) = (mouse.just_pressed(MouseButton::Left), mouse.just_pressed(MouseButton::Right));
    let delete = keys.any_just_pressed([KeyCode::KeyX, KeyCode::Delete]);
    if keys.just_pressed(KeyCode::KeyG) {
        pen.snap = !pen.snap;
        let s = if pen.snap { "Snap on: 0.25 m grid, 15 degree turns" } else { "Snap off" };
        pen.say(s, now);
    }
    let wants_edit = click || copy || delete || (color_by != 0 && target.is_some());
    if wants_edit && !pen.can_edit() {
        let r = pen.refusal();
        pen.say(r, now);
        // the owner may have let this player in since: ask again (at most every 3 s)
        if now - pen.asked > 3.0 {
            pen.asked = now;
            save::run(|l| save::Answer::Info(save::fetch(l)));
        }
        return;
    }
    if color_by != 0 && pen.sel.is_none() && target.is_none() {
        pen.color = pen.step_color(pen.color, color_by);
        let s = format!("Colour: {}", pen.color_name(pen.color));
        pen.say(s, now);
    }
    let busy = |by: Option<&EditedBy>| by.filter(|b| Some(b.0) != me).map(|b| b.0);

    if pen.sel.is_some() {
        let e = pen.sel.as_ref().unwrap().entity;
        if delete {
            let sel = pen.sel.take().unwrap();
            if made.contains(e) {
                net.send_rpc(e, "MkSel", vec![0], RpcTarget::Others);
                pen.history.push(Change { uid: sel.uid, before: sel.before, after: None });
                net.destroy(e);
                pen.say("Deleted", now);
            }
        } else if click {
            put_down(pen, &mut net, &made, now);
        } else if copy {
            let (kind, dist) = {
                let s = pen.sel.as_ref().unwrap();
                (s.kind.clone(), s.dist)
            };
            let at = made.get(e).map(|m| *m.3).ok();
            put_down(pen, &mut net, &made, now);
            if let Some(at) = at {
                hold_new(pen, &mut net, kind, at, dist, now);
            }
        }
        return;
    }

    if let Some((e, d)) = target {
        let Ok((_, m, _, t, _, _, by)) = made.get(e) else { return };
        if let Some(b) = busy(by).filter(|_| click || delete || copy || color_by != 0) {
            pen.say(format!("{} is holding it", name_of(b)), now);
            return;
        }
        let (kind, uid, at) = (m.kind.clone(), m.uid, *t);
        let state = ObjState { address: kind.address(uid), at };
        if click {
            if net.request_ownership(e) {
                let dist = at.translation.distance(from).max(0.5);
                net.send_rpc(e, "MkSel", vec![1], RpcTarget::Others);
                pen.sel = Some(Sel { entity: e, uid, dist, before: Some(state), kind, look_changed: false, fresh: 0 });
            }
        } else if copy {
            hold_new(pen, &mut net, kind, at, d.max(1.0), now);
        } else if delete {
            pen.history.push(Change { uid, before: Some(state), after: None });
            net.destroy(e);
            pen.say("Deleted", now);
        } else if color_by != 0 {
            if let Kind::Shape { shape, color, material } = kind {
                let c = pen.step_color(color, color_by);
                pen.color = c;
                let new = ObjState { address: Kind::Shape { shape, color: c, material }.address(uid), at };
                pen.history.push(Change { uid, before: Some(state), after: Some(new.clone()) });
                net.destroy(e);
                net.net_spawn(&new.address, new.at);
                let s = format!("Colour: {}", pen.color_name(c));
                pen.say(s, now);
            }
        }
        return;
    }

    if click {
        // make the current item where the view points, and hold it
        let dist = wall.map_or(3.0, |w| w - 0.5).clamp(1.0, 6.0);
        let (kind, scale) = pen.kind();
        let yaw = (-dir.x).atan2(-dir.z);
        let at = Transform { translation: from + dir * dist, rotation: Quat::from_rotation_y(yaw), scale };
        hold_new(pen, &mut net, kind, at, dist, now);
    }
}

fn step_color(colors: &[Swatch], id: i32, by: i32) -> i32 {
    let n = colors.len() as i32;
    let i = colors.iter().position(|c| c.id == id).map_or(0, |i| i as i32);
    colors.get((i + by).rem_euclid(n.max(1)) as usize).map_or(id, |c| c.id)
}

/// Makes a new maker object for everyone and holds it.
fn hold_new(pen: &mut Pen, net: &mut NetObjects, kind: Kind, at: Transform, dist: f32, now: f64) {
    let uid = new_uid();
    let at = if pen.snap { Transform { translation: edit::snap_position(at.translation), ..at } } else { at };
    match net.net_spawn(&kind.address(uid), at) {
        Some(e) => {
            net.send_rpc(e, "MkSel", vec![1], RpcTarget::Others);
            pen.sel = Some(Sel { entity: e, uid, dist, before: None, kind, look_changed: false, fresh: 3 });
        }
        None => pen.say("Not in a room: nothing made", now),
    }
}

/// Puts the held object down where it is: one undo step; a changed colour, material or size respawns it so the room
/// (and late joiners) keep the change.
fn put_down(pen: &mut Pen, net: &mut NetObjects, made: &MadeQuery, _now: f64) {
    let Some(sel) = pen.sel.take() else { return };
    let Ok((e, _, _, t, ..)) = made.get(sel.entity) else { return };
    let after = ObjState { address: sel.kind.address(sel.uid), at: *t };
    net.send_rpc(e, "MkSel", vec![0], RpcTarget::Others);
    if sel.look_changed {
        net.destroy(e);
        net.net_spawn(&after.address, after.at);
    }
    pen.history.push(Change { uid: sel.uid, before: sel.before, after: Some(after) });
}

/// Puts object `cur` (by uid) into state `want` for undo/redo: moved in place when only its place differs, else
/// respawned (or made, or deleted).
fn apply(pen: &mut Pen, net: &mut NetObjects, made: &mut MadeQuery, cur: Option<Entity>, want: Option<ObjState>) {
    match (cur, want) {
        (Some(e), None) => net.destroy(e),
        (None, Some(s)) => pen.queue.push_back(s),
        (Some(e), Some(s)) => {
            let same = made.get(e).is_ok_and(|m| m.2.address == s.address && m.3.scale == s.at.scale);
            if same && net.request_ownership(e) {
                if let Ok(mut m) = made.get_mut(e) {
                    (m.3.translation, m.3.rotation) = (s.at.translation, s.at.rotation);
                }
            } else {
                net.destroy(e);
                pen.queue.push_back(s);
            }
        }
        (None, None) => {}
    }
}

/// The panel's save, publish and builders requests: they need the room's objects.
fn serve_requests(time: Res<Time>, mut pen: ResMut<Pen>, made: Query<(&NetObject, &Transform), With<Made>>, link: Res<RoomLink>) {
    let Some(r) = pen.request.filter(|r| matches!(r, Request::Save | Request::Publish | Request::Builders)) else { return };
    pen.request = None;
    let now = time.elapsed_secs_f64();
    let objects: Vec<ObjState> = made.iter().filter(|(o, _)| creator(o.view) != 0).map(|(o, t)| ObjState { address: o.address.clone(), at: *t }).collect();
    match r {
        Request::Save => {
            if !pen.can_edit() {
                let s = pen.refusal();
                return pen.say(s, now);
            }
            let version = pen.info.as_ref().map(|i| i.version);
            pen.say(format!("Saving {} objects ...", objects.len()), now);
            save::run(move |l| save::Answer::Saved(save::save(l, version, &objects)));
        }
        Request::Publish => {
            pen.say(format!("Publishing a room with {} objects ...", objects.len()), now);
            save::run(move |l| {
                let mut last = String::new();
                for n in 1..=9 {
                    let name = if n == 1 { format!("{}'s Room", l.me) } else { format!("{}'s Room {n}", l.me) };
                    match save::publish(l, &name, &objects) {
                        Ok(name) => return save::Answer::Published(Ok(name)),
                        Err(e) => last = e,
                    }
                }
                save::Answer::Published(Err(last))
            });
        }
        Request::Builders => {
            let names: Vec<String> = link.get().map(|rt| rt.room.lock().unwrap().shown().iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
            if names.is_empty() {
                return pen.say("Nobody else is here", now);
            }
            pen.say(format!("Letting {} build here ...", names.join(", ")), now);
            save::run(move |l| save::Answer::Builders(save::builders(l, &names)));
        }
        Request::Undo | Request::Redo => {}
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Hud,
        Node { position_type: PositionType::Absolute, bottom: px(18.0), left: percent(50.0), ..default() },
        bevy::ui::UiTransform { translation: bevy::ui::Val2::percent(-50.0, 0.0), ..default() },
        Text::new(""),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.93, 0.95, 0.98)),
        TextLayout::justify(Justify::Center),
        BackgroundColor(Color::srgba(0.05, 0.06, 0.09, 0.7)),
        Visibility::Hidden,
    ));
}

/// The pen in the hand, the line it points along, boxes round what it aims at and holds, and the help line.
#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    time: Res<Time>,
    pen: Res<Pen>,
    view: Query<(Entity, &GlobalTransform), With<View>>,
    mut model: Query<(Entity, &mut Visibility), (With<PenModel>, Without<Hud>)>,
    mut hud: Single<(&mut Text, &mut Visibility), (With<Hud>, Without<PenModel>)>,
    made: Query<(Entity, &Made, &GlobalTransform, Option<&PropColliders>, Option<&EditedBy>)>,
    mut gizmos: Gizmos,
    (mut meshes, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
) {
    let Ok((view_e, cam)) = view.single() else { return };
    let shown = if pen.equipped { Visibility::Inherited } else { Visibility::Hidden };
    match model.single_mut() {
        Ok((_, mut v)) => drop(v.set_if_neq(shown)),
        Err(_) => {
            // our own pen: a dark body with a bright tip, low on the right (not the game's model)
            let body = mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.13, 0.16), perceptual_roughness: 0.4, ..default() });
            let tip = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.1), emissive: LinearRgba::rgb(2.0, 0.9, 0.1), ..default() });
            let at = Transform::from_xyz(0.2, -0.17, -0.42).with_rotation(Quat::from_rotation_x(-1.35) * Quat::from_rotation_z(0.25));
            let pen_e = commands.spawn((PenModel, at, Visibility::Hidden)).with_children(|p| {
                p.spawn((Mesh3d(meshes.add(Cylinder::new(0.012, 0.16))), MeshMaterial3d(body)));
                p.spawn((Mesh3d(meshes.add(Cone { radius: 0.012, height: 0.035 })), MeshMaterial3d(tip), Transform::from_xyz(0.0, 0.0975, 0.0)));
            }).id();
            commands.entity(view_e).add_child(pen_e);
        }
    }
    let (text, vis) = &mut *hud;
    vis.set_if_neq(shown);
    if !pen.equipped {
        return;
    }
    let (from, dir) = (cam.translation(), *cam.forward());
    let tip = cam.transform_point(Vec3::new(0.2, -0.17, -0.42) + Vec3::new(0.0, 0.0, -0.1));
    let held = pen.sel.as_ref().map(|s| s.entity);
    let wire = |gizmos: &mut Gizmos, boxes: &[Obb], c: Color| {
        for b in boxes {
            let rot = Quat::from_mat3(&Mat3::from_cols(b.axes[0], b.axes[1], b.axes[2]));
            gizmos.cube(Transform { translation: b.center, rotation: rot, scale: b.half * 2.0 }, c);
        }
    };
    let mut aim: Option<(f32, Vec<Obb>)> = None;
    for (e, m, g, c, by) in &made {
        let bx = boxes(m, c, g);
        if Some(e) == held {
            wire(&mut gizmos, &bx, Color::srgb(1.0, 0.8, 0.1));
        } else if by.is_some() {
            wire(&mut gizmos, &bx, Color::srgb(0.9, 0.3, 0.3));
        } else if let Some(d) = edit::ray_boxes(&bx, from, dir).filter(|&d| d < REACH && aim.as_ref().is_none_or(|a| d < a.0)) {
            aim = Some((d, bx));
        }
    }
    let end = match (&aim, held) {
        (_, Some(_)) => pen.sel.as_ref().map_or(from + dir * 3.0, |s| from + dir * s.dist),
        (Some((d, bx)), None) => {
            wire(&mut gizmos, bx, Color::WHITE);
            from + dir * *d
        }
        _ => from + dir * 2.0,
    };
    gizmos.line(tip, end, Color::srgba(1.0, 0.6, 0.15, 0.8));
    gizmos.sphere(Isometry3d::from_translation(end), 0.02, Color::srgb(1.0, 0.6, 0.15));

    let what = match pen.item {
        Item::Shape(..) => format!("{}  {}  {}", pen.item.label(), pen.color_name(pen.color), pen.finish_name(pen.material)),
        Item::Prop(_) => pen.item.label().to_string(),
    };
    let rights = if pen.can_edit() { "" } else { "  |  view only" };
    let status = if time.elapsed_secs_f64() - pen.status.1 < 5.0 { format!("\n{}", pen.status.0) } else { String::new() };
    let help = if held.is_some() {
        "click put down | wheel nearer/further | Shift+wheel turn | Ctrl+wheel tilt | Alt+wheel size | [ ] colour | right click copy | X delete"
    } else {
        "click make/pick up | right click copy | [ ] colour | X delete | Z/Y undo/redo | G snap | B palette | M put away"
    };
    let line = format!("Maker Pen: {what}  |  snap {}{rights}\n{help}{status}", if pen.snap { "on" } else { "off" });
    if text.0 != line {
        text.0 = line;
    }
}
