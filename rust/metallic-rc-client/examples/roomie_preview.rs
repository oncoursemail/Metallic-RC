//! Loads the REAL Rec Room avatar body mesh (`BodyMesh_LOD0`) straight from the owned game bundle and renders it, so
//! we can see the real character we are switching Pip over to. Milestone 1: geometry + a plain skin material, no
//! skinning/animation yet.
//!
//! `cargo run --profile fast -p metallic-rc-client --example roomie_preview -- <out dir> [bundle] [pathid]`
use std::path::PathBuf;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use unity_assets::{Bundle, Mesh as UMesh};

/// Default: the avatar skin bundle in the owned 2026 install, and the BodyMesh_LOD0 object inside it.
const GAME_BUNDLE: &str = r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711\RecRoom_Data\StreamingAssets\aa\StandaloneWindows64\edf7f2d64a5698a6cec7ec8a1f821ab8.bundle";
const BODY_MESH_PATHID: i64 = 8332635914843264857;

#[derive(Resource)]
struct Out(PathBuf);

#[derive(Component)]
struct Model;

/// Loads `pathid` from `bundle` as a Bevy mesh in Bevy space (Unity is left-handed: z negated, winding flipped, uv v
/// flipped — same convention as `room.rs`). Returns the mesh and its feet-to-floor offset + half-height for framing.
fn load_mesh(bundle: &str, pathid: i64) -> (Mesh, f32, f32) {
    let b = Bundle::open(std::path::Path::new(bundle)).unwrap_or_else(|e| panic!("open {bundle}: {e}"));
    let files = b.serialized_files().expect("serialized files");
    let (_, sf) = files.iter().find(|(_, sf)| sf.object(pathid).is_some()).expect("object not in bundle");
    let o = sf.object(pathid).unwrap();
    let v = sf.read_value(o).expect("read value");
    let m = UMesh::from_value(&v, &b).expect("mesh from value");
    println!("loaded {} ({} verts, {} submeshes)", m.name, m.vertex_count, m.submeshes.len());

    let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
    let positions: Vec<[f32; 3]> = m.positions().into_iter().map(flip).collect();
    let mut normals: Vec<[f32; 3]> = m.normals().into_iter().map(flip).collect();
    normals.resize(positions.len(), [0.0, 1.0, 0.0]);
    let mut uvs = m.uv(0);
    uvs.resize(positions.len(), [0.0, 0.0]);
    let uvs: Vec<[f32; 2]> = uvs.into_iter().map(|[u, v]| [u, 1.0 - v]).collect();

    let mut indices: Vec<u32> = Vec::new();
    for (i, sub) in m.submeshes.iter().enumerate() {
        if sub.topology != 0 {
            continue;
        }
        if let Some(mut idx) = m.submesh_indices(i) {
            for tri in idx.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
            indices.extend_from_slice(&idx);
        }
    }

    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for p in &positions {
        lo = lo.min(p[1]);
        hi = hi.max(p[1]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    (mesh, -lo, (hi - lo) * 0.5)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "roomie_preview".into()));
    let bundle = args.next().unwrap_or_else(|| GAME_BUNDLE.into());
    let pathid: i64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(BODY_MESH_PATHID);
    std::fs::create_dir_all(&out).unwrap();

    let (mesh, foot_offset, half) = load_mesh(&bundle, pathid);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Metallic RC: real Roomie".into(), resolution: (1280, 800).into(), ..default() }),
            ..default()
        }))
        .insert_resource(Out(out))
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .insert_resource(Loaded { mesh: Some(mesh), foot_offset, half })
        .add_systems(Startup, setup)
        .add_systems(Update, run)
        .run();
}

#[derive(Resource)]
struct Loaded {
    mesh: Option<Mesh>,
    foot_offset: f32,
    half: f32,
}

fn setup(
    mut commands: Commands,
    mut loaded: ResMut<Loaded>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // soft pastel room
    let mat = |m: &mut Assets<StandardMaterial>, c: Color| m.add(StandardMaterial { base_color: c, perceptual_roughness: 0.9, ..default() });
    let floor = mat(&mut materials, Color::srgb(0.86, 0.8, 0.72));
    let wall = mat(&mut materials, Color::srgb(0.95, 0.93, 0.9));
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(14.0, 14.0))), MeshMaterial3d(floor)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(14.0, 5.0, 0.1))), MeshMaterial3d(wall), Transform::from_xyz(0.0, 2.5, -2.5)));
    commands.spawn((DirectionalLight { illuminance: 6000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((PointLight { intensity: 300_000.0, range: 20.0, ..default() }, Transform::from_xyz(-2.0, 3.0, 3.0)));

    // the real avatar body, feet on the floor, in a skin-tone lit material
    let skin = materials.add(StandardMaterial { base_color: Color::srgb(0.90, 0.78, 0.66), perceptual_roughness: 0.6, ..default() });
    commands.spawn((
        Mesh3d(meshes.add(loaded.mesh.take().unwrap())),
        MeshMaterial3d(skin),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Model,
    ));

    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.9, 3.6).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y)));
}

/// Four shots: front, three-quarter, side, back — rotating the model between them — then exit.
fn run(
    mut commands: Commands,
    time: Res<Time>,
    out: Res<Out>,
    mut model: Query<&mut Transform, With<Model>>,
    mut exit: MessageWriter<AppExit>,
    mut step: Local<(usize, f32)>,
) {
    // stand the mesh up (authored lying along -z after our z-flip: +90deg about x puts the head up, feet at origin),
    // then turntable for front / three-quarter / side / back.
    let stand = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    const TURNS: [(&str, f32); 4] = [("front", 0.0), ("three_quarter", 0.75), ("side", 1.5708), ("back", 3.1416)];
    let (i, since) = &mut *step;
    *since += time.delta_secs();
    let Some((name, angle)) = TURNS.get(*i).copied() else {
        if *since > 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    };
    for mut tr in &mut model {
        tr.rotation = Quat::from_rotation_y(angle) * stand;
    }
    let wait = if *i == 0 { 2.5 } else { 0.8 };
    if *since > wait {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.0.join(format!("{name}.png"))));
        *i += 1;
        *since = 0.0;
    }
}
