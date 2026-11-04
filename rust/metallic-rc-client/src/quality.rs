//! Graphics quality tiers. `Quality` is the one place that says how much the renderer may spend; the settings key
//! `graphics_quality` (0 Low, 1 Medium, 2 High, 3 Ultra) picks the tier, capped by what the GPU is (`tier_cap`) and
//! lowered one step at a time when the measured frame time stays over budget (`AutoQuality`).
//!
//! `QualityPlugin` applies the parts that are plain components and resources: anti-aliasing (MSAA / FXAA / SMAA) and
//! SSAO on every 3D camera, shadow map sizes, directional-light cascades, how many lights cast shadows and how many
//! are on at all (the ones nearest the camera win), and distance culling of small meshes. Other code reads the
//! rest: textures are capped at `texture_cap` by `texprep`; `lightmaps` and `lod_bias` are for the level loader;
//! `render_scale` waits for an upscaling pass (Bevy 0.20 has none apart from DLSS). It also freezes generated
//! environment maps once filtered (`LiveEnvironmentMap` opts out).
//!
//! Reading it from another system: `fn my_system(q: Res<Quality>) { if q.is_changed() { .. q.ssao .. } }`.
use bevy::{
    anti_alias::{fxaa::Fxaa, smaa::{Smaa, SmaaPreset}},
    camera::visibility::VisibilityRange,
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, PointLightShadowMap},
    pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    prelude::*,
    render::{renderer::RenderAdapterInfo, view::Msaa},
};

use crate::perf::Frames;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Low,
    Medium,
    High,
    Ultra,
}

impl Tier {
    pub fn from_index(i: i64) -> Tier {
        match i {
            ..=0 => Tier::Low,
            1 => Tier::Medium,
            2 => Tier::High,
            _ => Tier::Ultra,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn lower(self) -> Tier {
        Tier::from_index(self as i64 - 1)
    }
}

/// How edges are smoothed. MSAA is cheap on tile-based mobile GPUs and in forward rendering; SSAO needs MSAA off, so
/// the tiers with SSAO use SMAA instead.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AntiAlias {
    Off,
    Fxaa,
    Msaa4,
    Smaa(SmaaPreset),
}

#[derive(Resource, Clone, Copy, PartialEq)]
pub struct Quality {
    pub tier: Tier,
    /// fraction of the window's pixels the 3D view is drawn at (1 = native). INFERRED values; not applied yet
    pub render_scale: f32,
    /// directional light shadow map size (texels per side, per cascade)
    pub sun_shadow_size: usize,
    pub sun_cascades: usize,
    /// how far directional shadows reach (metres)
    pub sun_shadow_distance: f32,
    /// point and spot light shadow map size (texels per cube face)
    pub point_shadow_size: usize,
    /// lights allowed to cast shadows at once (the most important ones near the camera)
    pub max_shadow_lights: usize,
    /// point/spot lights on at once (the rest are switched off until the camera comes closer)
    pub max_lights: usize,
    pub anti_alias: AntiAlias,
    pub ssao: bool,
    /// a mesh of radius r metres is culled beyond `r * small_cull` metres (0 = never), and nothing beyond `view_distance`
    pub small_cull: f32,
    pub view_distance: f32,
    /// > 1 switches to simpler meshes sooner (for the level loader's LODs)
    pub lod_bias: f32,
    /// largest texture side uploaded (bigger textures drop their top mip levels)
    pub texture_cap: u32,
    /// anisotropic filtering of room textures (1 = off)
    pub anisotropy: u16,
    /// prefer baked lightmaps over extra realtime lights where a room has them
    pub lightmaps: bool,
}

impl Quality {
    pub fn tier(tier: Tier) -> Quality {
        match tier {
            Tier::Low => Quality {
                tier, render_scale: 0.75, sun_shadow_size: 1024, sun_cascades: 1, sun_shadow_distance: 20.0, point_shadow_size: 256,
                max_shadow_lights: 1, max_lights: 8, anti_alias: AntiAlias::Fxaa, ssao: false, small_cull: 60.0, view_distance: 60.0,
                lod_bias: 2.0, texture_cap: 512, anisotropy: 2, lightmaps: true,
            },
            Tier::Medium => Quality {
                tier, render_scale: 1.0, sun_shadow_size: 2048, sun_cascades: 2, sun_shadow_distance: 40.0, point_shadow_size: 512,
                max_shadow_lights: 2, max_lights: 16, anti_alias: AntiAlias::Msaa4, ssao: false, small_cull: 120.0, view_distance: 120.0,
                lod_bias: 1.0, texture_cap: 1024, anisotropy: 4, lightmaps: true,
            },
            Tier::High => Quality {
                tier, render_scale: 1.0, sun_shadow_size: 2048, sun_cascades: 3, sun_shadow_distance: 60.0, point_shadow_size: 1024,
                max_shadow_lights: 4, max_lights: 32, anti_alias: AntiAlias::Smaa(SmaaPreset::High), ssao: true, small_cull: 250.0,
                view_distance: 250.0, lod_bias: 1.0, texture_cap: 2048, anisotropy: 8, lightmaps: false,
            },
            Tier::Ultra => Quality {
                tier, render_scale: 1.0, sun_shadow_size: 4096, sun_cascades: 4, sun_shadow_distance: 100.0, point_shadow_size: 1024,
                max_shadow_lights: 8, max_lights: 64, anti_alias: AntiAlias::Smaa(SmaaPreset::Ultra), ssao: true, small_cull: 0.0,
                view_distance: 1000.0, lod_bias: 0.75, texture_cap: 4096, anisotropy: 16, lightmaps: false,
            },
        }
    }

