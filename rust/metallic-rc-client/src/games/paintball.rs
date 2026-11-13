//! Paintball on the client: the gun, paintballs, splats, the bots and team markers, flags, and moving this player
//! when the server respawns it.
//!
//! The gun is the game's `(PaintballGun)` prefab (mesh and materials through the world loader), a networked object
//! each player spawns for itself when the room has a match, picked up with the grab system (left click on it). While
//! it is held, left click fires instead of throwing (Q still drops it): at most 5 a second, 33 m/s (the prefab's
//! `Pistol`), toward what the view is on. The server flies the paintball again and decides the hits; this side draws
//! every paintball (its own at once, others' from `Shot`) with gravity and puts a splat where it meets the world
//! (the room's box colliders), sized 0.8-1.2 x (the prefab's `minMaxImpactDecalScale`) a 0.2 m disc (INFERRED).
use std::collections::{HashMap, VecDeque};

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use rr_net::games::{Mode, Phase, Up};

use super::{send, Game, GameEvent, TEAM_COLOURS};
use crate::{
    avatar::{self, Drive, Kit, Look},
    collide::{Collider, Obb},
    interact::{body::{ray_box, Body, Shape}, Grabbable, Held, Physics, Remote},
    netobj::{NetObject, NetObjects, RoomLink},
    player::{Player, PlayerSystems, Solids, View},
    world,
};

pub const GUN: &str = "(PaintballGun)";
/// `Pistol.bulletFireSpeed` and `maximumFiringRate` of the gun prefab
pub const SPEED: f32 = 33.0;
pub const FIRE_RATE: f32 = 5.0;
/// INFERRED: Unity's default gravity on the paintball's rigidbody
pub const GRAVITY: f32 = 9.81;
/// paintballs fly at most this long (the server's limit)
const MAX_LIFE: f32 = 3.0;
const SPLAT: f32 = 0.2;
const MAX_SPLATS: usize = 300;

/// A paintball gun (any player's).
#[derive(Component)]
pub struct PaintballGun;

/// This player's own gun.
#[derive(Resource, Default)]
struct MyGun(Option<Entity>);

#[derive(Component)]
struct Paintball {
    shooter: i32,
    vel: Vec3,
    left: f32,
    /// where it meets the world, if it does: point and surface normal
    lands: Option<(Vec3, Vec3)>,
    team: u8,
}

#[derive(Resource, Default)]
struct Splats(VecDeque<Entity>);

/// Meshes and materials made once.
#[derive(Resource)]
struct Paint {
    ball: Handle<Mesh>,
    splat: Handle<Mesh>,
    marker: Handle<Mesh>,
    pole: Handle<Mesh>,
    cloth: Handle<Mesh>,
    ring: Handle<Mesh>,
    team: [Handle<StandardMaterial>; 2],
    flat: [Handle<StandardMaterial>; 2],
    pole_mat: Handle<StandardMaterial>,
}

/// A bot's avatar, by its id, and the gun in its hand.
#[derive(Component)]
struct BotAvatar {
    id: i32,
    gun: Option<Entity>,
}

/// A team marker over another player's or a bot's head.
#[derive(Component)]
struct Marker(i32);

/// A flag (by team) or a goal ring.
#[derive(Component)]
struct FlagMark(u8);

#[derive(Component)]
struct GoalMark;

pub struct PaintballPlugin;

impl Plugin for PaintballPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MyGun>()
            .init_resource::<Splats>()
            .add_systems(Startup, make_paint)
            .add_systems(Update, (tag_guns, fire).chain().after(super::pump).before(PlayerSystems::Interact))
            .add_systems(Update, (give_gun, respawn, shots, hits, fly, bots, markers, flags).chain().after(super::pump).after(PlayerSystems::Interact));
    }
}

fn make_paint(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let team = TEAM_COLOURS.map(|c| materials.add(StandardMaterial { base_color: c, emissive: LinearRgba::from(c) * 0.4, perceptual_roughness: 0.4, ..default() }));
    let flat = TEAM_COLOURS.map(|c| materials.add(StandardMaterial { base_color: c, unlit: true, depth_bias: 10.0, cull_mode: None, ..default() }));
    commands.insert_resource(Paint {
        ball: meshes.add(Sphere::new(0.05)),
        splat: meshes.add(Circle::new(SPLAT)),
        marker: meshes.add(Sphere::new(0.09)),
        pole: meshes.add(Cylinder::new(0.03, 2.4)),
        cloth: meshes.add(Cuboid::new(0.02, 0.45, 0.7)),
        ring: meshes.add(Annulus::new(1.7, 2.0)),
        team,
        flat,
        pole_mat: materials.add(Color::srgb(0.85, 0.85, 0.8)),
    });
}

