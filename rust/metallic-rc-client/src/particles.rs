//! Room ambient particle VFX: a lightweight CPU particle system that renders the scene's inline ParticleSystems
//! (dust, steam, fire, sparks), decoded in `levels::scene_particles` into [`crate::room::ParticleEmitter`]. Each
//! emitter keeps a particle pool, emits at its rate (+ start bursts), integrates velocity + gravity, and lerps size
//! and colour over each particle's life; every live particle is drawn as a camera-facing billboard in a per-emitter
//! dynamic mesh with an unlit, additive, soft-dot material. A per-room budget caps the total particle count.
use bevy::math::Vec3;
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::room::ParticleEmitter;

/// Total live particles allowed across a room's emitters (shared budget; emitters past it stop spawning).
const ROOM_BUDGET: usize = 6000;

pub struct ParticlePlugin;

impl Plugin for ParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleAssets>().add_systems(Update, update_particles);
    }
}

/// The shared soft-dot texture + unlit additive material for every emitter.
#[derive(Resource)]
pub struct ParticleAssets {
    material: Handle<StandardMaterial>,
}

impl FromWorld for ParticleAssets {
    fn from_world(world: &mut World) -> Self {
        let dot = {
            let mut images = world.resource_mut::<Assets<Image>>();
            images.add(soft_dot(64))
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(dot),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        });
        ParticleAssets { material }
    }
}

/// A radial soft dot (white, alpha falls off to the edge), RGBA8.
fn soft_dot(n: u32) -> Image {
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5) / n as f32 * 2.0 - 1.0, (y as f32 + 0.5) / n as f32 * 2.0 - 1.0);
            let d = (u * u + v * v).sqrt();
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.6);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        bevy::render::render_resource::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size0: f32,
}

/// A live emitter: its decoded spec plus the pool and emission state. The mesh is rebuilt each frame.
#[derive(Component)]
pub struct Emitter {
    spec: ParticleEmitter,
    pool: Vec<Particle>,
    accum: f32,
    age: f32,
    bursts_done: bool,
    mesh: Handle<Mesh>,
    rng: u32,
}

impl Emitter {
    fn rand(&mut self) -> f32 {
        // xorshift32 -> 0..1
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }
    fn rand_dir(&mut self) -> Vec3 {
        let z = self.rand() * 2.0 - 1.0;
        let t = self.rand() * std::f32::consts::TAU;
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * t.cos(), r * t.sin(), z)
    }
}

/// Spawn an emitter entity for every emitter the room carries. Tag with `tag` (the room-piece marker) so it is
/// despawned with the room. Returns the spawned entities.
pub fn spawn_emitters(commands: &mut Commands, meshes: &mut Assets<Mesh>, assets: &ParticleAssets, emitters: &[ParticleEmitter], tag: impl Bundle + Clone) -> Vec<Entity> {
    let mut out = Vec::new();
    for (i, spec) in emitters.iter().enumerate() {
        let mesh = meshes.add(empty_mesh());
        let e = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                Transform::IDENTITY,
                Emitter { spec: spec.clone(), pool: Vec::new(), accum: 0.0, age: 0.0, bursts_done: false, mesh, rng: 0x9e3779b9 ^ (i as u32).wrapping_mul(2654435761) | 1 },
                tag.clone(),
            ))
            .id();
        out.push(e);
    }
    out
}

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    m.insert_indices(Indices::U32(Vec::new()));
    m
}

/// Lerp `(t, v)` stops at `x` (0..1); `default` when there are none.
fn sample(stops: &[(f32, f32)], x: f32, default: f32) -> f32 {
    sample_by(stops, x, |a, b, f| a + (b - a) * f).unwrap_or(default)
}
fn sample_color(stops: &[(f32, [f32; 4])], x: f32) -> Option<[f32; 4]> {
    sample_by(stops, x, |a, b, f| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f))
}
fn sample_by<T: Copy>(stops: &[(f32, T)], x: f32, lerp: impl Fn(T, T, f32) -> T) -> Option<T> {
    if stops.is_empty() {
        return None;
    }
    if x <= stops[0].0 {
        return Some(stops[0].1);
    }
    for w in stops.windows(2) {
        if x <= w[1].0 {
            let f = ((x - w[0].0) / (w[1].0 - w[0].0).max(1e-6)).clamp(0.0, 1.0);
            return Some(lerp(w[0].1, w[1].1, f));
        }
    }
    Some(stops.last().unwrap().1)
}

