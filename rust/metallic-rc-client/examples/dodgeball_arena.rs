//! Dodgeball in the REAL arena: loads the game's own `dodgeball` scene — decrypted `level11`,
//! `Packages/com.recroom.legacy/Runtime/Assets/Activities/Dodgeball/Scenes/dodgeball.unity` — with the real world
//! loader (geometry, materials, lights, colliders, the scene's own spawn points), then places avatars as the two
//! teams, throws a ball on a ballistic arc, and runs the ported `gamemodes::DodgeballManager` so a thrown ball knocks
//! a player out and the manager scores the round. This is the first built-in RR Original running in its ACTUAL
//! environment in the Rust client — not a void.
//!
//! The rules come from the ported manager (its method bodies aren't in the IL2CPP dump, so they're behavior-modeled
//! and marked `// 1:1:`); the arena is the game's real asset. Plays no audio (muted, per the owner's standing
//! request). The full in-app version runs the same manager over rr-net with the real DodgeballBall prop + grab/throw.
//! `cargo run --profile fast -p metallic-rc-client --example dodgeball_arena -- [out dir]`

use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use gamemodes::dodgeball::{DodgeballConfig, DodgeballEffects, DodgeballManager, Phase, PlayerId};
use metallic_rc_client::{
    audio,
    avatar::{self, Kit, Look, Rig},
    levels, room, world,
};

const GRAVITY: f32 = 9.81;
const THROW_SPEED: f32 = 9.0; // horizontal, ~ the client's prop throw (ThrowToolBehaviorConfig 8 m/s)
const BALL_R: f32 = 0.16; // ~ the real DodgeballBall; a touch large so it reads in a wide shot
const TORSO: f32 = 1.0; // a player's torso height, for the hit test

#[derive(Resource)]
struct Out(PathBuf);

/// The two teams' spawn spots in the real arena, found from the scene's spawn points.
#[derive(Resource)]
struct Spawns {
    /// team 0 (RED) and team 1 (BLUE) floor positions
    teams: [Vec<Vec3>; 2],
    /// camera eye + target that frame the court
    eye: Vec3,
    target: Vec3,
}

/// The running game once the arena is in and the players are spawned.
#[derive(Resource)]
struct Game {
    mgr: DodgeballManager,
    thrower: PlayerId,
    /// player id -> (avatar entity, spawn position)
    avatars: Vec<(PlayerId, Entity, Vec3)>,
    throw_cd: f32,
    ball: Option<Entity>,
    toppled: Vec<PlayerId>,
    shot: usize,
    t: f32,
}

#[derive(Component)]
struct Ball {
    vel: Vec3,
    thrower: PlayerId,
}

#[derive(Default)]
struct Fx;
impl DodgeballEffects for Fx {}

fn col(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "dodgeball_arena".into()));
    std::fs::create_dir_all(&out).unwrap();

    // Load the real Dodgeball arena from the decrypted corpus (level11). `.resS` streams are read from the game
    // install (plaintext there).
    let game = room::GAME_DIRS.iter().map(PathBuf::from).find(|p| p.exists());
    let data = game.as_deref().map(|g| g.join("RecRoom_Data"));
    let r = levels::load_scene(data.as_deref(), "dodgeball")
        .expect("no `dodgeball` scene in the decrypted corpus — is the corpus present?")
        .expect("dodgeball arena failed to load");
    eprintln!(
        "dodgeball arena: {} parts, {} materials, {} lights, {} colliders, {} spawns",
        r.parts.len(), r.materials.len(), r.lights.len(), r.colliders.len(), r.spawns.len()
    );

    // Work out the two teams' spots and a framing camera from the scene's own spawn points + bounds.
    let spawns = team_spawns(&r);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: Dodgeball (real arena)".into(), resolution: (1280, 720).into(), ..default() }),
            ..default()
        }))
        .add_plugins(world::WorldPlugin)
        .add_plugins(metallic_rc_client::particles::ParticlePlugin)
        .add_plugins(avatar::AvatarPlugin)
        // muted: master/music/sfx all 0 (owner's standing request)
        .insert_resource(audio::Volumes { master: 0.0, music: 0.0, sfx: 0.0 })
        .insert_resource(Out(out))
        .insert_resource(spawns)
        .insert_resource(PendingRoom(Some(r)))
        .add_systems(Startup, place_camera)
        .add_systems(Update, (hand_room_over, start_game, run).chain())
        .run();
}

/// Holds the loaded arena until the first frame, when it's handed to `world::WorldPlugin` to build.
#[derive(Resource)]
struct PendingRoom(Option<room::RoomData>);

