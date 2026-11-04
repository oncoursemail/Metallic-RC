//! A room's look in Bevy: the game's materials as `StandardMaterial`s, its lights, and a sky.
//!
//! Units: the game (URP) puts `colour * intensity / d^2` straight into the frame; Bevy works in lumens and lux and
//! multiplies lit colour by the camera's exposure (default EV100 9.7, so 1 / 998) and Lambert's 1/pi. The factors
//! below undo that, so a Unity value lands on screen at the same brightness. INFERRED: tone mapping differs.
use bevy::{
    anti_alias::smaa::Smaa,
    asset::RenderAssetUsages,
    camera::{Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, GeneratedEnvironmentMapLight, ShadowFilteringMethod, Skybox},
    math::Affine2,
    pbr::{ExtendedMaterial, MaterialExtension, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter},
    prelude::*,
    render::{render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension}, view::Msaa},
    shader::ShaderRef,
};
use std::{collections::HashMap, hash::{Hash, Hasher}};

use crate::{room::{Blend, RoomLight, RoomMaterial}, shapes::Pixels};

/// 1 / (exposure at EV100 9.7) = 1.2 * 2^9.7
const UNEXPOSE: f32 = 998.0;
/// Bevy's default exposure, which the unit factors here assume
const EV100: f32 = 9.7;
/// Unity point/spot intensity to Bevy lumens: 4 pi (lumens to candela) * pi (Lambert) * UNEXPOSE
const POINT_LUMENS: f32 = 4.0 * std::f32::consts::PI * std::f32::consts::PI * UNEXPOSE;
/// Unity directional intensity to Bevy lux
const DIRECTIONAL_LUX: f32 = std::f32::consts::PI * UNEXPOSE;

/// A texture with its mip chain (the decoded game textures keep only their top level) and trilinear, anisotropic
/// sampling, as Unity samples them; without mips, floors and walls shimmer and moire in the distance.
pub fn image(px: &Pixels, srgb: bool) -> Image {
    let (w, h, data) = px;
    let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    let (chain, levels) = mip_chain(*w, *h, data, srgb);
    let mut img = Image::new(Extent3d { width: *w, height: *h, depth_or_array_layers: 1 }, TextureDimension::D2, chain, format, RenderAssetUsages::RENDER_WORLD);
    img.texture_descriptor.mip_level_count = levels;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

/// Level 0 followed by each half-size level down to 1x1, box-filtered (in linear light for sRGB data).
fn mip_chain(w: u32, h: u32, top: &[u8], srgb: bool) -> (Vec<u8>, u32) {
    let to_lin: Vec<f32> = (0..256).map(|i| if srgb { crate::shapes::linear([i as f32 / 255.0; 3])[0] } else { i as f32 / 255.0 }).collect();
    // linear 0..1 in 4096 steps back to the byte
    let to_byte: Vec<u8> = (0..4096).map(|i| {
        let v = i as f32 / 4095.0;
        let s = if !srgb { v } else if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }).collect();
    let mut out = top.to_vec();
    let (mut lw, mut lh, mut levels) = (w as usize, h as usize, 1);
    let mut prev = top.to_vec();
    while lw > 1 || lh > 1 {
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let (x0, y0) = ((2 * x).min(lw - 1), (2 * y).min(lh - 1));
                let (x1, y1) = ((2 * x + 1).min(lw - 1), (2 * y + 1).min(lh - 1));
                for c in 0..4 {
                    let at = |xx: usize, yy: usize| prev[(yy * lw + xx) * 4 + c];
                    let o = &mut next[(y * nw + x) * 4 + c];
                    if c == 3 {
                        *o = ((at(x0, y0) as u32 + at(x1, y0) as u32 + at(x0, y1) as u32 + at(x1, y1) as u32 + 2) / 4) as u8;
                    } else {
                        let v = (to_lin[at(x0, y0) as usize] + to_lin[at(x1, y0) as usize] + to_lin[at(x0, y1) as usize] + to_lin[at(x1, y1) as usize]) / 4.0;
                        *o = to_byte[(v * 4095.0 + 0.5) as usize];
                    }
                }
            }
        }
        out.extend_from_slice(&next);
        (prev, lw, lh, levels) = (next, nw, nh, levels + 1);
    }
    (out, levels)
}

