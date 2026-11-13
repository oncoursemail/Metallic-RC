//! Close-up of the real Rec Room face so we can see it animate: the resting face, the talking visemes (mouth cycling
//! through the talking shapes), and a real blink (the shot is timed to the blink the face system actually produces).
//! Run the Full body for a head at a known height: `METALLIC_BODY=full cargo run --profile fast -p metallic-rc-client
//! --example avatar_face -- <out dir>`
use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use metallic_rc_client::avatar::{self, Drive, Kit, Look, Rig};

#[derive(Resource)]
struct Out(PathBuf);

/// The avatar's blink seed (matches `real::animate_face`'s clock), so we can snap a shot exactly when it blinks.
const SEED: u32 = 7;

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "avatar_face".into()));
    std::fs::create_dir_all(&out).unwrap();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: avatar face".into(), resolution: (900, 900).into(), ..default() }),
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
    let wall = materials.add(StandardMaterial { base_color: Color::srgb(0.93, 0.92, 0.9), perceptual_roughness: 0.95, ..default() });
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(6.0, 6.0, 0.1))), MeshMaterial3d(wall), Transform::from_xyz(0.0, 1.5, -0.6)));
    commands.spawn((DirectionalLight { illuminance: 6500.0, ..default() }, Transform::from_xyz(1.0, 3.0, 4.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y)));
    commands.spawn((PointLight { intensity: 120_000.0, range: 12.0, ..default() }, Transform::from_xyz(-1.0, 2.0, 2.0)));
    // tight on the face (Full body head ~1.52 m)
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 1.52, 0.62).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y)));
}

fn spawn_one(mut commands: Commands, kit: Option<Res<Kit>>, mut materials: ResMut<Assets<StandardMaterial>>, mut done: Local<bool>) {
    let Some(kit) = kit else { return };
    if *done {
        return;
    }
    *done = true;
    let look = Look { skin: [0xf2, 0xc9, 0x9b], shirt: [0x2b, 0x5f, 0xd9], ..default() };
    avatar::spawn(&mut commands, &kit, &mut materials, look, Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)), None, SEED);
}

/// Is the face blinking now, per `real::animate_face`'s own clock (so we can snap during a real blink).
fn blinking(t: f32) -> bool {
    let period = 3.2 + (SEED % 7) as f32 * 0.45;
    (t + (SEED as f32 * 0.613).rem_euclid(period)).rem_euclid(period) < 0.12
}

/// Shots: rest, three talking frames (different visemes), then a frame snapped during a real blink. `state`: which shot
/// is next; `clock`: time in the current shot.
fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    mut avatars: Query<&mut Drive, With<Rig>>,
    mut exit: MessageWriter<AppExit>,
    mut state: Local<u32>,
    mut clock: Local<f32>,
) {
    *clock += time.delta_secs();
    let t = time.elapsed_secs();
    let snap = |commands: &mut Commands, name: &str| {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join(format!("{name}.png"))));
    };
    match *state {
        // wait for the body + face to load, then the resting face
        0 => {
            if *clock > 3.5 {
                snap(&mut commands, "01_rest");
                *state = 1;
                *clock = 0.0;
            }
        }
        // talking: cycle visemes; snap three frames 0.12 s apart
        1 | 2 | 3 => {
            for mut d in &mut avatars {
                d.talking = true;
            }
            if *clock > 0.12 {
                snap(&mut commands, &format!("0{}_talk", *state + 1));
                *state += 1;
                *clock = 0.0;
            }
        }
        // stop talking, then snap the next real blink
        4 => {
            for mut d in &mut avatars {
                d.talking = false;
            }
            if *clock > 0.3 && blinking(t) {
                snap(&mut commands, "05_blink");
                *state = 5;
                *clock = 0.0;
            }
        }
        _ => {
            if *clock > 0.6 {
                exit.write(AppExit::Success);
            }
        }
    }
}