/// Sets up a Dodgeball match in the real arena: two teams of three on either side of the court centre, facing off
/// down the court's long axis, with a camera looking down that axis from near one end. The scene ships 20 spawn
/// points, but they're a mix (team spawns, entrances, spectator spots) with no team labels in `RoomData`, so a
/// centred symmetric layout is the faithful representation of a round; the networked version uses the real spawns.
fn team_spawns(r: &room::RoomData) -> Spawns {
    let (lo, hi) = world::bounds(r);
    let center = (lo + hi) * 0.5;
    let size = hi - lo;
    let floor = lo.y;
    // teams face off down the longer horizontal axis of the court
    let long_z = size.z >= size.x;
    let half_len = (if long_z { size.z } else { size.x }) * 0.5;
    let gap = (half_len * 0.3).clamp(4.0, 8.0); // each team this far from the centre line
    let spread = 3.0_f32; // sideways spacing between a team's three players

    let team_at = |side: f32| -> Vec<Vec3> {
        (-1..=1)
            .map(|i| {
                let lateral = i as f32 * spread;
                if long_z {
                    Vec3::new(center.x + lateral, floor, center.z + side * gap)
                } else {
                    Vec3::new(center.x + side * gap, floor, center.z + lateral)
                }
            })
            .collect()
    };
    let teams = [team_at(1.0), team_at(-1.0)];

    // camera a little behind the near team, low and close, looking down the long axis at the centre line, so both
    // teams, the thrown ball and a player going out all read clearly (roughly a player's-eye view of the round)
    let end = gap + 5.0;
    let eye = if long_z {
        Vec3::new(center.x, floor + 2.3, center.z + end)
    } else {
        Vec3::new(center.x + end, floor + 2.3, center.z)
    };
    let target = Vec3::new(center.x, floor + 1.1, center.z - gap * 0.3);
    Spawns { teams, eye, target }
}

fn place_camera(mut commands: Commands, spawns: Res<Spawns>) {
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 60f32.to_radians(), near: 0.06, far: 1000.0, ..default() }),
        Transform::from_translation(spawns.eye).looking_at(spawns.target, Vec3::Y),
    ));
}

/// Hands the loaded arena to WorldPlugin on the first frame (it builds meshes/materials/lights + a sky).
fn hand_room_over(mut pending: ResMut<PendingRoom>, mut arrived: ResMut<world::Arrived>) {
    if let Some(r) = pending.0.take() {
        arrived.0 = Some(r);
    }
}

