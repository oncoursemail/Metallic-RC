//! Renders the robot avatar in a small bright room and saves screenshots of it: a line-up of looks, a turnaround,
//! the face (eyes following, blinking, talking), the hand grips, every accessory and outfit, and a dark room.
//! `cargo run --profile fast -p metallic-rc-client --example avatar_preview -- <out dir>`
use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, anim::Grip, Accessory, Drive, Eyes, Kit, Look, Outfit, Rig};

#[derive(Resource)]
struct Out(PathBuf);

/// The shots: a name, where the camera is and looks, and what the avatars do.
struct Shot {
    name: &'static str,
    eye: Vec3,
    at: Vec3,
    dark: bool,
    talk: bool,
    grip: Grip,
    turn: f32,
    /// ground speed (m/s) fed to the avatars so they walk/run in place (0 = standing)
    speed: f32,
}

const S: f32 = 0.0;
const SHOTS: &[Shot] = &[
    Shot { name: "01_lineup", eye: Vec3::new(0.0, 1.45, 4.3), at: Vec3::new(0.0, 0.9, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "02_hero", eye: Vec3::new(0.55, 1.35, 1.65), at: Vec3::new(0.0, 1.0, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "03_face", eye: Vec3::new(0.25, 1.6, 0.75), at: Vec3::new(0.0, 1.5, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "04_talking", eye: Vec3::new(-0.2, 1.6, 0.75), at: Vec3::new(0.0, 1.5, 0.0), dark: false, talk: true, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "05_side", eye: Vec3::new(2.0, 1.3, 0.0), at: Vec3::new(0.0, 0.95, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "06_back", eye: Vec3::new(0.7, 1.4, -2.0), at: Vec3::new(0.0, 0.95, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "07_point", eye: Vec3::new(0.15, 1.05, 0.95), at: Vec3::new(0.2, 0.85, 0.0), dark: false, talk: false, grip: Grip::Point, turn: 0.0, speed: S },
    Shot { name: "08_hold", eye: Vec3::new(0.15, 1.05, 0.95), at: Vec3::new(0.2, 0.85, 0.0), dark: false, talk: false, grip: Grip::Hold, turn: 0.0, speed: S },
    Shot { name: "09_dark_room", eye: Vec3::new(0.0, 1.45, 4.3), at: Vec3::new(0.0, 0.9, 0.0), dark: true, talk: true, grip: Grip::Open, turn: 0.0, speed: S },
    Shot { name: "10_lineup_turned", eye: Vec3::new(0.0, 1.45, 4.3), at: Vec3::new(0.0, 0.9, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.6, speed: S },
    Shot { name: "11_walk_side", eye: Vec3::new(2.0, 1.3, 0.0), at: Vec3::new(0.0, 0.95, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: 2.4 },
    Shot { name: "12_walk_hero", eye: Vec3::new(0.55, 1.35, 1.65), at: Vec3::new(0.0, 1.0, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: 2.4 },
    Shot { name: "13_run_side", eye: Vec3::new(2.0, 1.3, 0.0), at: Vec3::new(0.0, 0.95, 0.0), dark: false, talk: false, grip: Grip::Open, turn: 0.0, speed: 4.5 },
];

/// The looks in the line-up: every accessory, outfit and eye style at least once.
fn looks() -> Vec<Look> {
    let l = |skin: u32, shirt: u32, pants: u32, accent: u32, accessory, outfit, eyes| {
        let c = |v: u32| [(v >> 16) as u8, (v >> 8) as u8, v as u8];
        Look { skin: c(skin), shirt: c(shirt), pants: c(pants), accent: c(accent), visor: [0x0f, 0x11, 0x17], accessory, outfit, eyes }
    };
    vec![
        l(0xe8ecf1, 0x2b5fd9, 0x3a3f4b, 0x3df5ff, Accessory::Antenna, Outfit::None, Eyes::Pill),
        l(0xf2c99b, 0xe04f3c, 0x26324f, 0xffb32e, Accessory::Cap, Outfit::Scarf, Eyes::Round),
        l(0x8fd6c3, 0x7b45c9, 0xe9e7e1, 0xff5ca8, Accessory::Ears, Outfit::Bowtie, Eyes::Happy),
        l(0xf5d04a, 0x232a36, 0x3f6fb5, 0x9dff4a, Accessory::Headphones, Outfit::Backpack, Eyes::Wide),
        l(0xd99ac9, 0xf3f1ea, 0x5b3a6b, 0xb57dff, Accessory::Beanie, Outfit::None, Eyes::Pill),
        l(0x7a4e33, 0x2f9e63, 0xc9b48f, 0x3df5ff, Accessory::Sprout, Outfit::Scarf, Eyes::Round),
        l(0x9aa7b8, 0xf09a1c, 0x1b1d22, 0xff4545, Accessory::Goggles, Outfit::Backpack, Eyes::Pill),
        l(0xe8ecf1, 0x159aa8, 0x5d6b3a, 0xffffff, Accessory::Halo, Outfit::Bowtie, Eyes::Happy),
    ]
}

#[derive(Component)]
struct Bright;

#[derive(Component)]
struct Dim;

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "avatar_preview".into()));
    std::fs::create_dir_all(&out).unwrap();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: avatar preview".into(), resolution: (1280, 800).into(), ..default() }),
            ..default()
        }))
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(Out(out))
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .add_systems(Startup, room)
        .add_systems(Update, (spawn_avatars, run))
        .run();
}

/// A soft pastel room: floor, back and side walls, a big window light and a fill.
fn room(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mat = |m: &mut Assets<StandardMaterial>, c: Color| m.add(StandardMaterial { base_color: c, perceptual_roughness: 0.9, ..default() });
    let floor = mat(&mut materials, Color::srgb(0.86, 0.8, 0.72));
    let wall = mat(&mut materials, Color::srgb(0.95, 0.93, 0.9));
    let side = mat(&mut materials, Color::srgb(0.62, 0.8, 0.92));
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(14.0, 14.0))), MeshMaterial3d(floor)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(14.0, 5.0, 0.1))), MeshMaterial3d(wall.clone()), Transform::from_xyz(0.0, 2.5, -2.5)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(14.0, 5.0, 0.1))), MeshMaterial3d(wall), Transform::from_xyz(0.0, 2.5, 5.5)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(0.1, 5.0, 14.0))), MeshMaterial3d(side.clone()), Transform::from_xyz(-4.5, 2.5, 0.0)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(0.1, 5.0, 14.0))), MeshMaterial3d(side), Transform::from_xyz(4.5, 2.5, 0.0)));
    commands.spawn((DirectionalLight { illuminance: 6000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y), Bright));
    commands.spawn((PointLight { intensity: 300_000.0, range: 20.0, ..default() }, Transform::from_xyz(-2.0, 3.0, 3.0), Bright));
    commands.spawn((PointLight { intensity: 40_000.0, range: 10.0, color: Color::srgb(1.0, 0.75, 0.5), ..default() },
        Transform::from_xyz(1.5, 2.4, 1.5), Visibility::Hidden, Dim));
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 1.45, 4.3).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y)));
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
        // the first in the middle front (the one the close-ups look at), the rest in a shallow arc behind
        let at = if i == 0 { Vec3::ZERO } else {
            let k = i as f32 - 1.0 - (n - 2.0) / 2.0;
            Vec3::new(k * 0.82, 0.0, -0.9 - 0.12 * k.abs())
        };
        let name = format!("bot {i}");
        avatar::spawn(&mut commands, &kit, &mut materials, look, Transform::from_translation(at), Some(&name), 100 + i as u32);
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<Rig>)>,
    mut avatars: Query<(&mut Transform, &mut Drive), With<Rig>>,
    mut bright: Query<&mut Visibility, (With<Bright>, Without<Dim>)>,
    mut dim: Query<&mut Visibility, (With<Dim>, Without<Bright>)>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut exit: MessageWriter<AppExit>,
    mut step: Local<(usize, f32)>,
) {
    // each shot: set up, wait for the eyes and grips to settle, take it, move on
    let (i, since) = &mut *step;
    *since += time.delta_secs();
    let Some(shot) = SHOTS.get(*i) else {
        if *since > 1.5 {
            exit.write(AppExit::Success);
        }
        return;
    };
    **camera = Transform::from_translation(shot.eye).looking_at(shot.at, Vec3::Y);
    for mut v in &mut bright {
        *v = if shot.dark { Visibility::Hidden } else { Visibility::Inherited };
    }
    for mut v in &mut dim {
        *v = if shot.dark { Visibility::Inherited } else { Visibility::Hidden };
    }
    ambient.brightness = if shot.dark { 60.0 } else { 400.0 };
    for (k, (mut tr, mut drive)) in avatars.iter_mut().enumerate() {
        // they face -z: turned round to face the camera
        tr.rotation = Quat::from_rotation_y(std::f32::consts::PI + shot.turn * if k % 2 == 0 { 1.0 } else { -1.0 });
        drive.talking = shot.talk && k % 3 == 0;
        drive.grip = [shot.grip; 2];
        // walk/run in place: a ground velocity drives the stride (they don't translate here)
        drive.velocity = Vec3::new(shot.speed, 0.0, 0.0);
        drive.grounded = Some(true);
    }
    // the first shot waits for the room and avatars to load
    let wait = if *i == 0 { 3.0 } else { 1.2 };
    if *since > wait {
        let path = out.0.join(format!("{}.png", shot.name));
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        *i += 1;
        *since = 0.0;
    }
}