/// A room material: Bevy's PBR with the game's AG surface maths (`ag_surface.wesl`).
pub type AgMaterial = ExtendedMaterial<StandardMaterial, AgSurface>;

/// The inputs of the game's surface shaders that Bevy's StandardMaterial has no slot for.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct AgSurface {
    #[uniform(100)]
    pub params: AgParams,
    #[texture(101)]
    #[sampler(102)]
    pub base_map: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    pub spec_map: Option<Handle<Image>>,
    #[texture(105)]
    #[sampler(106)]
    pub emission_map: Option<Handle<Image>>,
}

#[derive(ShaderType, Reflect, Debug, Clone, Copy, Default)]
pub struct AgParams {
    /// linear tint, opacity
    pub tint: Vec4,
    /// uv * xy + zw
    pub uv_st: Vec4,
    /// linear `_SpecColor`, `_Smoothness`
    pub spec: Vec4,
    /// linear `_EmissionColor`, `_EmissivePower`
    pub emission: Vec4,
    pub flags: u32,
}

const AG_MASKED: u32 = 1;
const AG_BASE_MAP: u32 = 2;
const AG_SPEC_MAP: u32 = 4;
const AG_EMISSION_MAP: u32 = 8;
const AG_URP_LIT: u32 = 16;
const AG_ALPHA_FROM_MAP: u32 = 32;
const AG_MIRROR: u32 = 64;

impl MaterialExtension for AgSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://metallic_rc_client/ag_surface.wesl".into()
    }
}

/// GPU images made from room textures, one per distinct texture (materials share them).
#[derive(Default)]
pub struct Textures(HashMap<(u32, u32, u64, bool), Handle<Image>>);

impl Textures {
    pub fn get(&mut self, p: &Pixels, srgb: bool, images: &mut Assets<Image>) -> Handle<Image> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        p.2.hash(&mut h);
        self.0.entry((p.0, p.1, h.finish(), srgb)).or_insert_with(|| images.add(image(p, srgb))).clone()
    }
}

pub fn material(m: &RoomMaterial, images: &mut Assets<Image>, textures: &mut Textures) -> AgMaterial {
    let mut tex = |p: &Option<Pixels>, srgb: bool| p.as_ref().map(|p| textures.get(p, srgb, images));
    let base_map = tex(&m.texture, true);
    let lin = crate::shapes::linear([m.color[0], m.color[1], m.color[2]]);
    let flags = [
        (m.masked, AG_MASKED),
        (base_map.is_some(), AG_BASE_MAP),
        (m.spec_map.is_some(), AG_SPEC_MAP),
        (m.emissive_map.is_some(), AG_EMISSION_MAP),
        (!m.masked && !m.unlit, AG_URP_LIT),
        (matches!(m.blend, Blend::Mask(_)), AG_ALPHA_FROM_MAP),
        (m.shader == "AG/Mirror", AG_MIRROR),
    ].iter().filter(|f| f.0).fold(0, |a, f| a | f.1);
    let uv_transform = Affine2::from_scale_angle_translation(Vec2::from(m.uv_scale), 0.0, Vec2::from(m.uv_offset));
    let [sx, sy] = m.uv_scale;
    let [ox, oy] = m.uv_offset;
    ExtendedMaterial {
        base: StandardMaterial {
            // the extension computes colour, gloss and emission; the base keeps what Bevy's own passes need: the
            // opacity (and, for alpha-tested ones, the map) for the depth prepass, the normal map and blend mode
            base_color: Color::srgba(1.0, 1.0, 1.0, m.color[3]),
            base_color_texture: if matches!(m.blend, Blend::Mask(_)) { base_map.clone() } else { None },
            normal_map_texture: tex(&m.normal_map, false),
            unlit: m.unlit,
            alpha_mode: match m.blend {
                Blend::Opaque => AlphaMode::Opaque,
                Blend::Mask(c) => AlphaMode::Mask(c),
                Blend::Blend => AlphaMode::Blend,
                Blend::Add => AlphaMode::Add,
            },
            uv_transform,
            double_sided: true,
            cull_mode: None,
            // baked lightmaps (Levels lane) hold Unity's light values; this puts them at the frame's brightness
            lightmap_exposure: UNEXPOSE,
            ..default()
        },
        extension: AgSurface {
            params: AgParams {
                tint: Vec4::new(lin[0], lin[1], lin[2], m.color[3]),
                uv_st: Vec4::new(sx, sy, ox, oy),
                spec: Vec4::new(m.spec_color[0], m.spec_color[1], m.spec_color[2], m.smoothness),
                emission: Vec4::new(m.emissive[0], m.emissive[1], m.emissive[2], m.emissive_power),
                flags,
            },
            base_map,
            spec_map: tex(&m.spec_map, false),
            emission_map: tex(&m.emissive_map, true),
        },
    }
}