/// Once the arena is built (its pieces exist) and the avatar kit is ready, spawn the two teams and start the manager.
#[allow(clippy::too_many_arguments)]
fn start_game(
    mut commands: Commands,
    kit: Option<Res<Kit>>,
    spawns: Res<Spawns>,
    pieces: Query<(), With<world::RoomPiece>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game: Option<Res<Game>>,
    mut done: Local<bool>,
) {
    if *done || game.is_some() {
        return;
    }
    let (Some(kit), false) = (kit, pieces.is_empty()) else { return };
    *done = true;

    let mut mgr = DodgeballManager::new(DodgeballConfig { score_to_win: 1, pre_round_duration: 1.0, post_round_duration: 3.0 });
    let mut avatars = Vec::new();

    let looks = [
        Look { skin: col(0xe8ecf1), shirt: col(0xd1343a), pants: col(0x3a3f4b), ..default() }, // RED
        Look { skin: col(0xf2c99b), shirt: col(0x2b5fd9), pants: col(0x26324f), ..default() }, // BLUE
    ];
    // face the opposing side
    let a0 = spawns.teams[0].first().copied().unwrap_or(Vec3::ZERO);
    let b0 = spawns.teams[1].first().copied().unwrap_or(Vec3::X);
    let mut next_id: u32 = 1;
    for team in 0..2 {
        let toward = if team == 0 { b0 } else { a0 };
        for pos in spawns.teams[team].iter().copied() {
            let id = next_id;
            next_id += 1;
            let dir = (toward - pos) * Vec3::new(1.0, 0.0, 1.0);
            let yaw = if dir.length_squared() > 1e-4 { f32::atan2(-dir.x, -dir.z) } else { 0.0 };
            let tr = Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw));
            let e = avatar::spawn(&mut commands, &kit, &mut materials, looks[team], tr, None, id);
            mgr.add_player(id, team as u8);
            avatars.push((id, e, pos));
        }
    }
    let thrower = avatars.first().map(|(id, _, _)| *id).unwrap_or(1);
    for (id, _, p) in &avatars {
        eprintln!("  player {id} at ({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
    }
    commands.insert_resource(Game { mgr, thrower, avatars, throw_cd: 0.0, ball: None, toppled: Vec::new(), shot: 0, t: 0.0 });
    eprintln!("dodgeball: {} players spawned in the real arena", next_id - 1);
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    game: Option<ResMut<Game>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut balls: Query<(Entity, &mut Transform, &mut Ball), Without<Rig>>,
    mut avatars: Query<&mut Transform, With<Rig>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut game) = game else { return };
    let dt = time.delta_secs().min(0.05);
    game.t += dt;
    let mut fx = Fx;
    game.mgr.tick(dt, &mut fx);

    // move the in-flight ball; ground-miss despawns; contact with an alive target raises the hit
    let mut hit: Option<(PlayerId, PlayerId, Entity)> = None;
    for (e, mut tr, mut ball) in &mut balls {
        ball.vel.y -= GRAVITY * dt;
        tr.translation += ball.vel * dt;
        let floor = game.avatars.first().map(|(_, _, p)| p.y).unwrap_or(0.0);
        if tr.translation.y <= floor + BALL_R {
            commands.entity(e).despawn();
            if game.ball == Some(e) {
                game.ball = None;
            }
            continue;
        }
        for &(id, _ent, pos) in &game.avatars {
            if id == ball.thrower {
                continue;
            }
            if game.mgr.players.get(&id).map(|p| p.alive) == Some(true)
                && tr.translation.distance(pos + Vec3::Y * TORSO) < 0.6
            {
                hit = Some((id, ball.thrower, e));
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    if let Some((victim, thrower, e)) = hit {
        game.mgr.on_player_hit(victim, thrower, &mut fx);
        commands.entity(e).despawn();
        game.ball = None;
    }

    // topple newly-eliminated players (a clear "out" read)
    let outs: Vec<(PlayerId, Entity)> = game
        .avatars
        .iter()
        .filter(|(id, _, _)| game.mgr.players.get(id).map(|p| !p.alive) == Some(true) && !game.toppled.contains(id))
        .map(|(id, e, _)| (*id, *e))
        .collect();
    for (id, e) in outs {
        if let Ok(mut tr) = avatars.get_mut(e) {
            tr.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) * tr.rotation;
            tr.translation.y += 0.15;
        }
        game.toppled.push(id);
    }

    // throw at the next alive target while the round runs and no ball is in the air
    game.throw_cd -= dt;
    if game.mgr.is_round_running() && game.ball.is_none() && game.throw_cd <= 0.0 {
        let thrower_pos = game.avatars.iter().find(|(id, _, _)| *id == game.thrower).map(|(_, _, p)| *p).unwrap_or(Vec3::ZERO);
        if let Some(&(_, _, target_pos)) =
            game.avatars.iter().find(|(id, _, _)| *id != game.thrower && game.mgr.players.get(id).map(|p| p.alive) == Some(true))
        {
            let start = thrower_pos + Vec3::new(0.0, 1.3, 0.0);
            let target = target_pos + Vec3::Y * TORSO;
            let flat = Vec3::new(target.x - start.x, 0.0, target.z - start.z);
            let dist = flat.length().max(0.01);
            let tflight = dist / THROW_SPEED;
            let vy = (target.y - start.y + 0.5 * GRAVITY * tflight * tflight) / tflight;
            let vel = flat / dist * THROW_SPEED + Vec3::Y * vy;
            let ball_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.85, 0.2), perceptual_roughness: 0.5, ..default() });
            let e = commands
                .spawn((Mesh3d(meshes.add(Sphere::new(BALL_R).mesh().uv(16, 12))), MeshMaterial3d(ball_mat), Transform::from_translation(start), Ball { vel, thrower: game.thrower }))
                .id();
            game.ball = Some(e);
            game.throw_cd = 1.6;
        }
    }

    // shots: the arena with both teams, a ball mid-flight, a hit/topple, and the finished round
    let want = [(2.5_f32, "01_arena"), (3.5, "02_throw"), (4.7, "03_hit"), (7.0, "04_round_won")];
    if let Some(&(_, name)) = want.get(game.shot).filter(|(at, _)| game.t >= *at) {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join(format!("{name}.png"))));
        game.shot += 1;
    }
    if matches!(game.mgr.phase, Phase::GameOver(_)) && game.t > 8.0 {
        exit.write(AppExit::Success);
    }
    if game.t > 20.0 {
        exit.write(AppExit::Success);
    }
}