/// Every networked paintball gun (others' too) gets the gun's shape for grabbing.
fn tag_guns(mut commands: Commands, guns: Query<(Entity, &NetObject), Without<PaintballGun>>) {
    for (e, o) in &guns {
        if o.address == GUN {
            // INFERRED: a box round the pistol's body and grip
            commands.entity(e).insert((PaintballGun, Grabbable { shape: Shape::Box(Vec3::new(0.05, 0.1, 0.16)), mass: 1.0, bounce: 0.1 }));
        }
    }
}

/// Where a paintball from `from` at `vel` first meets the room's boxes within `life` seconds: time, point, normal.
pub fn impact(solids: &[Collider], from: Vec3, vel: Vec3, life: f32) -> Option<(f32, Vec3, Vec3)> {
    const DT: f32 = 1.0 / 30.0;
    let (mut p, mut v, mut t) = (from, vel, 0.0);
    while t < life {
        let step = DT.min(life - t);
        let next = p + v * step - Vec3::Y * 0.5 * GRAVITY * step * step;
        let seg = next - p;
        let l = seg.length();
        if l > 1e-6 {
            let dir = seg / l;
            let near = solids.iter().filter_map(|c| c.as_box()).filter_map(|b| ray_box(b, p, dir).filter(|&d| d <= l).map(|d| (d, b))).min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((d, b)) = near {
                let at = p + dir * d;
                return Some((t + step * d / l, at, face_normal(b, at)));
            }
        }
        v.y -= GRAVITY * step;
        p = next;
        t += step;
    }
    None
}

/// The outward normal of the box face nearest to `at`.
fn face_normal(b: &Obb, at: Vec3) -> Vec3 {
    let local = at - b.center;
    let i = (0..3).max_by(|&i, &j| {
        let f = |k: usize| local.dot(b.axes[k]).abs() / b.half[k].max(1e-4);
        f(i).total_cmp(&f(j))
    }).unwrap();
    let n = b.axes[i] * local.dot(b.axes[i]).signum();
    if n == Vec3::ZERO { Vec3::Y } else { n }
}

fn spawn_ball(commands: &mut Commands, paint: &Paint, solids: &[Collider], shooter: i32, team: u8, origin: Vec3, vel: Vec3, life: f32) {
    let landing = impact(solids, origin, vel, life.min(MAX_LIFE) + 0.2);
    let (left, lands) = match landing {
        Some((t, at, n)) => (t, Some((at, n))),
        None => (life.min(MAX_LIFE), None),
    };
    commands.spawn((Mesh3d(paint.ball.clone()), MeshMaterial3d(paint.team[team as usize & 1].clone()), Transform::from_translation(origin),
        Paintball { shooter, vel, left, lands, team }, Name::new("paintball")));
}

fn splat(commands: &mut Commands, paint: &Paint, splats: &mut Splats, at: Vec3, normal: Vec3, team: u8, size: f32) {
    let rot = Quat::from_rotation_arc(Vec3::Z, normal) * Quat::from_rotation_z(at.x * 7.0 + at.z * 3.0);
    let e = commands.spawn((Mesh3d(paint.splat.clone()), MeshMaterial3d(paint.flat[team as usize & 1].clone()),
        Transform::from_translation(at + normal * 0.01).with_rotation(rot).with_scale(Vec3::splat(size)), Name::new("splat"))).id();
    splats.0.push_back(e);
    while splats.0.len() > MAX_SPLATS {
        if let Some(old) = splats.0.pop_front() {
            commands.entity(old).despawn();
        }
    }
}

/// Left click with the gun in hand fires (and is not a throw); the server is told, the paintball drawn at once.
#[allow(clippy::too_many_arguments)]
fn fire(
    mut commands: Commands,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    (link, game, solids, paint): (Res<RoomLink>, Res<Game>, Res<Solids>, Option<Res<Paint>>),
    player: Single<&Transform, (With<Player>, Without<View>)>,
    view: Single<&Transform, (With<View>, Without<Player>)>,
    held: Query<&Transform, (With<PaintballGun>, With<Held>, Without<Player>, Without<View>)>,
    time: Res<Time>,
    mut last: Local<f32>,
) {
    let Ok(gun) = held.single() else { return };
    if cursor.grab_mode == CursorGrabMode::None || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    mouse.clear_just_pressed(MouseButton::Left);
    let now = time.elapsed_secs();
    let (Some(me), Some(paint)) = (game.me, paint) else { return };
    let out = game.member(me).is_some_and(|m| m.out);
    if out || now - *last < 1.0 / FIRE_RATE || matches!(game.phase(), Phase::Countdown | Phase::RoundEnd | Phase::Results) {
        return;
    }
    *last = now;
    let cam = player.mul_transform(**view);
    let (eye, look) = (cam.translation, *cam.forward());
    let wall = solids.0.iter().filter_map(|c| c.as_box()).filter_map(|b| ray_box(b, eye, look)).fold(60.0f32, f32::min).max(1.0);
    let muzzle = gun.translation + look * 0.25;
    let vel = (eye + look * wall - muzzle).normalize_or(look) * SPEED;
    let life = impact(&solids.0, muzzle, vel, MAX_LIFE).map_or(MAX_LIFE, |h| h.0);
    send(&link, Up::Fire { origin: muzzle.into(), velocity: vel.into(), life });
    let team = game.my_team().unwrap_or(0);
    spawn_ball(&mut commands, &paint, &solids.0, me, team, muzzle, vel, life);
}