/// The light as a Bevy light bundle, spawned by the caller (as a child of the prop it belongs to, or alone).
pub fn spawn_light(commands: &mut Commands, l: &RoomLight, transform: Transform) -> Entity {
    let [r, g, b] = l.color;
    let color = Color::linear_rgb(r, g, b);
    let lumens = l.intensity * POINT_LUMENS;
    match (l.kind, l.hemisphere) {
        (1, _) => commands.spawn((DirectionalLight { color, illuminance: l.intensity * DIRECTIONAL_LUX, shadow_maps_enabled: l.shadows, ..default() }, transform)).id(),
        // a hemisphere light lights the half-space in front of it: a spot of up to 180 degrees (INFERRED: Bevy caps
        // the outer angle at 90 degrees from the axis, which is exactly that)
        (2, Some((inner, outer))) => commands.spawn((SpotLight {
            color, intensity: lumens, range: l.range.max(0.1), shadow_maps_enabled: l.shadows,
            outer_angle: (outer / 2.0).to_radians().min(std::f32::consts::FRAC_PI_2),
            inner_angle: (inner / 2.0).to_radians().min((outer / 2.0).to_radians()),
            ..default()
        }, transform)).id(),
        (0, _) => commands.spawn((SpotLight {
            color, intensity: lumens, range: l.range.max(0.1), shadow_maps_enabled: l.shadows,
            outer_angle: (l.spot_angle / 2.0).to_radians(),
            inner_angle: (l.inner_spot_angle / 2.0).to_radians().min((l.spot_angle / 2.0).to_radians()),
            ..default()
        }, transform)).id(),
        _ => commands.spawn((PointLight { color, intensity: lumens, range: l.range.max(0.1), shadow_maps_enabled: l.shadows, ..default() }, transform)).id(),
    }
}

/// A shadowed spot light far outside the room (the old scenes' `KeyLight`: the Dorm's is ~22 m out, past the
/// window) is the room's sun: a directional light along its axis. Its strength is `SUN_SHARE` of the spot's
/// intensity: in the game's own capture of the lit Dorm (its custom reflection cubemap) the rug in the sun is ~4.3x
/// brighter (linear) than in the window frame's shadow, which with the room's ambient probe needs a sun of ~2 Unity
/// units (intensity 5 x 0.4). Unity's spot falloff over 22 m would leave far less, so the share is calibrated, not
/// derived (INFERRED).
pub fn sun_of(l: &RoomLight, lo: Vec3, hi: Vec3) -> Option<(f32, [f32; 3])> {
    const SUN_SHARE: f32 = 0.4;
    let at = l.world.w_axis.truncate();
    let outside = at.cmplt(lo - Vec3::splat(2.0)).any() || at.cmpgt(hi + Vec3::splat(2.0)).any();
    (l.kind == 0 && l.shadows && outside && lo.x <= hi.x).then_some((l.intensity * SUN_SHARE, l.color))
}

