//! Proof of the Customize click->avatar wiring: spawns the real body, then at t=4s mutates the live `Dressing` the exact
//! way a Customize click does (new top/bottom/shoes/hat meshes, new hair style, recoloured skin+hair), so `redress`
//! swaps them on the standing avatar. Shoots "before" (default) and "after" (re-dressed).
//! `cargo run --profile fast -p metallic-rc-client --example wear_preview -- <out dir>`
use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, Dressing, Kit, Look};

#[derive(Resource)]
struct Out(PathBuf);

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "wear_preview".into()));
    std::fs::create_dir_all(&out).unwrap();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: wear".into(), resolution: (900, 1100).into(), ..default() }),
            ..default()
        }))
        .add_plugins(avatar::AvatarPlugin)
        .insert_resource(Out(out))
        // the Customize screen edits this; mutating it drives `redress` exactly as a click would
        .insert_resource(Dressing::default())
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
    avatar::spawn(&mut commands, &kit, &mut materials, Look::default(), Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)), None, 7);
}

fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    mut dress: ResMut<Dressing>,
    mut exit: MessageWriter<AppExit>,
    mut step: Local<usize>,
    mut t: Local<f32>,
) {
    *t += time.delta_secs();
    match *step {
        0 if *t > 4.0 => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join("before.png")));
            *step = 1;
        }
        1 if *t > 4.3 => {
            // exactly what the Customize clicks do: a new outfit + colours
            dress.top = "HockeySet_Shirt".into();
            dress.bottom = "Sweatpants_Leg".into();
            dress.shoes = "Boots_Shoe".into();
            dress.hat = "BrimlessCap_Hat".into();
            dress.hair_style = dress.hair_styles.iter().find(|s| *s == "Afro").cloned().unwrap_or_else(|| dress.hair_style.clone());
            dress.hair_color = [0xff, 0x3d, 0x9a]; // hot pink
            dress.skin = [0x7a, 0x4a, 0x2b]; // a deeper skin tone
            *step = 2;
        }
        2 if *t > 7.5 => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join("after.png")));
            *step = 3;
        }
        3 if *t > 8.8 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
