//! Local interaction in screen mode: a reticle in the middle of the screen, the prop under it within reach (the
//! targeting config's 2.5 m from the player, not behind a wall), left click to grab it into the right hand, left click
//! again to throw it toward what the view is on (`ThrowToolBehaviorConfig`: 8 m/s, spinning 360 deg/s), Q to drop it
//! (with the hand's own motion plus `dropSpeed`). Props that are let go fall and tumble (`body`), and the player
//! bumps into them and can stand on them.
//!
//! Networked props (the realtime lane's `netobj`): grabbing one takes it over (`request_ownership`), so its Transform
//! (held, thrown, falling) is what everyone sees; one owned by someone else is `Remote` (physics leaves it alone, the
//! owner's poses move it), and when someone else takes the one in this hand, the hand lets go. A networked object
//! without a `Grabbable` gets the stand-in prop's box (0.3 m). The world lane's `Prop` with its colliders should
//! insert `Grabbable` with its own shape. `HandEvent` messages say when this player grabbed or let go of a prop.
//! `METALLIC_RC_TEST_PROPS=1` puts three props of our own in front of the player once the room is loaded (and again
//! on P), local only; in a room the master also drops one networked crate, for trying it with two players.
pub mod body;

use bevy::{
    prelude::*,
    ui::UiTransform,
    ui::Val2,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::{
    avatar::{anim::Grip, LookPanel},
    collide::Obb,
    netobj::{NetObject, NetObjects, RoomLink},
    player::{camera, Player, PlayerSystems, Solids, Tuning, View},
};
use body::{Body, Shape};

/// Where the interaction wants the local player's hands (world poses; `None` leaves them to the screen pose) and
/// their shapes.
#[derive(Resource, Default)]
pub struct LocalHands {
    pub place: [Option<Transform>; 2],
    pub grip: [Grip; 2],
}

/// A prop that can be picked up and thrown.
#[derive(Component, Clone, Copy, Debug)]
pub struct Grabbable {
    pub shape: Shape,
    pub mass: f32,
    /// 0 no bounce, 1 all of it
    pub bounce: f32,
}

/// A prop falling or tumbling here (not held, not moved by someone else).
#[derive(Component)]
pub struct Physics(pub Body);

/// The prop in this player's hand.
#[derive(Component)]
pub struct Held;

/// A prop this player just let go of: it starts where the hand is, inside the player's capsule, so the player does
/// not bump into it for a moment (seconds left).
#[derive(Component)]
struct JustReleased(f32);

/// A prop another player moves: physics and holding leave it alone (grabbing it still works and sends `Grabbed`).
#[derive(Component)]
pub struct Remote;

/// What this player's hand did, for the network side.
#[derive(Message, Clone, Debug)]
pub enum HandEvent {
    Grabbed(Entity),
    Released { entity: Entity, velocity: Vec3, spin: Vec3 },
}

/// The props as boxes, for the player to bump into (the held one left out).
#[derive(Resource, Default)]
pub struct PropSolids(pub Vec<Obb>);

#[derive(Resource, Default)]
struct Holding {
    entity: Option<Entity>,
    /// the prop's turn relative to the view's yaw
    turn: Quat,
    /// where it was last frame, for its speed
    last: Option<Vec3>,
    velocity: Vec3,
}

#[derive(Component)]
struct Reticle;

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Holding>()
            .init_resource::<PropSolids>()
            .add_message::<HandEvent>()
            .add_systems(Startup, spawn_reticle)
            .add_systems(Update, (ownership, grab_and_throw, claim, hold, simulate).chain().in_set(PlayerSystems::Interact))
            .add_systems(Update, (test_props, test_net_prop).before(PlayerSystems::Move));
    }
}

fn spawn_reticle(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50.0),
            top: percent(50.0),
            width: px(6.0),
            height: px(6.0),
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::all(percent(50.0)),
            ..default()
        },
        UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        Reticle,
    ));
}

/// The camera in the world: the player's eyes times the view's own offset.
fn camera_at(eye: &Transform, view: &Transform) -> Transform {
    eye.mul_transform(*view)
}

/// The first wall along a ray, as a distance.
fn wall_hit(solids: &[Obb], from: Vec3, dir: Vec3) -> f32 {
    solids.iter().filter_map(|b| body::ray_box(b, from, dir)).fold(f32::INFINITY, f32::min)
}