    /// The tier for the settings value `graphics_quality` (0..3).
    pub fn from_setting(v: f64) -> Quality {
        Quality::tier(Tier::from_index(v.round() as i64))
    }
}

impl Default for Quality {
    fn default() -> Self {
        Quality::tier(Tier::High)
    }
}

/// The highest tier worth trying on this GPU: software and GL renderers Low, phones and headsets Low (INFERRED: a
/// Quest 3 manages Medium's features at reduced resolution, to be measured), integrated GPUs Medium, else Ultra.
pub fn tier_cap(adapter: &RenderAdapterInfo) -> Tier {
    // wgpu's Backend and DeviceType are not re-exported by bevy_render: compared by name
    let (backend, device) = (format!("{:?}", adapter.backend), format!("{:?}", adapter.device_type));
    if cfg!(target_os = "android") || backend == "Gl" || device == "Cpu" {
        return Tier::Low;
    }
    match device.as_str() {
        "DiscreteGpu" => Tier::Ultra,
        _ => Tier::Medium,
    }
}

/// Steps the tier down when frames stay slower than the budget: after `patience` seconds over `budget_ms * 1.25`
/// (smoothed), one tier lower, at most down to Low. It never raises the tier by itself (that would oscillate); a new
/// settings value starts it over.
#[derive(Resource, Clone, Debug)]
pub struct AutoQuality {
    pub enabled: bool,
    pub budget_ms: f32,
    pub patience: f32,
    over: f32,
    /// the tier the settings asked for, before the cap and the auto steps
    pub wanted: Option<Tier>,
    pub cap: Option<Tier>,
}

impl Default for AutoQuality {
    fn default() -> Self {
        // 144 fps is the phase-4 target on the owner's PC; below 60 is what triggers a step down
        AutoQuality { enabled: true, budget_ms: 1000.0 / 60.0, patience: 5.0, over: 0.0, wanted: None, cap: None }
    }
}

pub struct QualityPlugin;

impl Plugin for QualityPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::perf::PerfPlugin>() {
            app.add_plugins(crate::perf::PerfPlugin);
        }
        app.init_resource::<Quality>()
            .init_resource::<AutoQuality>()
            .add_systems(Update, (follow_settings, auto_step, apply_resources, apply_cameras, apply_lights, apply_cull).chain())
            .add_systems(Update, freeze_environment_maps);
    }
}

