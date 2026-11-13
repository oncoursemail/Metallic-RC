//! Tight close-up of the avatar's face, to dial in the eye/mouth decals.
//! `cargo run --profile fast -p metallic-rc-client --example facecam -- <out dir>`
use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, Kit, Look};

#[derive(Resource)]
struct Out(PathBuf);

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "facecam".into()));
    std::fs::create_dir_all(&out).unwrap();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: face".into(), resolution: (800, 800).into(), ..default() }),
            ..default()
        }))
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(Out(out))
        .insert_resource(GlobalAmbientLight { brightness: 600.0, ..default() })
        .add_systems(Startup, scene)
        .add_systems(Update, (spawn_one, run))
        .run();
}

fn scene(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>, mut meshes: ResMut<Assets<Mesh>>) {
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(6.0, 6.0, 0.1))), MeshMaterial3d(materials.add(Color::srgb(0.9, 0.9, 0.92))), Transform::from_xyz(0.0, 1.5, -1.0)));
    commands.spawn((DirectionalLight { illuminance: 6000.0, ..default() }, Transform::from_xyz(1.0, 3.0, 4.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y)));
    // eye-level, close in on the face (full-body head ~1.42, eyes ~1.52)
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 1.52, 0.72).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y)));
}

fn spawn_one(mut commands: Commands, kit: Option<Res<Kit>>, mut materials: ResMut<Assets<StandardMaterial>>, mut done: Local<bool>) {
    let Some(kit) = kit else { return };
    if *done {
        return;
    }
    *done = true;
    avatar::spawn(&mut commands, &kit, &mut materials, Look::default(), Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)), None, 7);
}

fn run(mut commands: Commands, time: Res<Time>, out: Res<Out>, mut exit: MessageWriter<AppExit>, mut shot: Local<bool>, mut t: Local<f32>) {
    *t += time.delta_secs();
    if !*shot && *t > 4.0 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join("face.png")));
        *shot = true;
    }
    if *shot && *t > 5.3 {
        exit.write(AppExit::Success);
    }
}