/// Once the room has a match, this player gets a gun in front of it (and again after it went away).
#[allow(clippy::too_many_arguments)]
fn give_gun(
    mut net: NetObjects,
    mut mine: ResMut<MyGun>,
    game: Res<Game>,
    link: Res<RoomLink>,
    solids: Res<Solids>,
    guns: Query<&NetObject, With<PaintballGun>>,
    player: Single<(&Player, &Transform), Without<View>>,
) {
    if game.state.is_none() || solids.0.is_empty() {
        return;
    }
    if let Some(e) = mine.0 {
        if guns.get(e).is_ok() {
            return;
        }
        mine.0 = None;
    }
    if !link.get().is_some_and(|rt| rt.room.lock().unwrap().is_spawned()) {
        return;
    }
    let (p, t) = *player;
    let at = Transform::from_translation(p.feet(t) + Quat::from_rotation_y(p.yaw) * Vec3::new(0.0, 1.1, -0.7)).with_rotation(Quat::from_rotation_y(p.yaw));
    mine.0 = net.net_spawn(GUN, at);
}

/// The server moved this player (a match starting, back in after being out): go there, gun in hand's reach.
#[allow(clippy::too_many_arguments)]
fn respawn(
    mut commands: Commands,
    mut events: MessageReader<GameEvent>,
    game: Res<Game>,
    mine: Res<MyGun>,
    mut net: NetObjects,
    mut player: Single<(&mut Player, &mut Transform), Without<View>>,
    mut guns: Query<&mut Transform, (With<PaintballGun>, Without<Held>, Without<Player>, Without<View>)>,
) {
    for e in events.read() {
        let GameEvent::Respawn { id, at, yaw } = *e else { continue };
        if Some(id) != game.me {
            continue;
        }
        let (p, t) = &mut *player;
        p.respawn(yaw);
        t.translation = at + Vec3::Y * p.eye;
        t.rotation = Quat::from_rotation_y(yaw);
        // a gun lying somewhere comes along
        if let Some(g) = mine.0 {
            if let Ok(mut gt) = guns.get_mut(g) {
                net.request_ownership(g);
                let pos = at + Quat::from_rotation_y(yaw) * Vec3::new(0.0, 1.1, -0.7);
                (gt.translation, gt.rotation) = (pos, Quat::from_rotation_y(yaw));
                commands.entity(g).remove::<Remote>().insert(Physics(Body::new(pos, gt.rotation)));
            }
        }
    }
}

/// Others' paintballs, as the server passes them on.
fn shots(mut commands: Commands, mut events: MessageReader<GameEvent>, game: Res<Game>, solids: Res<Solids>, paint: Option<Res<Paint>>) {
    let Some(paint) = paint else { return };
    for e in events.read() {
        if let GameEvent::Shot { shooter, origin, velocity, life } = *e {
            let team = game.member(shooter).map_or(0, |m| m.team);
            spawn_ball(&mut commands, &paint, &solids.0, shooter, team, origin, velocity, life);
        }
    }
}

/// A hit: the paintball that did it goes, and the paint shows on the one hit.
fn hits(mut commands: Commands, mut events: MessageReader<GameEvent>, game: Res<Game>, paint: Option<Res<Paint>>, mut splats: ResMut<Splats>,
        balls: Query<(Entity, &Paintball, &Transform)>) {
    let Some(paint) = paint else { return };
    for e in events.read() {
        let GameEvent::Hit { shooter, at, .. } = *e else { continue };
        if let Some((b, _, _)) = balls.iter().filter(|(_, p, _)| p.shooter == shooter).min_by(|a, b| a.2.translation.distance(at).total_cmp(&b.2.translation.distance(at))) {
            commands.entity(b).despawn();
        }
        let team = game.member(shooter).map_or(0, |m| m.team);
        // a burst of paint: three splats facing about
        for (i, n) in [Vec3::X, Vec3::NEG_X, Vec3::Z].into_iter().enumerate() {
            splat(&mut commands, &paint, &mut splats, at + n * 0.05, n, team, 0.9 + 0.1 * i as f32);
        }
    }
}

