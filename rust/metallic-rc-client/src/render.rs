//! A room's look in Bevy: the game's materials as `StandardMaterial`s, its lights, and a sky.
//!
//! Units: the game (URP) puts `colour * intensity / d^2` straight into the frame; Bevy works in lumens and lux and
//! multiplies lit colour by the camera's exposure (default EV100 9.7, so 1 / 998) and Lambert's 1/pi. The factors
//! below undo that, so a Unity value lands on screen at the same brightness. INFERRED: tone mapping differs.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    light::{GeneratedEnvironmentMapLight, Skybox},
    math::Affine2,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension},
};

use crate::{room::{Blend, RoomLight, RoomMaterial}, shapes::Pixels};

/// 1 / (exposure at EV100 9.7) = 1.2 * 2^9.7
const UNEXPOSE: f32 = 998.0;
/// Unity point/spot intensity to Bevy lumens: 4 pi (lumens to candela) * pi (Lambert) * UNEXPOSE
const POINT_LUMENS: f32 = 4.0 * std::f32::consts::PI * std::f32::consts::PI * UNEXPOSE;
/// Unity directional intensity to Bevy lux
const DIRECTIONAL_LUX: f32 = std::f32::consts::PI * UNEXPOSE;

pub fn image(px: &Pixels, srgb: bool) -> Image {
    let (w, h, data) = px;
    let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    let mut img = Image::new(Extent3d { width: *w, height: *h, depth_or_array_layers: 1 }, TextureDimension::D2, data.clone(), format, RenderAssetUsages::RENDER_WORLD);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
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

/// Lights for a room that brings none (the game lights those from the level files): one bright light under the
/// ceiling and a weak sun, the same as the client's stand-in. INFERRED.
pub fn spawn_default_lights(commands: &mut Commands, lo: Vec3, hi: Vec3) -> Vec<Entity> {
    let top = Vec3::new((lo.x + hi.x) / 2.0, hi.y.min(lo.y + 6.0) - 0.3, (lo.z + hi.z) / 2.0);
    vec![
        commands.spawn((PointLight { intensity: 400_000.0, range: 40.0, shadow_maps_enabled: true, ..default() }, Transform::from_translation(top))).id(),
        commands.spawn((DirectionalLight { illuminance: 3000.0, ..default() }, Transform::from_xyz(2.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y))).id(),
    ]
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

/// A sky cubemap (64 px faces, linear half floats): a vertical gradient from `ground` through `horizon` to
/// `zenith`. INFERRED: the game's skies come from the level files; these colours are ours.
pub fn sky_cubemap(zenith: [f32; 3], horizon: [f32; 3], ground: [f32; 3]) -> Image {
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
                }.normalize();
                let c: [f32; 3] = if d.y >= 0.0 {
                    let t = d.y.powf(0.6);
                    std::array::from_fn(|i| horizon[i] + (zenith[i] - horizon[i]) * t)
                } else {
                    let t = (-d.y).powf(0.4);
                    std::array::from_fn(|i| horizon[i] + (ground[i] - horizon[i]) * t)
                };
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

/// Puts the sky on a camera, with the same cubemap filtered for reflections and ambient light.
pub fn sky_on_camera(commands: &mut Commands, camera: Entity, sky: Handle<Image>) {
    commands.entity(camera).insert((
        Skybox { image: Some(sky.clone()), brightness: UNEXPOSE, ..default() },
        // INFERRED strength: the game's ambient comes from probes baked into the level files
        GeneratedEnvironmentMapLight { environment_map: sky, intensity: 300.0, ..default() },
    ));
}
