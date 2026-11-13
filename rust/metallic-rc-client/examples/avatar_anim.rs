//! Drives the avatar through its locomotion states (idle, walk, run, jump, fall) and saves 3/4-view shots, so we can see
//! the skeleton animate — with METALLIC_ANIM_CORPUS set, that's the REAL authored Rec Room clips; otherwise the
//! procedural poser. A short burst on walk/run catches the stride mid-swing.
//! `METALLIC_BODY=full METALLIC_ANIM_CORPUS=<decrypted resources.assets> cargo run --profile fast -p metallic-rc-client
//! --example avatar_anim -- <out dir>`
use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, Drive, Kit, Look, Rig};

#[derive(Resource)]
struct Out(PathBuf);

struct Shot {
    name: &'static str,
    vel: Vec3,
    grounded: Option<bool>,
    emote: Option<&'static str>,
    dwell: f32,
    burst: u8,
}

const Q: Vec3 = Vec3::new(2.3, 1.45, 2.4);
const LOOKM: Vec3 = Vec3::new(0.0, 0.9, 0.0);
const BURST_GAP: f32 = 0.2;

const SHOTS: &[Shot] = &[
    Shot { name: "01_idle", vel: Vec3::ZERO, grounded: Some(true), emote: None, dwell: 1.0, burst: 1 },
    // a quick walk burst right after idle also catches the idle->walk crossfade in the first frame
    Shot { name: "02_walk", vel: Vec3::new(3.0, 0.0, 0.0), grounded: Some(true), emote: None, dwell: 0.08, burst: 3 },
    Shot { name: "04_run", vel: Vec3::new(4.6, 0.0, 0.0), grounded: Some(true), emote: None, dwell: 0.5, burst: 3 },
    Shot { name: "05_jump", vel: Vec3::new(0.5, 4.0, 0.0), grounded: Some(false), emote: None, dwell: 0.4, burst: 2 },
    Shot { name: "06_fall", vel: Vec3::new(0.5, -3.5, 0.0), grounded: Some(false), emote: None, dwell: 0.4, burst: 2 },
    Shot { name: "07_emote_sit", vel: Vec3::ZERO, grounded: Some(true), emote: Some("sit"), dwell: 0.8, burst: 1 },
    Shot { name: "08_emote_prone", vel: Vec3::ZERO, grounded: Some(true), emote: Some("prone"), dwell: 0.8, burst: 1 },
    Shot { name: "09_emote_hips", vel: Vec3::ZERO, grounded: Some(true), emote: Some("hips"), dwell: 0.8, burst: 1 },
    Shot { name: "10_emote_flex", vel: Vec3::ZERO, grounded: Some(true), emote: Some("flex"), dwell: 0.8, burst: 1 },
    Shot { name: "11_emote_cute", vel: Vec3::ZERO, grounded: Some(true), emote: Some("cute"), dwell: 0.8, burst: 1 },
];

fn looks() -> Vec<Look> {
    let c = |v: u32| [(v >> 16) as u8, (v >> 8) as u8, v as u8];
    vec![
        Look { skin: c(0xe8ecf1), shirt: c(0x2b5fd9), pants: c(0x3a3f4b), ..default() },
        Look { skin: c(0xf2c99b), shirt: c(0xe04f3c), pants: c(0x26324f), ..default() },
        Look { skin: c(0x8fd6c3), shirt: c(0x7b45c9), pants: c(0xe9e7e1), ..default() },
    ]
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "avatar_anim".into()));
    std::fs::create_dir_all(&out).unwrap();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: avatar anim".into(), resolution: (1280, 800).into(), ..default() }),
            ..default()
        }))
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(Out(out))
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, scene)
        .add_systems(Update, (spawn_avatars, run))
        .run();
}

fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let floor = materials.add(StandardMaterial { base_color: Color::srgb(0.86, 0.8, 0.72), perceptual_roughness: 0.9, ..default() });
    let wall = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.93, 0.9), perceptual_roughness: 0.9, ..default() });
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(14.0, 14.0))), MeshMaterial3d(floor)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(14.0, 5.0, 0.1))), MeshMaterial3d(wall), Transform::from_xyz(0.0, 2.5, -2.5)));
    commands.spawn((DirectionalLight { illuminance: 6000.0, ..default() }, Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((PointLight { intensity: 300_000.0, range: 20.0, ..default() }, Transform::from_xyz(-2.0, 3.0, 3.0)));
    commands.spawn((Camera3d::default(), Transform::from_translation(Q).looking_at(LOOKM, Vec3::Y)));
}

fn spawn_avatars(mut commands: Commands, kit: Option<Res<Kit>>, mut materials: ResMut<Assets<StandardMaterial>>, mut done: Local<bool>) {
    let Some(kit) = kit else { return };
    if *done {
        return;
    }
    *done = true;
    let all = looks();
    let n = all.len() as f32;
    for (i, look) in all.into_iter().enumerate() {
        let x = (i as f32 - (n - 1.0) / 2.0) * 0.95;
        avatar::spawn(&mut commands, &kit, &mut materials, look, Transform::from_xyz(x, 0.0, 0.0), None, 20 + i as u32);
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    mut avatars: Query<(&mut Transform, &mut Drive), With<Rig>>,
    mut exit: MessageWriter<AppExit>,
    mut step: Local<(usize, f32, u8)>,
) {
    let (i, since, taken) = &mut *step;
    *since += time.delta_secs();
    let Some(shot) = SHOTS.get(*i) else {
        if *since > 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    };
    for (mut tr, mut drive) in &mut avatars {
        tr.rotation = Quat::from_rotation_y(std::f32::consts::PI); // face the camera
        drive.velocity = shot.vel;
        drive.grounded = shot.grounded;
        drive.flying = false;
        drive.emote = shot.emote;
    }
    let wait = if *i == 0 { 3.0 } else if *taken == 0 { shot.dwell } else { BURST_GAP };
    if *since > wait {
        let name = if shot.burst > 1 { format!("{}_{}.png", shot.name, (b'a' + *taken) as char) } else { format!("{}.png", shot.name) };
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join(name)));
        *taken += 1;
        *since = 0.0;
        if *taken >= shot.burst {
            *i += 1;
            *taken = 0;
        }
    }
}