/// The settings' `graphics_quality`, capped for this GPU, and `max_fps` for the frame limiter. Runs when the player's
/// settings change (the client's `Controls` resource; absent in tools and the benchmark, which set `Quality` themselves).
fn follow_settings(
    controls: Option<Res<crate::player::Controls>>,
    adapter: Option<Res<RenderAdapterInfo>>,
    mut auto: ResMut<AutoQuality>,
    mut quality: ResMut<Quality>,
    mut limit: ResMut<crate::perf::FrameLimit>,
) {
    if auto.cap.is_none() {
        if let Some(a) = &adapter {
            let cap = tier_cap(a);
            auto.cap = Some(cap);
            info!("quality: {} ({:?}, {:?}) allows up to {cap:?}", a.name, a.device_type, a.backend);
        }
    }
    let Some(controls) = controls.filter(|c| c.is_changed()) else { return };
    let fps = controls.settings.get("max_fps") as f32;
    if limit.max_fps != Some(fps).filter(|f| *f > 0.0) {
        *limit = crate::perf::FrameLimit::new(Some(fps));
    }
    let wanted = Tier::from_index(controls.settings.get("graphics_quality").round() as i64);
    if auto.wanted == Some(wanted) {
        return;
    }
    auto.wanted = Some(wanted);
    auto.over = 0.0;
    let tier = wanted.min(auto.cap.unwrap_or(Tier::Ultra));
    if quality.tier != tier {
        *quality = Quality::tier(tier);
    }
}

fn auto_step(time: Res<Time<Real>>, frames: Res<Frames>, mut auto: ResMut<AutoQuality>, mut quality: ResMut<Quality>) {
    if !auto.enabled || frames.smoothed_ms == 0.0 || quality.tier == Tier::Low {
        return;
    }
    // a hitch while loading is not the steady frame rate: only frames under a quarter second count
    if frames.smoothed_ms > auto.budget_ms * 1.25 && frames.last.frame_ms < 250.0 {
        auto.over += time.delta_secs();
    } else {
        auto.over = 0.0;
    }
    if auto.over > auto.patience {
        auto.over = 0.0;
        let lower = quality.tier.lower();
        warn!("quality: {:.1} ms per frame for {:.0} s, stepping down to {lower:?}", frames.smoothed_ms, auto.patience);
        *quality = Quality::tier(lower);
    }
}

fn apply_resources(quality: Res<Quality>, mut sun: ResMut<DirectionalLightShadowMap>, mut point: ResMut<PointLightShadowMap>) {
    if !quality.is_changed() {
        return;
    }
    sun.size = quality.sun_shadow_size;
    point.size = quality.point_shadow_size;
}

/// Anti-aliasing and SSAO on the 3D cameras (new ones too).
fn apply_cameras(mut commands: Commands, quality: Res<Quality>, cameras: Query<Entity, With<Camera3d>>, new: Query<(), Added<Camera3d>>) {
    let changed = quality.is_changed();
    for cam in &cameras {
        if !changed && new.get(cam).is_err() {
            continue;
        }
        let mut e = commands.entity(cam);
        let msaa = if quality.anti_alias == AntiAlias::Msaa4 && !quality.ssao { Msaa::Sample4 } else { Msaa::Off };
        e.insert(msaa);
        match quality.anti_alias {
            AntiAlias::Fxaa => drop(e.insert(Fxaa::default()).remove::<Smaa>()),
            AntiAlias::Smaa(preset) => drop(e.insert(Smaa { preset }).remove::<Fxaa>()),
            AntiAlias::Off | AntiAlias::Msaa4 => drop(e.remove::<(Fxaa, Smaa)>()),
        }
        if quality.ssao {
            let level = if quality.tier == Tier::Ultra { ScreenSpaceAmbientOcclusionQualityLevel::High } else { ScreenSpaceAmbientOcclusionQualityLevel::Medium };
            e.insert(ScreenSpaceAmbientOcclusion { quality_level: level, ..default() });
        } else {
            e.remove::<ScreenSpaceAmbientOcclusion>();
        }
    }
}