fn ray_prop(g: &Grabbable, at: &Transform, from: Vec3, dir: Vec3) -> Option<f32> {
    match g.shape {
        Shape::Ball(r) => body::ray_ball(at.translation, r * at.scale.x, from, dir),
        Shape::Box(h) => body::ray_box(&Shape::Box(h * at.scale).obb(at.translation, at.rotation), from, dir),
    }
}

/// Targets the prop under the reticle; a click grabs it, or throws the one held; Q drops it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn grab_and_throw(
    mut commands: Commands,
    (mouse, keys, tuning, solids, panel): (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>, Res<Tuning>, Res<Solids>, Res<LookPanel>),
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    player: Single<(&Player, &Transform), Without<View>>,
    view: Single<&Transform, (With<View>, Without<Player>)>,
    props: Query<(Entity, &Grabbable, &Transform), (Without<Player>, Without<View>, Without<Held>)>,
    held: Query<(&Grabbable, &Transform), With<Held>>,
    mut holding: ResMut<Holding>,
    mut hands: ResMut<LocalHands>,
    mut events: MessageWriter<HandEvent>,
    mut reticle: Single<(&mut Node, &mut BackgroundColor, &mut Visibility), With<Reticle>>,
    mut was_locked: Local<bool>,
) {
    let (p, eye) = *player;
    let cam = camera_at(eye, &view);
    let (from, dir) = (cam.translation, *cam.forward());
    let wall = wall_hit(&solids.0, from, dir);
    let target = props
        .iter()
        .filter_map(|(e, g, t)| ray_prop(g, t, from, dir).map(|d| (e, d)))
        .filter(|&(_, d)| d < wall && (from + dir * d).distance(eye.translation) <= tuning.reach)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    // clicks count once the mouse is already taken for looking (the first click only takes it)
    let locked = cursor.grab_mode != CursorGrabMode::None && panel.open.is_none();
    let click = locked && *was_locked && mouse.just_pressed(MouseButton::Left);
    *was_locked = locked;

    let (node, colour, vis) = &mut *reticle;
    vis.set_if_neq(if locked { Visibility::Inherited } else { Visibility::Hidden });
    let size = if target.is_some() && holding.entity.is_none() { 14.0 } else { 6.0 };
    if node.width != px(size) {
        (node.width, node.height) = (px(size), px(size));
        colour.0 = if size > 6.0 { Color::srgba(1.0, 1.0, 1.0, 0.35) } else { Color::srgba(1.0, 1.0, 1.0, 0.9) };
    }

    if let Some(e) = holding.entity {
        let Ok((_, t)) = held.get(e) else {
            // it went away (despawned, or taken by someone else)
            *holding = Holding::default();
            return;
        };
        let drop = locked && keys.just_pressed(KeyCode::KeyQ);
        if !(click || drop) {
            return;
        }
        let (velocity, spin) = if click {
            // toward what the view is on (the gaze target, 0.5 to 30 m), from where the prop is
            let props_hit = props.iter().filter_map(|(_, g, t)| ray_prop(g, t, from, dir)).fold(f32::INFINITY, f32::min);
            let gaze = from + dir * wall.min(props_hit).clamp(0.5, 30.0);
            let aim = (gaze - t.translation).normalize_or(dir);
            (aim * tuning.throw_speed, cam.right() * -tuning.throw_spin.to_radians())
        } else {
            (holding.velocity + Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero() * tuning.drop_speed, Vec3::ZERO)
        };
        let mut b = Body::new(t.translation, t.rotation);
        (b.vel, b.spin) = (velocity, spin);
        commands.entity(e).remove::<Held>().insert((Physics(b), JustReleased(0.3)));
        events.write(HandEvent::Released { entity: e, velocity, spin });
        *holding = Holding::default();
        return;
    }
    hands.grip = [Grip::Open, if target.is_some() { Grip::Point } else { Grip::Open }];
    hands.place = [None, None];
    if let (true, Some((e, _))) = (click, target) {
        let Ok((_, _, t)) = props.get(e) else { return };
        commands.entity(e).remove::<Physics>().insert(Held);
        events.write(HandEvent::Grabbed(e));
        *holding = Holding { entity: Some(e), turn: Quat::from_rotation_y(-p.yaw) * t.rotation, last: None, velocity: Vec3::ZERO };
    }
}