fn fly(mut commands: Commands, time: Res<Time>, paint: Option<Res<Paint>>, mut splats: ResMut<Splats>, mut balls: Query<(Entity, &mut Paintball, &mut Transform)>) {
    let Some(paint) = paint else { return };
    let dt = time.delta_secs();
    for (e, mut b, mut t) in &mut balls {
        let step = dt.min(b.left);
        t.translation += b.vel * step - Vec3::Y * 0.5 * GRAVITY * step * step;
        b.vel.y -= GRAVITY * step;
        b.left -= step;
        if b.left <= 0.0 {
            if let Some((at, n)) = b.lands {
                // 0.8..1.2 of the disc (the gun's minMaxImpactDecalScale), varied by place
                let size = 0.8 + 0.4 * ((at.x * 12.9898 + at.z * 78.233).sin() * 0.5 + 0.5);
                splat(&mut commands, &paint, &mut splats, at, n, b.team, size);
            }
            commands.entity(e).despawn();
        }
    }
}

/// The bots: a Pip avatar each, with a gun in the right hand, moved as the server says.
#[allow(clippy::too_many_arguments)]
fn bots(
    mut commands: Commands,
    game: Res<Game>,
    kit: Option<Res<Kit>>,
    loader: Option<Res<world::Loader>>,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut avatars: Query<(Entity, &mut BotAvatar, &mut Transform, &mut Drive)>,
    mut guns: Query<&mut Transform, (Without<BotAvatar>, Without<Player>, Without<View>, Without<PaintballGun>)>,
) {
    let Some(kit) = kit else { return };
    let members: HashMap<i32, &rr_net::games::BotPose> = game.state.iter().flat_map(|s| s.members.iter()).filter_map(|m| m.bot.as_ref().map(|b| (m.id, b))).collect();
    let k = 1.0 - (-time.delta_secs() * 12.0).exp();
    let mut have = vec![];
    for (e, a, mut t, mut drive) in &mut avatars {
        let Some(b) = members.get(&a.id) else {
            if let Some(g) = a.gun {
                commands.entity(g).despawn();
            }
            commands.entity(e).despawn();
            continue;
        };
        have.push(a.id);
        t.translation = t.translation.lerp(Vec3::from(b.pos), k);
        t.rotation = t.rotation.slerp(Quat::from_rotation_y(b.yaw), k);
        drive.velocity = Vec3::from(b.vel);
        let hand = t.mul_transform(Transform::from_xyz(0.22, 1.1, -0.35));
        drive.hands = [None, Some(hand)];
        if let Some(mut g) = a.gun.and_then(|g| guns.get_mut(g).ok()) {
            *g = hand.with_translation(hand.translation + t.rotation * Vec3::new(0.0, 0.05, -0.1));
        }
    }
    for (&id, b) in members.iter().filter(|(id, _)| !have.contains(id)) {
        let at = Transform::from_translation(Vec3::from(b.pos)).with_rotation(Quat::from_rotation_y(b.yaw));
        let e = avatar::spawn(&mut commands, &kit, &mut materials, Look::for_name(&b.name), at, Some(&b.name), 50_000 + id.unsigned_abs());
        let gun = loader.as_deref().map(|l| world::spawn_prop(&mut commands, l, GUN, at));
        commands.entity(e).insert((BotAvatar { id, gun }, Name::new(format!("bot {}", b.name))));
    }
}