/// Keeps an environment map filtering every frame (a sky that changes); without it a generated one is frozen.
#[derive(Component)]
pub struct LiveEnvironmentMap;

/// A `GeneratedEnvironmentMapLight` re-filters its cubemap on the GPU every frame (about 1 ms on an RTX 3060 Ti at
/// 1600x900, far more on a phone). Our skies are fixed, so once it has run a few frames the generator is removed;
/// the `EnvironmentMapLight` it made keeps the filtered result. Putting the component back (a new sky) runs it again.
fn freeze_environment_maps(
    mut commands: Commands,
    mut ages: Local<std::collections::HashMap<Entity, u32>>,
    maps: Query<(Entity, Ref<bevy::light::GeneratedEnvironmentMapLight>), (With<bevy::light::EnvironmentMapLight>, Without<LiveEnvironmentMap>)>,
) {
    ages.retain(|e, _| maps.contains(*e));
    for (e, g) in &maps {
        let age = ages.entry(e).or_default();
        if g.is_changed() {
            *age = 0;
        }
        *age += 1;
        // INFERRED margin: the filter runs once its source cubemap is on the GPU, a frame or two after it appears
        if *age > 30 {
            commands.entity(e).remove::<bevy::light::GeneratedEnvironmentMapLight>();
        }
    }
}

/// What the room asked for, kept so the light budget can switch shadows and lights back on.
#[derive(Component, Clone, Copy)]
pub struct LightWanted {
    shadows: bool,
    /// lumens over range, for ranking (brighter, wider lights matter more)
    weight: f32,
}

/// Cascades on directional lights; for point and spot lights, the ones nearest the camera (weighted by brightness)
/// keep their shadows up to `max_shadow_lights` and stay on up to `max_lights`. Re-ranked twice a second.
#[allow(clippy::type_complexity)]
fn apply_lights(
    mut commands: Commands,
    quality: Res<Quality>,
    time: Res<Time<Real>>,
    mut next: Local<f32>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    suns: Query<(Entity, Ref<DirectionalLight>)>,
    new_points: Query<(Entity, &PointLight), Without<LightWanted>>,
    new_spots: Query<(Entity, &SpotLight), Without<LightWanted>>,
    mut points: Query<(&GlobalTransform, &LightWanted, &mut PointLight, &mut Visibility), Without<SpotLight>>,
    mut spots: Query<(&GlobalTransform, &LightWanted, &mut SpotLight, &mut Visibility), Without<PointLight>>,
) {
    for (e, sun) in &suns {
        if quality.is_changed() || sun.is_added() {
            commands.entity(e).insert(CascadeShadowConfigBuilder {
                num_cascades: quality.sun_cascades,
                maximum_distance: quality.sun_shadow_distance,
                ..default()
            }.build());
        }
    }
    for (e, l) in &new_points {
        commands.entity(e).insert(LightWanted { shadows: l.shadow_maps_enabled, weight: l.intensity * l.range.max(0.1) });
    }
    for (e, l) in &new_spots {
        commands.entity(e).insert(LightWanted { shadows: l.shadow_maps_enabled, weight: l.intensity * l.range.max(0.1) });
    }
    *next -= time.delta_secs();
    if *next > 0.0 && !quality.is_changed() {
        return;
    }
    *next = 0.5;
    let Some(eye) = camera.iter().next().map(|c| c.translation()) else { return };
    let rank = |t: &GlobalTransform, w: &LightWanted| w.weight / (t.translation().distance_squared(eye) + 1.0);
    let mut all: Vec<(f32, bool, usize)> = points.iter().enumerate().map(|(i, (t, w, _, _))| (rank(t, w), false, i))
        .chain(spots.iter().enumerate().map(|(i, (t, w, _, _))| (rank(t, w), true, i))).collect();
    all.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut shadows_left = quality.max_shadow_lights;
    let (mut pi, mut si): (Vec<_>, Vec<_>) = (points.iter_mut().collect(), spots.iter_mut().collect());
    for (n, &(_, spot, i)) in all.iter().enumerate() {
        let on = n < quality.max_lights;
        let (wanted, shadow, vis) = if spot {
            let (_, w, l, v) = &mut si[i];
            (w.shadows, &mut l.shadow_maps_enabled, v)
        } else {
            let (_, w, l, v) = &mut pi[i];
            (w.shadows, &mut l.shadow_maps_enabled, v)
        };
        let cast = on && wanted && shadows_left > 0;
        if cast {
            shadows_left -= 1;
        }
        if *shadow != cast {
            *shadow = cast;
        }
        let v = if on { Visibility::Inherited } else { Visibility::Hidden };
        if **vis != v {
            **vis = v;
        }
    }
}