/// Keeps props in step with who owns them: someone else's is `Remote` (and leaves this hand), one that became this
/// player's falls and settles here.
#[allow(clippy::type_complexity)]
fn ownership(
    mut commands: Commands,
    mut holding: ResMut<Holding>,
    props: Query<(Entity, &NetObject, &Transform, Has<Remote>, Has<Held>), With<Grabbable>>,
) {
    for (e, o, t, remote, held) in &props {
        if !o.mine && !remote {
            commands.entity(e).remove::<(Physics, Held, JustReleased)>().insert(Remote);
            if held || holding.entity == Some(e) {
                *holding = Holding::default();
            }
        } else if o.mine && remote {
            commands.entity(e).remove::<Remote>();
            if !held {
                commands.entity(e).insert(Physics(Body::new(t.translation, t.rotation)));
            }
        }
    }
}

/// A grab takes a networked prop over; networked objects without a shape get the stand-in prop's box.
#[allow(clippy::type_complexity)]
fn claim(
    mut commands: Commands,
    mut events: MessageReader<HandEvent>,
    mut net: NetObjects,
    bare: Query<(Entity, &NetObject, &Transform), Without<Grabbable>>,
) {
    for ev in events.read() {
        if let HandEvent::Grabbed(e) = ev {
            net.request_ownership(*e);
        }
    }
    for (e, o, t) in &bare {
        // INFERRED: netobj's stand-in `spawn_prop` is a 0.3 m box of 1 kg
        let g = Grabbable { shape: Shape::Box(Vec3::splat(0.15)), mass: 1.0, bounce: 0.1 };
        if o.mine {
            commands.entity(e).insert((g, Physics(Body::new(t.translation, t.rotation))));
        } else {
            commands.entity(e).insert((g, Remote));
        }
    }
}

/// Keeps the held prop in front of the view, low on the right where the right hand holds it, pulled in where a wall
/// is in the way. INFERRED placement (the game's `DefaultToolRelativeDistance` 1 m is for its manipulation mode).
#[allow(clippy::type_complexity)]
fn hold(
    time: Res<Time>,
    solids: Res<Solids>,
    tuning: Res<Tuning>,
    player: Single<(&Player, &Transform), (Without<View>, Without<Held>)>,
    mut held: Query<(&Grabbable, &mut Transform), (With<Held>, Without<Player>, Without<View>)>,
    mut holding: ResMut<Holding>,
    mut hands: ResMut<LocalHands>,
) {
    let (p, eye) = *player;
    let Some(e) = holding.entity else { return };
    let Ok((g, mut t)) = held.get_mut(e) else { return };
    let r = g.shape.radius() * t.scale.max_element();
    let dist = (tuning.min_hold_distance + r).max(0.6);
    let want = eye.transform_point(Vec3::new(0.2, -0.22, -dist));
    let at = camera::pull_in(&solids.0, eye.translation, want, r * 0.8);
    let dt = time.delta_secs().max(1e-3);
    let k = 1.0 - (-dt * 30.0).exp();
    let pos = match holding.last {
        Some(_) => t.translation.lerp(at, k),
        None => t.translation.lerp(at, 0.5),
    };
    holding.velocity = holding.last.map_or(Vec3::ZERO, |l| (pos - l) / dt);
    holding.last = Some(pos);
    t.translation = pos;
    t.rotation = Quat::from_rotation_y(p.yaw) * holding.turn;
    // the right hand closes on the near side of it
    let hand = pos - (pos - eye.translation).normalize_or(Vec3::NEG_Z) * (r * 0.85);
    hands.place = [None, Some(Transform::from_translation(hand).with_rotation(eye.rotation * Quat::from_rotation_x(0.25)))];
    hands.grip = [Grip::Open, Grip::Hold];
}