/// The sun's shadow cascades: one map over the whole of a room up to 40 m across, three beyond. Every cascade is
/// drawn again for each view (the mirror's too) and costs ~1.3 ms of CPU per view at 1080p, so small rooms get
/// one (measured in the Dorm: 3 cascades 15 ms a frame, 1 cascade 9.5 ms).
fn sun_cascades(reach: f32) -> bevy::light::CascadeShadowConfig {
    let num_cascades = if reach <= 40.0 { 1 } else { 3 };
    CascadeShadowConfigBuilder { num_cascades, first_cascade_far_bound: 6.0, maximum_distance: reach, ..default() }.build()
}

/// The sun from `sun_of`: illuminance in Unity units, linear colour, the light's rotation.
pub fn spawn_sun(commands: &mut Commands, (strength, [r, g, b]): (f32, [f32; 3]), rotation: Quat, lo: Vec3, hi: Vec3) -> Entity {
    let reach = (hi - lo).length().clamp(20.0, 120.0);
    info!("room sun: {strength:.2} Unity units, shadows over {reach:.0} m (room {lo} .. {hi})");
    commands.spawn((
        DirectionalLight { color: Color::linear_rgb(r, g, b), illuminance: strength * DIRECTIONAL_LUX, shadow_maps_enabled: true, ..default() },
        sun_cascades(reach),
        Transform::from_rotation(rotation),
    )).id()
}

/// Lights for a room that brings none (the game lights those from the level files): a warm sun with soft cascaded
/// shadows. Its direction is the original Dorm scene's `KeyLight` (dormroom2 in the older build; same frame as
/// `rooms/dorm.layout`); colour and strength are INFERRED. Ambient light comes from the sky (`sky_on_camera`).
pub fn spawn_default_lights(commands: &mut Commands, lo: Vec3, hi: Vec3) -> Vec<Entity> {
    // Unity rotation (-0.645, -0.216, 0.635, -0.365) turned into the client's space as `room::unity_to_bevy` does
    let sun = Quat::from_xyzw(0.645, 0.216, 0.635, -0.365).normalize();
    let reach = (hi - lo).length().clamp(20.0, 120.0);
    vec![commands.spawn((
        DirectionalLight { color: Color::srgb(1.0, 0.93, 0.82), illuminance: 2.5 * DIRECTIONAL_LUX, shadow_maps_enabled: true, ..default() },
        sun_cascades(reach),
        Transform::from_rotation(sun),
    )).id()]
}

/// The camera look every room gets: the HDR chain of the game's URP camera. From the Stunt Runner volume profile
/// (`StuntRunner_Default_SSFX_Profile`, the one room profile in the Addressables): Neutral tone mapping, bloom
/// threshold 1.1 / intensity 0.5 / scatter 0.617, post exposure +0.21 EV (its contrast 2.1 and white balance 2.8 of
/// +-100 are too small to matter). Khronos PBR Neutral stands in for URP's Neutral curve and SMAA + SSAO for URP's
/// MSAA and SSAO renderer feature: INFERRED.
pub struct LookPlugin;

impl Plugin for LookPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "ag_surface.wesl");
        app.add_plugins((MaterialPlugin::<AgMaterial>::default(), crate::mirror::MirrorPlugin))
            .insert_resource(DirectionalLightShadowMap { size: 4096 })
            .add_systems(Update, dress_cameras);
        app.init_resource::<RoomEnvMaps>().add_systems(Update, apply_room_env.after(dress_cameras));
        if std::env::var_os("METALLIC_RC_FRAMES").is_some() {
            app.add_systems(Last, log_frames);
        }
    }
}

/// `METALLIC_RC_FRAMES=1`: logs the mean and worst frame time every 5 s (performance checks).
fn log_frames(time: Res<Time<Real>>, mut acc: Local<(f32, f32, u32)>) {
    let dt = time.delta_secs();
    *acc = (acc.0 + dt, acc.1.max(dt), acc.2 + 1);
    if acc.0 >= 5.0 {
        info!("frames: {} in {:.1} s, mean {:.2} ms ({:.0} fps), worst {:.2} ms", acc.2, acc.0, acc.0 * 1000.0 / acc.2 as f32,
              acc.2 as f32 / acc.0, acc.1 * 1000.0);
        *acc = (0.0, 0.0, 0);
    }
}