/// Distance culling: every mesh gets a `VisibilityRange` from its size (small ones go sooner), redone when the
/// quality changes. Meshes whose bounds are not known yet are picked up on a later frame.
fn apply_cull(
    mut commands: Commands,
    quality: Res<Quality>,
    meshes: Query<(Entity, &bevy::camera::primitives::Aabb, &GlobalTransform, Has<VisibilityRange>), With<Mesh3d>>,
    new: Query<(), Added<bevy::camera::primitives::Aabb>>,
) {
    let changed = quality.is_changed();
    for (e, aabb, t, has) in &meshes {
        if !changed && (has || new.get(e).is_err()) {
            continue;
        }
        let radius = (Vec3::from(aabb.half_extents) * t.scale()).length();
        let far = if quality.small_cull > 0.0 { (radius * quality.small_cull).clamp(8.0, quality.view_distance) } else { quality.view_distance };
        if far >= 999.0 {
            commands.entity(e).remove::<VisibilityRange>();
        } else {
            commands.entity(e).insert(VisibilityRange { start_margin: 0.0..0.0, end_margin: far..far * 1.1, use_aabb: true });
        }
    }
}

/// The GPU size of an image as uploaded: every mip level of every layer, in its texture format's blocks.
pub fn image_bytes(i: &Image) -> u64 {
    let d = &i.texture_descriptor;
    let (bw, bh) = d.format.block_dimensions();
    let block = d.format.block_copy_size(None).unwrap_or(4) as u64;
    let layers = d.size.depth_or_array_layers.max(1) as u64;
    (0..d.mip_level_count.max(1)).map(|m| {
        let (w, h) = ((d.size.width >> m).max(1), (d.size.height >> m).max(1));
        w.div_ceil(bw) as u64 * h.div_ceil(bh) as u64 * block
    }).sum::<u64>() * layers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_follow_the_setting() {
        assert_eq!(Quality::from_setting(0.0).tier, Tier::Low);
        assert_eq!(Quality::from_setting(2.0).tier, Tier::High);
        assert_eq!(Quality::from_setting(7.0).tier, Tier::Ultra);
        // SSAO tiers never ask for MSAA (Bevy's SSAO needs it off)
        for t in [Tier::Low, Tier::Medium, Tier::High, Tier::Ultra] {
            let q = Quality::tier(t);
            assert!(!(q.ssao && q.anti_alias == AntiAlias::Msaa4), "{t:?}");
        }
    }

    #[test]
    fn image_bytes_counts_mips_and_blocks() {
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let mut i = Image::new_fill(Extent3d { width: 256, height: 256, depth_or_array_layers: 1 }, TextureDimension::D2, &[0; 4], TextureFormat::Rgba8Unorm, default());
        assert_eq!(image_bytes(&i), 256 * 256 * 4);
        i.texture_descriptor.mip_level_count = 9;
        i.texture_descriptor.format = TextureFormat::Bc7RgbaUnorm;
        // 64x64 blocks of 16 bytes, then 32x32, ..., and 1 block each for the 4x4 and smaller levels
        let blocks: u64 = (0..9).map(|m| (((256u64 >> m).max(1)).div_ceil(4)).pow(2)).sum();
        assert_eq!(image_bytes(&i), blocks * 16);
    }
}