fn update_particles(
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut emitters: Query<&mut Emitter>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs().min(0.1);
    let Some(cam) = camera.iter().next() else { return };
    let right = cam.right().as_vec3();
    let up = cam.up().as_vec3();

    let live: usize = emitters.iter().map(|e| e.pool.len()).sum();
    let mut headroom = ROOM_BUDGET.saturating_sub(live);

    for mut e in &mut emitters {
        e.age += dt;
        // emitter placement
        let origin = e.spec.world.transform_point3(Vec3::ZERO);
        let axis = e.spec.world.transform_vector3(Vec3::Y).normalize_or(Vec3::Y); // cone/emit axis

        // how many to emit this frame (steady rate + one-shot start bursts)
        let mut to_emit = 0usize;
        if e.spec.looping || e.age <= e.spec.lifetime.max(0.1) {
            e.accum += e.spec.rate * dt;
            let n = e.accum.floor();
            e.accum -= n;
            to_emit += n as usize;
        }
        if !e.bursts_done {
            e.bursts_done = true;
            to_emit += e.spec.bursts.iter().map(|&(_, c)| c as usize).sum::<usize>();
        }
        let cap = (e.spec.max_particles as usize).saturating_sub(e.pool.len());
        to_emit = to_emit.min(cap).min(headroom);
        headroom -= to_emit;

        let (angle, radius, shape, speed, life, size) = (e.spec.shape_angle, e.spec.shape_radius, e.spec.shape, e.spec.speed, e.spec.lifetime, e.spec.size);
        for _ in 0..to_emit {
            // initial direction by shape: cone(4) spreads from the axis by up to `angle`; sphere(0) is any dir;
            // others emit along the axis.
            let dir = match shape {
                0 => e.rand_dir(),
                4 => {
                    let a = angle.to_radians() * e.rand().sqrt();
                    let az = e.rand() * std::f32::consts::TAU;
                    let (sa, ca) = a.sin_cos();
                    // build a basis around `axis`
                    let t = if axis.x.abs() < 0.9 { Vec3::X } else { Vec3::Z };
                    let u = axis.cross(t).normalize_or(Vec3::X);
                    let w = axis.cross(u);
                    (axis * ca + (u * az.cos() + w * az.sin()) * sa).normalize_or(axis)
                }
                _ => axis,
            };
            let jitter = (e.rand_dir()) * radius.min(2.0) * e.rand();
            e.pool.push(Particle { pos: origin + jitter, vel: dir * speed, age: 0.0, life: life.max(0.05), size0: size });
        }

        // integrate + cull
        let g = e.spec.gravity * -9.81;
        e.pool.retain_mut(|p| {
            p.age += dt;
            p.vel.y += g * dt;
            p.pos += p.vel * dt;
            p.age < p.life
        });

        // rebuild the billboard mesh in world space
        let start = e.spec.start_color;
        let (szc, colc) = (e.spec.size_over_life.clone(), e.spec.color_over_life.clone());
        if let Some(mut mesh) = meshes.get_mut(&e.mesh) {
            let n = e.pool.len();
            let mut pos: Vec<[f32; 3]> = Vec::with_capacity(n * 4);
            let mut uv: Vec<[f32; 2]> = Vec::with_capacity(n * 4);
            let mut col: Vec<[f32; 4]> = Vec::with_capacity(n * 4);
            let mut idx: Vec<u32> = Vec::with_capacity(n * 6);
            for (i, p) in e.pool.iter().enumerate() {
                let t = (p.age / p.life).clamp(0.0, 1.0);
                let s = p.size0 * sample(&szc, t, 1.0) * 0.5;
                let c = sample_color(&colc, t).map_or(start, |g| std::array::from_fn(|k| start[k] * g[k]));
                let (r, u) = (right * s, up * s);
                let b = i as u32 * 4;
                pos.push((p.pos - r - u).into());
                pos.push((p.pos + r - u).into());
                pos.push((p.pos + r + u).into());
                pos.push((p.pos - r + u).into());
                uv.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
                col.extend_from_slice(&[c, c, c, c]);
                idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            mesh.insert_indices(Indices::U32(idx));
        }
    }
}