/// Moves the free props, against the room and each other, and lists them for the player to bump into.
#[allow(clippy::type_complexity)]
fn simulate(
    mut commands: Commands,
    time: Res<Time>,
    solids: Res<Solids>,
    mut prop_solids: ResMut<PropSolids>,
    mut props: Query<(Entity, &Grabbable, &mut Transform, Option<&mut Physics>, Has<Held>, Has<Remote>)>,
    mut released: Query<(Entity, &mut JustReleased)>,
) {
    let dt = time.delta_secs().min(0.1);
    let mut fresh = Vec::new();
    for (e, mut r) in &mut released {
        r.0 -= dt;
        if r.0 <= 0.0 {
            commands.entity(e).remove::<JustReleased>();
        } else {
            fresh.push(e);
        }
    }
    let boxes: Vec<(Entity, Obb, bool)> = props.iter().map(|(e, g, t, _, held, _)| {
        let shape = match g.shape {
            Shape::Box(h) => Shape::Box(h * t.scale),
            Shape::Ball(r) => Shape::Ball(r * t.scale.x),
        };
        (e, shape.obb(t.translation, t.rotation), held)
    }).collect();
    prop_solids.0 = boxes.iter().filter(|b| !b.2 && !fresh.contains(&b.0)).map(|b| b.1.clone()).collect();
    for (e, g, mut t, physics, held, remote) in &mut props {
        let Some(mut ph) = physics.filter(|_| !held && !remote) else { continue };
        if ph.0.asleep() {
            continue;
        }
        let mut world = solids.0.clone();
        world.extend(boxes.iter().filter(|b| b.0 != e && !b.2).map(|b| b.1.clone()));
        ph.0.step(&g.shape, g.mass, g.bounce, &world, dt);
        // fell out of the room: put it to rest where it is
        if ph.0.pos.y < -50.0 {
            ph.0.vel = Vec3::ZERO;
        }
        (t.translation, t.rotation) = (ph.0.pos, ph.0.rot);
    }
}

/// `METALLIC_RC_TEST_PROPS=1` in a room: two seconds after the room has its colliders, the master drops one networked
/// crate in front of itself (and anyone in the room another on N).
fn test_net_prop(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    solids: Res<Solids>,
    link: Res<RoomLink>,
    player: Single<(&Player, &Transform)>,
    mut net: NetObjects,
    mut since: Local<Option<f32>>,
) {
    if solids.0.is_empty() || std::env::var_os("METALLIC_RC_TEST_PROPS").is_none() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    if actor.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let due = *since.get_or_insert(now) + 2.0;
    let go = (master && now >= due && now - time.delta_secs() < due) || keys.just_pressed(KeyCode::KeyN);
    if !go {
        return;
    }
    let (p, eye) = *player;
    let at = p.feet(eye) + Quat::from_rotation_y(p.yaw) * Vec3::new(0.0, 1.0, -1.0);
    let e = net.net_spawn("test/Crate", Transform::from_translation(at));
    info!("networked test crate at {at}: {e:?}");
}

/// `METALLIC_RC_TEST_PROPS=1`: a second after the room has its colliders (and P again later), three props of our own
/// in front of the player (a crate, a ball and a small cube).
#[allow(clippy::too_many_arguments)]
fn test_props(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    solids: Res<Solids>,
    player: Single<(&Player, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut since: Local<Option<f32>>,
) {
    if solids.0.is_empty() || std::env::var_os("METALLIC_RC_TEST_PROPS").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let first = since.get_or_insert(now);
    let due = *first + 1.0;
    let go = (now >= due && now - time.delta_secs() < due) || keys.just_pressed(KeyCode::KeyP);
    if !go {
        return;
    }
    let (p, eye) = *player;
    let feet = p.feet(eye);
    let fwd = Quat::from_rotation_y(p.yaw);
    info!("test props in front of {feet}");
    let props = [
        ("test crate", Shape::Box(Vec3::splat(0.2)), Color::srgb(0.85, 0.55, 0.2), 2.0, 0.1, Vec3::new(-0.5, 1.0, -1.2)),
        ("test ball", Shape::Ball(0.12), Color::srgb(0.95, 0.25, 0.4), 0.5, 0.6, Vec3::new(0.0, 1.2, -1.1)),
        ("test cube", Shape::Box(Vec3::new(0.15, 0.1, 0.12)), Color::srgb(0.25, 0.7, 0.95), 1.0, 0.1, Vec3::new(0.5, 1.0, -1.2)),
    ];
    for (name, shape, colour, mass, bounce, at) in props {
        let mesh = match shape {
            Shape::Box(h) => meshes.add(Cuboid::from_size(h * 2.0)),
            Shape::Ball(r) => meshes.add(Sphere::new(r).mesh().uv(24, 14)),
        };
        let pos = feet + fwd * at;
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: colour, perceptual_roughness: 0.6, ..default() })),
            Transform::from_translation(pos).with_rotation(fwd),
            Grabbable { shape, mass, bounce },
            Physics(Body::new(pos, fwd)),
            Name::new(name),
        ));
    }
}