/// Mirror cameras get only the tone curve (mirror.rs): their picture is small on screen.
fn dress_cameras(mut commands: Commands, cameras: Query<Entity, (Added<Camera3d>, Without<crate::mirror::MirrorCamera>)>) {
    for cam in &cameras {
        commands.entity(cam).insert((
            Hdr,
            Tonemapping::KhronosPbrNeutral,
            Exposure { ev100: EV100 - 0.21 },
            // URP bloom adds `intensity` x the blurred part above the threshold; Bevy's additive mode does the same
            Bloom {
                intensity: 0.5 * 0.3,
                prefilter: BloomPrefilter { threshold: 1.1, threshold_softness: 0.5 },
                composite_mode: BloomCompositeMode::Additive,
                low_frequency_boost: 0.617,
                ..Bloom::NATURAL
            },
            Msaa::Off,
            Smaa::default(),
            ScreenSpaceAmbientOcclusion { quality_level: ScreenSpaceAmbientOcclusionQualityLevel::Medium, ..default() },
            ShadowFilteringMethod::Gaussian,
        ));
    }
}

/// A room's ambient and reflections from its level file (`levels::lighting`), handed over by the loader thread.
pub struct RoomEnv {
    /// RenderSettings' ambient probe, `[channel][coefficient]`
    pub sh: [[f32; 9]; 3],
    /// the custom reflection cubemap's faces (+X -X +Y -Y +Z -Z, sRGB) and its intensity
    pub reflection: Option<Vec<Pixels>>,
    pub reflection_intensity: f32,
    pub fog: Option<crate::levels::Fog>,
}

impl RoomEnv {
    pub fn new(l: &crate::levels::Lighting) -> Self {
        let square = |c: &&crate::levels::Cubemap| c.faces.len() == 6 && c.faces.iter().all(|f| f.0 == c.size && f.1 == c.size);
        RoomEnv { sh: l.ambient_sh, reflection: l.reflection.as_ref().filter(square).map(|c| c.faces.clone()), reflection_intensity: l.reflection_intensity, fog: l.fog.clone() }
    }
}

/// Set by the loader thread when the room's level file has lighting; `apply_room_env` takes it.
pub static ROOM_ENV: std::sync::Mutex<Option<RoomEnv>> = std::sync::Mutex::new(None);

/// The room's environment cube and fog once applied (cameras added later get them too).
#[derive(Resource, Default)]
pub struct RoomEnvMaps(pub Option<(Handle<Image>, Option<DistanceFog>)>);

/// The radiance whose cosine-weighted average (Bevy's diffuse ambient) is what Unity's `ShadeSH9` gives for the
/// probe (the shader-constant form: `sh0 + sh3 x + sh1 y + sh2 z + sh4 xy + sh5 yz + sh6 (3z^2 - 1) + sh7 xz +
/// sh8 (x^2 - y^2)` per unit albedo; the probe being stored in that form, DC = flat ambient colour, is INFERRED):
/// band 1 x 3/2 and band 2 x 4 undo the cosine lobe. `d` in Unity's axes.
fn sh_radiance(sh: &[[f32; 9]; 3], d: Vec3) -> [f32; 3] {
    let (x, y, z) = (d.x, d.y, d.z);
    std::array::from_fn(|c| {
        let k = &sh[c];
        k[0] + 1.5 * (k[3] * x + k[1] * y + k[2] * z) + 4.0 * (k[4] * x * y + k[5] * y * z + k[6] * (3.0 * z * z - 1.0) + k[7] * x * z + k[8] * (x * x - y * y))
    })
}

/// Real SH basis, bands 0..2, Unity's coefficient order.
fn sh_basis(d: Vec3) -> [f32; 9] {
    let (x, y, z) = (d.x, d.y, d.z);
    [0.282_095, 0.488_603 * y, 0.488_603 * z, 0.488_603 * x, 1.092_548 * x * y, 1.092_548 * y * z, 0.315_392 * (3.0 * z * z - 1.0), 1.092_548 * x * z, 0.546_274 * (x * x - y * y)]
}

