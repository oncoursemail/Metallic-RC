//! Full-body look at the DEFAULT character (Look::default) so we can see the jersey + chest logo + face together.
//! `METALLIC_BODY=full cargo run --profile fast -p metallic-rc-client --example char_preview -- <out dir>`
use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, Kit, Look, Rig};

#[derive(Resource)]
struct Out(PathBuf);

struct Shot {
    name: &'static str,
    eye: Vec3,
    at: Vec3,
}

const SHOTS: &[Shot] = &[
    Shot { name: "01_front", eye: Vec3::new(0.0, 0.95, 2.95), at: Vec3::new(0.0, 0.92, 0.0) },
    Shot { name: "02_threequarter", eye: Vec3::new(1.7, 1.15, 2.35), at: Vec3::new(0.0, 0.9, 0.0) },
    Shot { name: "03_chest", eye: Vec3::new(0.0, 1.12, 1.35), at: Vec3::new(0.0, 1.02, 0.0) },
];

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "char_preview".into()));
    std::fs::create_dir_all(&out).unwrap();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: character".into(), resolution: (900, 1100).into(), ..default() }),
            ..default()
        }))
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(Out(out))
        .insert_resource(GlobalAmbientLight { brightness: 500.0, ..default() })
        .add_systems(Startup, scene)
        .add_systems(Update, (spawn_one, run))
        .run();
}

fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let floor = materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.88, 0.85), perceptual_roughness: 0.95, ..default() });
    let wall = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.93, 0.9), perceptual_roughness: 0.95, ..default() });
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(14.0, 14.0))), MeshMaterial3d(floor)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(14.0, 7.0, 0.1))), MeshMaterial3d(wall), Transform::from_xyz(0.0, 3.0, -2.0)));
    commands.spawn((DirectionalLight { illuminance: 6500.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(2.0, 5.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((PointLight { intensity: 180_000.0, range: 14.0, ..default() }, Transform::from_xyz(-1.5, 2.5, 3.0)));
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.95, 2.95).looking_at(Vec3::new(0.0, 0.92, 0.0), Vec3::Y)));
}

fn spawn_one(mut commands: Commands, kit: Option<Res<Kit>>, mut materials: ResMut<Assets<StandardMaterial>>, mut done: Local<bool>) {
    let Some(kit) = kit else { return };
    if *done {
        return;
    }
    *done = true;
    // the actual new-player default look
    avatar::spawn(&mut commands, &kit, &mut materials, Look::default(), Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)), None, 7);
}

fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<Rig>)>,
    mut exit: MessageWriter<AppExit>,
    mut step: Local<(usize, f32)>,
) {
    let (i, since) = &mut *step;
    *since += time.delta_secs();
    let Some(shot) = SHOTS.get(*i) else {
        if *since > 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    };
    **camera = Transform::from_translation(shot.eye).looking_at(shot.at, Vec3::Y);
    let wait = if *i == 0 { 3.5 } else { 1.2 };
    if *since > wait {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join(format!("{}.png", shot.name))));
        *i += 1;
        *since = 0.0;
    }
}
