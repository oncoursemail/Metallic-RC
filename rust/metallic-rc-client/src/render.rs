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
    pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter},
    prelude::*,
    render::{render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension}, view::Msaa},
};

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

pub fn material(m: &RoomMaterial, images: &mut Assets<Image>) -> StandardMaterial {
    let mut tex = |p: &Option<Pixels>, srgb: bool| p.as_ref().map(|p| images.add(image(p, srgb)));
    let [cr, cg, cb, ca] = m.color;
    let [er, eg, eb] = m.emissive;
    StandardMaterial {
        base_color: Color::srgba(cr, cg, cb, ca),
        base_color_texture: tex(&m.texture, true),
        perceptual_roughness: m.roughness.clamp(0.05, 1.0),
        metallic: 0.0,
        metallic_roughness_texture: tex(&m.roughness_map, false),
        reflectance: m.reflectance,
        normal_map_texture: tex(&m.normal_map, false),
        emissive: LinearRgba::rgb(er, eg, eb),
        emissive_texture: tex(&m.emissive_map, true),
        unlit: m.unlit,
        alpha_mode: match m.blend {
            Blend::Opaque => AlphaMode::Opaque,
            Blend::Mask(c) => AlphaMode::Mask(c),
            Blend::Blend => AlphaMode::Blend,
            Blend::Add => AlphaMode::Add,
        },
        uv_transform: Affine2::from_scale_angle_translation(Vec2::from(m.uv_scale), 0.0, Vec2::from(m.uv_offset)),
        double_sided: true,
        cull_mode: None,
        ..default()
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

/// Lights for a room that brings none (the game lights those from the level files): a warm sun with soft cascaded
/// shadows. Its direction is the original Dorm scene's `KeyLight` (dormroom2 in the older build; same frame as
/// `rooms/dorm.layout`); colour and strength are INFERRED. Ambient light comes from the sky (`sky_on_camera`).
pub fn spawn_default_lights(commands: &mut Commands, lo: Vec3, hi: Vec3) -> Vec<Entity> {
    // Unity rotation (-0.645, -0.216, 0.635, -0.365) turned into the client's space as `room::unity_to_bevy` does
    let sun = Quat::from_xyzw(0.645, 0.216, 0.635, -0.365).normalize();
    let reach = (hi - lo).length().clamp(20.0, 120.0);
    vec![commands.spawn((
        DirectionalLight { color: Color::srgb(1.0, 0.93, 0.82), illuminance: 2.5 * DIRECTIONAL_LUX, shadow_maps_enabled: true, ..default() },
        CascadeShadowConfigBuilder { num_cascades: 3, first_cascade_far_bound: 6.0, maximum_distance: reach, ..default() }.build(),
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
        app.insert_resource(DirectionalLightShadowMap { size: 4096 })
            .add_systems(Update, dress_cameras);
    }
}

fn dress_cameras(mut commands: Commands, cameras: Query<Entity, Added<Camera3d>>) {
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
    const N: u32 = 64;
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