/// The direction of a cube texel: face order +X -X +Y -Y +Z -Z, (u, v) in -1..1, rows top down. Bevy samples cube
/// maps with z negated, so this is also the direction in Unity's axes.
fn cube_dir(face: usize, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
}

/// Every environment cube has this size: Bevy keeps a camera's filtered maps when its source is swapped, so the
/// stand-in and the room's own must match.
const ENV_SIZE: usize = 128;

/// The texel of a cube map (faces +X -X +Y -Y +Z -Z, `n` x `n`, rows top down) that direction `d` looks at.
fn cube_texel(n: usize, d: Vec3) -> (usize, usize, usize) {
    let a = d.abs();
    let (f, sc, tc, m) = if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 { (0, -d.z, -d.y, a.x) } else { (1, d.z, -d.y, a.x) }
    } else if a.y >= a.z {
        if d.y > 0.0 { (2, d.x, d.z, a.y) } else { (3, d.x, -d.z, a.y) }
    } else if d.z > 0.0 {
        (4, d.x, -d.y, a.z)
    } else {
        (5, -d.x, -d.y, a.z)
    };
    let at = |c: f32| (((c / m + 1.0) / 2.0 * n as f32) as usize).min(n - 1);
    (f, at(sc), at(tc))
}

/// The room's environment cube: its diffuse light is the ambient probe's (bands 0..2), its reflections the custom
/// reflection cubemap's detail at `reflection_intensity` (with the cubemap's own bands 0..2 taken out, so the
/// diffuse light is not counted twice). Without a reflection cubemap, the probe alone.
pub fn room_env_cubemap(env: &RoomEnv) -> Image {
    let n = ENV_SIZE;
    let lin = |b: u8| {
        let c = b as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let texel = |n: usize, f: usize, x: usize, y: usize| {
        let (u, v) = ((x as f32 + 0.5) / n as f32 * 2.0 - 1.0, (y as f32 + 0.5) / n as f32 * 2.0 - 1.0);
        (cube_dir(f, u, v).normalize(), (1.0 + u * u + v * v).powf(-1.5))
    };
    // the reflection's own bands 0..2: projection weighted by each texel's solid angle
    let mut low = [[0.0f32; 9]; 3];
    if let Some(faces) = &env.reflection {
        let mut total = 0.0;
        for (f, (size, _, px)) in faces.iter().enumerate() {
            let m = *size as usize;
            for y in 0..m {
                for x in 0..m {
                    let (d, w) = texel(m, f, x, y);
                    let b = sh_basis(d);
                    let p = &px[(y * m + x) * 4..];
                    for c in 0..3 {
                        let l = lin(p[c]) * w;
                        for k in 0..9 {
                            low[c][k] += l * b[k];
                        }
                    }
                    total += w;
                }
            }
        }
        let scale = 4.0 * std::f32::consts::PI / total;
        low.iter_mut().flatten().for_each(|x| *x *= scale);
    }
    let mut data = Vec::with_capacity(n * n * 6 * 8);
    for f in 0..6 {
        for y in 0..n {
            for x in 0..n {
                let d = texel(n, f, x, y).0;
                let mut c = sh_radiance(&env.sh, d);
                if let Some(faces) = &env.reflection {
                    let b = sh_basis(d);
                    let m = faces[0].0 as usize;
                    let (sf, sx, sy) = cube_texel(m, d);
                    let p = &faces[sf].2[(sy * m + sx) * 4..];
                    for ch in 0..3 {
                        let lowpass: f32 = (0..9).map(|k| low[ch][k] * b[k]).sum();
                        c[ch] += env.reflection_intensity * (lin(p[ch]) - lowpass);
                    }
                }
                for ch in [c[0], c[1], c[2], 1.0] {
                    data.extend_from_slice(&f16_bits(ch.max(0.0)).to_le_bytes());
                }
            }
        }
    }
    let n = n as u32;
    let mut img = Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 6 }, TextureDimension::D2, data, TextureFormat::Rgba16Float, RenderAssetUsages::RENDER_WORLD);
    img.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    img
}