/// A team-coloured ball over everyone else's head (players and bots), while the room has a match.
fn markers(
    mut commands: Commands,
    game: Res<Game>,
    link: Res<RoomLink>,
    paint: Option<Res<Paint>>,
    mut marks: Query<(Entity, &Marker, &mut Transform, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility)>,
    bots: Query<(&BotAvatar, &Transform), Without<Marker>>,
) {
    let (Some(paint), Some(state)) = (paint, &game.state) else {
        for (e, ..) in &marks {
            commands.entity(e).despawn();
        }
        return;
    };
    let mut heads: HashMap<i32, Vec3> = HashMap::new();
    if let Some(rt) = link.get() {
        for s in rt.room.lock().unwrap().shown() {
            heads.insert(s.actor, Vec3::from(s.body.position));
        }
    }
    for (a, t) in &bots {
        heads.insert(a.id, t.translation);
    }
    let mut have = vec![];
    for (e, m, mut t, mut mat, mut vis) in &mut marks {
        match (heads.get(&m.0), state.members.iter().find(|x| x.id == m.0)) {
            (Some(&at), Some(member)) => {
                t.translation = at + Vec3::Y * 2.05;
                let want = &paint.team[member.team as usize & 1];
                if mat.0 != *want {
                    mat.0 = want.clone();
                }
                vis.set_if_neq(if member.out { Visibility::Hidden } else { Visibility::Inherited });
                have.push(m.0);
            }
            _ => commands.entity(e).despawn(),
        }
    }
    for m in state.members.iter().filter(|m| Some(m.id) != game.me && !have.contains(&m.id) && heads.contains_key(&m.id)) {
        commands.spawn((Mesh3d(paint.marker.clone()), MeshMaterial3d(paint.team[m.team as usize & 1].clone()),
            Transform::from_translation(heads[&m.id] + Vec3::Y * 2.05).with_scale(Vec3::new(1.0, 1.4, 1.0)), Visibility::default(), Marker(m.id)));
    }
}

/// Capture the Flag: each team's flag where the server has it, and a ring at each goal.
#[allow(clippy::too_many_arguments)]
fn flags(
    mut commands: Commands,
    game: Res<Game>,
    paint: Option<Res<Paint>>,
    mut flags: Query<(Entity, &FlagMark, &mut Transform), Without<GoalMark>>,
    goals: Query<Entity, With<GoalMark>>,
    time: Res<Time>,
) {
    let ctf = game.state.as_ref().filter(|s| s.mode == Mode::CaptureTheFlag && s.phase != Phase::Lobby);
    let (Some(paint), Some(s), Some(map)) = (paint, ctf, &game.map) else {
        for e in flags.iter().map(|f| f.0).chain(goals.iter()) {
            commands.entity(e).despawn();
        }
        return;
    };
    let k = 1.0 - (-time.delta_secs() * 10.0).exp();
    if flags.is_empty() {
        for team in 0..2u8 {
            let at = Vec3::from(s.flags[team as usize].at);
            let root = commands.spawn((Transform::from_translation(at), Visibility::default(), FlagMark(team), Name::new("flag"))).id();
            let pole = commands.spawn((Mesh3d(paint.pole.clone()), MeshMaterial3d(paint.pole_mat.clone()), Transform::from_xyz(0.0, 0.0, 0.0))).id();
            let cloth = commands.spawn((Mesh3d(paint.cloth.clone()), MeshMaterial3d(paint.team[team as usize].clone()), Transform::from_xyz(0.0, 0.95, 0.37))).id();
            commands.entity(root).add_children(&[pole, cloth]);
            let goal = Vec3::from(map.goals[team as usize]) + Vec3::Y * 0.05;
            commands.spawn((Mesh3d(paint.ring.clone()), MeshMaterial3d(paint.flat[team as usize].clone()),
                Transform::from_translation(goal).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)), GoalMark, Name::new("goal")));
        }
        return;
    }
    for (_, f, mut t) in &mut flags {
        let flag = s.flags[f.0 as usize];
        let want = Vec3::from(flag.at);
        t.translation = if (want - t.translation).length() > 5.0 { want } else { t.translation.lerp(want, k) };
        // a carried flag leans back over the carrier's shoulder
        t.rotation = if flag.carrier != 0 { Quat::from_rotation_x(0.35) } else { Quat::IDENTITY };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_lands_on_the_floor() {
        let floor = Obb { center: Vec3::new(0.0, -0.5, 0.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::new(50.0, 0.5, 50.0) };
        let (t, at, n) = impact(&[Collider::Box(floor)], Vec3::new(0.0, 1.5, 0.0), Vec3::new(0.0, 0.0, -SPEED), 3.0).unwrap();
        // falls 1.5 m in sqrt(2 * 1.5 / 9.81) = 0.553 s, 18.3 m away
        assert!((t - 0.553).abs() < 0.02, "{t}");
        assert!((at.z + 18.25).abs() < 0.5 && at.y.abs() < 0.02, "{at}");
        assert_eq!(n, Vec3::Y);
        let wall = Obb { center: Vec3::new(0.0, 1.0, -5.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::new(5.0, 3.0, 0.1) };
        let (_, at, n) = impact(&[Collider::Box(wall)], Vec3::new(0.0, 1.5, 0.0), Vec3::new(0.0, 0.0, -SPEED), 3.0).unwrap();
        assert!((at.z + 4.9).abs() < 0.01);
        assert_eq!(n, Vec3::Z);
    }
}