/// Unity fog as Bevy distance fog (mode 1 linear, 2 exponential, 3 exponential squared).
fn room_fog(f: &crate::levels::Fog) -> DistanceFog {
    let falloff = match f.mode {
        1 => FogFalloff::Linear { start: f.start, end: f.end },
        3 => FogFalloff::ExponentialSquared { density: f.density },
        _ => FogFalloff::Exponential { density: f.density },
    };
    DistanceFog { color: Color::linear_rgb(f.color[0], f.color[1], f.color[2]), falloff, ..default() }
}

/// Extra exposure for rooms lit from the old level files: with their ambient probe and lights the walls come out
/// ~1.75x darker (luminance) than in the game's own capture of the lit Dorm (its reflection cubemap), and the
/// owner's reference screenshot is brighter still. Calibrated, INFERRED (the 2019 scenes used Unity's built-in
/// pipeline, whose post chain is not in these files).
const LEVEL_EV: f32 = 1.0;

/// Puts the room's environment (`ROOM_ENV`) on the main cameras when it arrives, and on cameras added later.
fn apply_room_env(mut commands: Commands, mut maps: ResMut<RoomEnvMaps>, mut images: ResMut<Assets<Image>>,
                  cameras: Query<Entity, (With<Camera3d>, Without<crate::mirror::MirrorCamera>)>,
                  added: Query<Entity, (Added<Camera3d>, Without<crate::mirror::MirrorCamera>)>) {
    let fresh = ROOM_ENV.lock().unwrap().take();
    if let Some(env) = &fresh {
        let t = std::time::Instant::now();
        maps.0 = Some((images.add(room_env_cubemap(env)), env.fog.as_ref().map(room_fog)));
        info!("room environment: ambient probe, reflection cubemap {} (x{:.2}), fog {}, made in {:.2} s", env.reflection.is_some(),
              env.reflection_intensity, env.fog.is_some(), t.elapsed().as_secs_f32());
    }
    let Some((map, fog)) = &maps.0 else { return };
    let targets: Vec<Entity> = if fresh.is_some() { cameras.iter().collect() } else { added.iter().collect() };
    for cam in targets {
        let mut e = commands.entity(cam);
        e.insert((
            GeneratedEnvironmentMapLight { environment_map: map.clone(), intensity: UNEXPOSE, affects_lightmapped_mesh_diffuse: false, ..default() },
            Exposure { ev100: EV100 - 0.21 - LEVEL_EV },
        ));
        if let Some(fog) = fog {
            e.insert(fog.clone());
        }
    }
}

fn f16_bits(v: f32) -> u16 {
    // finite, non-negative values well inside the half range
    if v <= 0.0 {
        return 0;
    }
    let b = v.to_bits();
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    if exp <= 0 {
        return 0;
    }
    ((exp as u16) << 10) | ((b >> 13) & 0x3ff) as u16
}

/// The game's sky shader `AG/Skybox/Simple Gradient` (VERIFIED from its D3D11 program): six colours along the view
/// direction's height y, sky top/mid/horizon at y = 1, 2/3, 1/3 and the ground horizon at y = 0 (so the sky's lowest
/// third fades from ground horizon to sky horizon), ground mid/top at y = -1/2 and -1, times `_Exposure`. Colours are
/// sRGB as authored (the slot order is the property order: INFERRED).
#[derive(Clone, Copy, Debug)]
pub struct SimpleGradient {
    pub sky_top: [f32; 3],
    pub sky_mid: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub ground_horizon: [f32; 3],
    pub ground_mid: [f32; 3],
    pub ground_top: [f32; 3],
    pub exposure: f32,
}

impl Default for SimpleGradient {
    /// A clear day. INFERRED: no room sky material ships in the Addressables (the level files have them).
    fn default() -> Self {
        SimpleGradient {
            sky_top: [0.24, 0.50, 0.92],
            sky_mid: [0.42, 0.67, 0.96],
            sky_horizon: [0.78, 0.89, 0.98],
            ground_horizon: [0.66, 0.74, 0.58],
            ground_mid: [0.36, 0.45, 0.26],
            ground_top: [0.27, 0.31, 0.20],
            exposure: 1.0,
        }
    }
}

impl SimpleGradient {
    /// Ambient light inside a room, standing in for the light probes the game bakes into its level files: warm white
    /// from above and the sides, the floor's warm bounce from below. INFERRED (`render.rs` notes in the look lane).
    pub fn interior() -> Self {
        SimpleGradient {
            sky_top: [1.0, 0.98, 0.95],
            sky_mid: [1.0, 0.98, 0.95],
            sky_horizon: [0.97, 0.95, 0.92],
            ground_horizon: [0.92, 0.88, 0.83],
            ground_mid: [0.80, 0.66, 0.52],
            ground_top: [0.72, 0.56, 0.42],
            exposure: 1.5,
        }
    }

    /// Linear colour towards a direction.
    pub fn at(&self, dir: Vec3) -> [f32; 3] {
        let y = dir.normalize_or_zero().y;
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] { std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0)) };
        let c = if y >= 0.0 {
            if y <= 1.0 / 3.0 {
                lerp(self.ground_horizon, self.sky_horizon, 3.0 * y)
            } else if y <= 2.0 / 3.0 {
                lerp(self.sky_horizon, self.sky_mid, 3.0 * y - 1.0)
            } else {
                lerp(self.sky_mid, self.sky_top, 3.0 * y - 2.0)
            }
        } else if -y <= 0.5 {
            lerp(self.ground_horizon, self.ground_mid, -2.0 * y)
        } else {
            lerp(self.ground_mid, self.ground_top, -2.0 * y - 1.0)
        };
        crate::shapes::linear(c).map(|v| v * self.exposure)
    }
}

/// A sky cubemap (64 px faces, linear half floats) of a gradient sky.
pub fn sky_cubemap(sky: &SimpleGradient) -> Image {
    const N: u32 = ENV_SIZE as u32;
    let mut data = Vec::with_capacity((N * N * 6 * 8) as usize);
    for face in 0..6 {
        for y in 0..N {
            for x in 0..N {
                let (u, v) = ((x as f32 + 0.5) / N as f32 * 2.0 - 1.0, (y as f32 + 0.5) / N as f32 * 2.0 - 1.0);
                // cube face order +X -X +Y -Y +Z -Z, image rows top down
                let d = match face {
                    0 => Vec3::new(1.0, -v, -u),
                    1 => Vec3::new(-1.0, -v, u),
                    2 => Vec3::new(u, 1.0, v),
                    3 => Vec3::new(u, -1.0, -v),
                    4 => Vec3::new(u, -v, 1.0),
                    _ => Vec3::new(-u, -v, -1.0),
                };
                let c = sky.at(d);
                for ch in [c[0], c[1], c[2], 1.0] {
                    data.extend_from_slice(&f16_bits(ch).to_le_bytes());
                }
            }
        }
    }
    let mut img = Image::new(Extent3d { width: N, height: N, depth_or_array_layers: 6 }, TextureDimension::D2, data, TextureFormat::Rgba16Float, RenderAssetUsages::RENDER_WORLD);
    img.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    img
}

/// Puts the sky on a camera, and an ambient cubemap (filtered for reflections and ambient light; the game's are the
/// probes baked into its level files). URP lights a surface's ambient with spherical harmonics at
/// `ambientIntensity` 1 (INFERRED), so the map is scaled by the exposure factor only. Baked lightmaps
/// already hold the ambient light, so lightmapped meshes take only reflections from it.
pub fn sky_on_camera(commands: &mut Commands, camera: Entity, sky: Handle<Image>, ambient: Handle<Image>) {
    commands.entity(camera).insert((
        Skybox { image: Some(sky), brightness: UNEXPOSE, ..default() },
        GeneratedEnvironmentMapLight { environment_map: ambient, intensity: UNEXPOSE, affects_lightmapped_mesh_diffuse: false, ..default() },
    ));
}
