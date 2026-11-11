//! Built scenes (`levelN`) of player builds without type trees: the build's scene list, and the scene-level
//! objects a room needs (renderers with their lightmap use, colliders, lights, RenderSettings, LightmapSettings,
//! light and reflection probes). Layouts are the Unity 2019.4 / 2021.3 player ones; every reader checks that it
//! used up exactly the object's bytes, so a layout mismatch is an error, not garbage.

use crate::reader::Reader;
use crate::value::PPtr;
use crate::{classes, format_err, Result, SerializedFile};

/// Unity version as (major, minor), e.g. (2019, 4).
pub fn version_of(unity_version: &str) -> (u32, u32) {
    let mut it = unity_version.split(|c: char| !c.is_ascii_digit()).map(|s| s.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// Unity 6 (6000.x) and later: several player-build layouts gained fields (verified against the type trees of the
/// 2026 Addressables bundles and the decrypted 2026 base files, both 6000.0.27f1).
fn is_u6(sf: &SerializedFile) -> bool {
    version_of(&sf.unity_version).0 >= 6000
}

fn pptr(r: &mut Reader) -> Result<PPtr> {
    Ok(PPtr { file_id: r.i32()?, path_id: r.i64()? })
}

fn floats<const N: usize>(r: &mut Reader) -> Result<[f32; N]> {
    let mut out = [0.0; N];
    for f in &mut out {
        *f = r.f32()?;
    }
    Ok(out)
}

fn done(r: &Reader, what: &str) -> Result<()> {
    if r.pos() != r.len() {
        return format_err(format!("{what} read {} of {} bytes", r.pos(), r.len()));
    }
    Ok(())
}

/// The scene paths of the build (`BuildSettings.m_Scenes`, in `globalgamemanagers`): index i is `level{i}`.
pub fn build_scenes(ggm: &SerializedFile) -> Result<Vec<String>> {
    let Some(o) = ggm.objects.iter().find(|o| o.class_id == classes::BUILD_SETTINGS) else {
        return format_err("no BuildSettings object");
    };
    let mut r = Reader::new(ggm.object_bytes(o), ggm.big_endian);
    let n = r.len_prefix(4)?;
    (0..n).map(|_| r.aligned_string()).collect()
}

/// A scene path's short name: `Assets/Scenes/DormRoom2.unity` -> `DormRoom2`.
pub fn scene_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".unity").unwrap_or(file)
}

fn color(r: &mut Reader) -> Result<[f32; 4]> {
    floats::<4>(r)
}

fn vec_of<T>(r: &mut Reader, min: usize, mut f: impl FnMut(&mut Reader) -> Result<T>) -> Result<Vec<T>> {
    let n = r.len_prefix(min)?;
    (0..n).map(|_| f(r)).collect()
}

fn skip(r: &mut Reader, n: usize) -> Result<()> {
    r.bytes(n).map(|_| ())
}

/// The Behaviour start the components below share: `m_GameObject`, `m_Enabled` (aligned).
fn behaviour(r: &mut Reader) -> Result<(PPtr, bool)> {
    let go = pptr(r)?;
    let enabled = r.bool()?;
    r.align(4);
    Ok((go, enabled))
}

/// A MonoBehaviour's `m_Script` PPtr (points at the MonoScript, in this file or an external), so a caller can
/// resolve what gameplay script sits on a scene object (spawn points, NetObjects, triggers, teleporters).
pub fn mono_script_ref(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<PPtr> {
    let mut r = reader(sf, o, classes::MONO_BEHAVIOUR, "MonoBehaviour")?;
    behaviour(&mut r)?;
    pptr(&mut r)
}

/// A MonoScript's `(class_name, namespace)`. Layout: m_Name, m_ExecutionOrder (i32), m_PropertiesHash (16 bytes),
/// m_ClassName, m_Namespace, m_AssemblyName — stable across the 2019..6000 player builds.
pub fn mono_script_name(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<(String, String)> {
    let mut r = reader(sf, o, classes::MONO_SCRIPT, "MonoScript")?;
    r.aligned_string()?; // m_Name
    r.i32()?; // m_ExecutionOrder
    r.bytes(16)?; // m_PropertiesHash (Hash128)
    let class_name = r.aligned_string()?;
    let namespace = r.aligned_string()?;
    Ok((class_name, namespace))
}

/// The readers below know the 2019.3 .. 2021.3 player layouts (checked on 2019.4.26f1) and the Unity 6
/// (6000.x) player layout (checked on 6000.0.27f1, the decrypted 2026 Rec Room base files). Every reader
/// also verifies it consumed exactly the object's bytes, so a layout mismatch is an error, not garbage.
fn check_version(sf: &SerializedFile, what: &str) -> Result<()> {
    let v = version_of(&sf.unity_version);
    if !(((2019, 3)..(2022, 0)).contains(&v) || ((6000, 0)..(6001, 0)).contains(&v)) {
        return crate::unsupported(format!("{what} layout for Unity {}", sf.unity_version));
    }
    Ok(())
}

fn reader<'a>(sf: &'a SerializedFile, o: &crate::ObjectInfo, class: i32, what: &str) -> Result<Reader<'a>> {
    check_version(sf, what)?;
    if o.class_id != class {
        return format_err(format!("object {} is class {}, not {what}", o.path_id, o.class_id));
    }
    Ok(Reader::new(sf.object_bytes(o), sf.big_endian))
}

/// Light (108). Colour as stored (gamma space; Unity linearises it in a linear-space project).
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub game_object: PPtr,
    pub enabled: bool,
    /// 0 spot, 1 directional, 2 point, 3 area (rectangle), 4 disc
    pub kind: i32,
    pub color: [f32; 4],
    pub intensity: f32,
    pub range: f32,
    /// full cone angles, degrees
    pub spot_angle: f32,
    pub inner_spot_angle: f32,
    pub cookie_size: f32,
    /// 0 none, 1 hard, 2 soft
    pub shadows: i32,
    pub shadow_strength: f32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub cookie: PPtr,
    /// `m_Lightmapping`: 1 mixed, 2 baked, 4 realtime
    pub lightmapping: i32,
    pub culling_mask: u32,
    pub bounce_intensity: f32,
    /// Kelvin, used when `use_color_temperature`
    pub color_temperature: f32,
    pub use_color_temperature: bool,
}

pub fn light(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<Light> {
    let mut r = reader(sf, o, classes::LIGHT, "Light")?;
    let (game_object, enabled) = behaviour(&mut r)?;
    let kind = r.i32()?;
    if !is_u6(sf) {
        r.i32()?; // m_Shape (removed in Unity 6)
    }
    let color = color(&mut r)?;
    let [intensity, range, spot_angle, inner_spot_angle, cookie_size] = floats::<5>(&mut r)?;
    let shadows = r.i32()?;
    skip(&mut r, 8)?; // m_Resolution, m_CustomResolution
    let [shadow_strength, shadow_bias, shadow_normal_bias, _near_plane] = floats::<4>(&mut r)?;
    skip(&mut r, 64)?; // m_CullingMatrixOverride
    r.bool()?; // m_UseCullingMatrixOverride
    r.align(4);
    let cookie = pptr(&mut r)?;
    r.bool()?; // m_DrawHalo
    r.align(4);
    skip(&mut r, 16)?; // m_BakingOutput: occlusion light index, mask channel, bake type, mixed mode
    r.bool()?; // m_BakingOutput.isBaked
    r.align(4);
    pptr(&mut r)?; // m_Flare
    r.i32()?; // m_RenderMode
    let culling_mask = r.u32()?;
    r.u32()?; // m_RenderingLayerMask
    let lightmapping = r.i32()?;
    r.i32()?; // m_LightShadowCasterMode
    skip(&mut r, 8)?; // m_AreaSize
    let bounce_intensity = r.f32()?;
    let color_temperature = r.f32()?;
    let use_color_temperature = r.bool()?;
    r.align(4);
    skip(&mut r, 16)?; // m_BoundingSphereOverride
    if is_u6(sf) {
        r.bool()?; // m_UseBoundingSphereOverride
        r.bool()?; // m_UseViewFrustumForShadowCasterCull
        r.bool()?; // m_ForceVisible
        r.align(4);
        r.i32()?; // m_LightUnit
        r.f32()?; // m_LuxAtDistance
        r.bool()?; // m_EnableSpotReflector
        r.align(4);
    } else {
        r.bool()?; // m_UseBoundingSphereOverride
        r.align(4);
        if version_of(&sf.unity_version) >= (2020, 1) && r.remaining() == 4 {
            r.bool()?; // m_UseViewFrustumForShadowCasterCull (INFERRED version)
            r.align(4);
        }
    }
    done(&r, "Light")?;
    Ok(Light {
        game_object, enabled, kind, color, intensity, range, spot_angle, inner_spot_angle, cookie_size, shadows,
        shadow_strength, shadow_bias, shadow_normal_bias, cookie, lightmapping, culling_mask, bounce_intensity,
        color_temperature, use_color_temperature,
    })
}

/// RenderSettings (104): the scene's ambient light, fog, skybox and default reflection.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSettings {
    pub fog: bool,
    pub fog_color: [f32; 4],
    /// 1 linear, 2 exponential, 3 exponential squared
    pub fog_mode: i32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_end: f32,
    /// ambient colours (gamma space): sky, equator, ground
    pub ambient_sky: [f32; 4],
    pub ambient_equator: [f32; 4],
    pub ambient_ground: [f32; 4],
    pub ambient_intensity: f32,
    /// 0 skybox, 1 trilight (gradient), 3 flat, 4 custom
    pub ambient_mode: i32,
    pub subtractive_shadow_color: [f32; 4],
    pub skybox_material: PPtr,
    /// 0 skybox, 1 custom (`custom_reflection`)
    pub reflection_mode: i32,
    pub reflection_resolution: i32,
    pub reflection_bounces: i32,
    pub reflection_intensity: f32,
    pub custom_reflection: PPtr,
    /// the baked ambient probe: SH L2, 9 coefficients per channel, R then G then B (linear)
    pub ambient_probe: [f32; 27],
    /// the Light the sky's sun follows
    pub sun: PPtr,
    pub indirect_specular_color: [f32; 4],
}

pub fn render_settings(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<RenderSettings> {
    let mut r = reader(sf, o, classes::RENDER_SETTINGS, "RenderSettings")?;
    let fog = r.bool()?;
    r.align(4);
    let fog_color = color(&mut r)?;
    let fog_mode = r.i32()?;
    let [fog_density, fog_start, fog_end] = floats::<3>(&mut r)?;
    let (ambient_sky, ambient_equator, ambient_ground) = (color(&mut r)?, color(&mut r)?, color(&mut r)?);
    let ambient_intensity = r.f32()?;
    let ambient_mode = r.i32()?;
    let subtractive_shadow_color = color(&mut r)?;
    let skybox_material = pptr(&mut r)?;
    skip(&mut r, 12)?; // halo strength, flare strength, flare fade speed
    pptr(&mut r)?; // m_HaloTexture
    pptr(&mut r)?; // m_SpotCookie
    let reflection_mode = r.i32()?;
    let reflection_resolution = r.i32()?;
    let reflection_bounces = r.i32()?;
    let reflection_intensity = r.f32()?;
    let custom_reflection = pptr(&mut r)?;
    let ambient_probe = floats::<27>(&mut r)?;
    pptr(&mut r)?; // m_GeneratedSkyboxReflection
    let sun = pptr(&mut r)?;
    let indirect_specular_color = color(&mut r)?;
    r.bool()?; // m_UseRadianceAmbientProbe
    if r.remaining() > 0 {
        r.align(4);
    }
    done(&r, "RenderSettings")?;
    Ok(RenderSettings {
        fog, fog_color, fog_mode, fog_density, fog_start, fog_end, ambient_sky, ambient_equator, ambient_ground,
        ambient_intensity, ambient_mode, subtractive_shadow_color, skybox_material, reflection_mode,
        reflection_resolution, reflection_bounces, reflection_intensity, custom_reflection, ambient_probe, sun,
        indirect_specular_color,
    })
}

/// One baked lightmap set: colour, directional and shadowmask textures (null when absent).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightmapData {
    pub color: PPtr,
    pub directional: PPtr,
    pub shadow_mask: PPtr,
}

/// LightmapSettings (157).
#[derive(Debug, Clone, PartialEq)]
pub struct LightmapSettings {
    pub light_probes: PPtr,
    /// indexed by a renderer's `lightmap_index`
    pub lightmaps: Vec<LightmapData>,
    /// 0 non-directional, 1 combined directional
    pub mode: i32,
    pub bounce_scale: f32,
    pub indirect_output_scale: f32,
    pub albedo_boost: f32,
    pub environment_lighting_mode: u32,
    pub baked_lightmaps: bool,
    pub realtime_lightmaps: bool,
}

/// Scenes with Enlighten realtime GI data (a non-empty `m_EnlightenSceneMapping`) are not read.
pub fn lightmap_settings(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<LightmapSettings> {
    let mut r = reader(sf, o, classes::LIGHTMAP_SETTINGS, "LightmapSettings")?;
    for _ in 0..5 {
        if r.i32()? != 0 {
            return crate::unsupported("LightmapSettings with an Enlighten scene mapping");
        }
    }
    let light_probes = pptr(&mut r)?;
    let lightmaps = vec_of(&mut r, 36, |r| Ok(LightmapData { color: pptr(r)?, directional: pptr(r)?, shadow_mask: pptr(r)? }))?;
    // Unity 6 (6000.x) inserts a 4-byte field between m_Lightmaps and m_LightmapsMode. INFERRED from the
    // decrypted 2026 base scenes (6000.0.27f1): it reads 0 in every sampled scene. Layout verified by the
    // cipher session's byte dumps of the decrypted corpus (session sweet-galileo).
    let u6 = is_u6(sf);
    if u6 {
        r.i32()?;
    }
    let mode = r.i32()?;
    let [bounce_scale, indirect_output_scale, albedo_boost] = floats::<3>(&mut r)?;
    let environment_lighting_mode = r.u32()?;
    let baked_lightmaps = r.bool()?;
    let realtime_lightmaps = r.bool()?;
    r.align(4);
    if u6 {
        // Unity 6 tail is three i32 (0x40-0x4b): m_UseShadowmask and two unidentified fields (candidates
        // m_ShadowMaskMode and a LightingDataAsset tail), replacing the 2019/2021 single-bool + align.
        r.i32()?;
        r.i32()?;
        r.i32()?;
    } else {
        r.bool()?; // m_UseShadowmask
        if r.remaining() > 0 {
            r.align(4);
        }
    }
    done(&r, "LightmapSettings")?;
    Ok(LightmapSettings {
        light_probes, lightmaps, mode, bounce_scale, indirect_output_scale, albedo_boost, environment_lighting_mode,
        baked_lightmaps, realtime_lightmaps,
    })
}

/// LightProbes (258): probe positions and their baked SH L2 (27 floats each, as in [`RenderSettings::ambient_probe`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LightProbes {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub coefficients: Vec<[f32; 27]>,
    /// tetrahedra over the probes: four probe indices each
    pub tetrahedra: Vec<[i32; 4]>,
}

pub fn light_probes(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<LightProbes> {
    let mut r = reader(sf, o, classes::LIGHT_PROBES, "LightProbes")?;
    let name = r.aligned_string()?;
    // m_Data.m_Tetrahedralization: tetrahedra (indices, neighbours, 3x4 matrix), hull rays
    let tetrahedra = vec_of(&mut r, 80, |r| {
        let i = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
        skip(r, 16 + 48)?;
        Ok(i)
    })?;
    vec_of(&mut r, 12, floats::<3>)?; // m_HullRays
    vec_of(&mut r, 24, |r| skip(r, 24))?; // m_ProbeSets (hash, offset, size)
    let positions = vec_of(&mut r, 12, floats::<3>)?;
    vec_of(&mut r, 20, |r| skip(r, 20))?; // m_NonTetrahedralizedProbeSetIndexMap
    let coefficients = vec_of(&mut r, 108, floats::<27>)?;
    vec_of(&mut r, 36, |r| skip(r, 36))?; // m_BakedLightOcclusion
    done(&r, "LightProbes")?;
    Ok(LightProbes { name, positions, coefficients, tetrahedra })
}

/// ReflectionProbe (215).
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectionProbe {
    pub game_object: PPtr,
    pub enabled: bool,
    /// 0 baked, 1 realtime, 2 custom
    pub mode: i32,
    pub resolution: i32,
    pub box_size: [f32; 3],
    pub box_offset: [f32; 3],
    pub intensity: f32,
    pub blend_distance: f32,
    pub hdr: bool,
    pub box_projection: bool,
    pub importance: i16,
    pub custom_texture: PPtr,
    pub baked_texture: PPtr,
}

pub fn reflection_probe(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<ReflectionProbe> {
    let mut r = reader(sf, o, classes::REFLECTION_PROBE, "ReflectionProbe")?;
    let (game_object, enabled) = behaviour(&mut r)?;
    r.i32()?; // m_Type
    let mode = r.i32()?;
    skip(&mut r, 8)?; // m_RefreshMode, m_TimeSlicingMode
    let resolution = r.i32()?;
    r.i32()?; // m_UpdateFrequency
    let box_size = floats::<3>(&mut r)?;
    let box_offset = floats::<3>(&mut r)?;
    skip(&mut r, 12 + 4 + 16 + 4)?; // near, far, shadow distance, clear flags, background colour, culling mask
    let intensity = r.f32()?;
    let blend_distance = r.f32()?;
    let hdr = r.bool()?;
    let box_projection = r.bool()?;
    skip(&mut r, 2)?; // m_RenderDynamicObjects, m_UseOcclusionCulling
    let importance = r.i16()?;
    r.align(4);
    let custom_texture = pptr(&mut r)?;
    let baked_texture = pptr(&mut r)?;
    done(&r, "ReflectionProbe")?;
    Ok(ReflectionProbe {
        game_object, enabled, mode, resolution, box_size, box_offset, intensity, blend_distance, hdr, box_projection,
        importance, custom_texture, baked_texture,
    })
}

/// LODGroup (205): the renderers at each level of detail, level 0 (highest detail) first.
#[derive(Debug, Clone, PartialEq)]
pub struct LodGroup {
    pub game_object: PPtr,
    /// renderer PPtrs per LOD level; `lods[0]` is LOD0 (drawn), `lods[1..]` are the lower levels
    pub lods: Vec<Vec<PPtr>>,
    pub enabled: bool,
}

/// LODGroup (205) without type tree. Layout verified on the decrypted 2026 scenes (6000.0.27f1): m_GameObject,
/// m_LocalReferencePoint (Vec3), m_Size, m_FadeMode (int), m_AnimateCrossFading (bool+align), m_LODs
/// (each: screenRelativeTransitionHeight f32, fadeTransitionWidth f32, renderers vector of PPtr), m_Enabled.
pub fn lod_group(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<LodGroup> {
    let mut r = reader(sf, o, classes::LOD_GROUP, "LODGroup")?;
    let game_object = pptr(&mut r)?;
    floats::<3>(&mut r)?; // m_LocalReferencePoint
    r.f32()?; // m_Size
    r.i32()?; // m_FadeMode
    r.bool()?; // m_AnimateCrossFading
    r.align(4);
    let count = r.i32()? as usize;
    let mut lods = Vec::with_capacity(count.min(16));
    for _ in 0..count {
        r.f32()?; // screenRelativeTransitionHeight
        r.f32()?; // fadeTransitionWidth
        let rn = r.i32()? as usize;
        let mut rs = Vec::with_capacity(rn.min(64));
        for _ in 0..rn {
            rs.push(pptr(&mut r)?);
        }
        lods.push(rs);
    }
    let enabled = r.i32()? != 0; // m_Enabled
    done(&r, "LODGroup")?;
    Ok(LodGroup { game_object, lods, enabled })
}

/// BoxCollider (65).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxCollider {
    pub game_object: PPtr,
    pub trigger: bool,
    pub enabled: bool,
    pub size: [f32; 3],
    pub center: [f32; 3],
}

/// The shared Collider prefix (`m_GameObject`, `m_Material`, the Unity 6 layer block, `m_IsTrigger`,
/// `m_ProvidesContacts`, `m_Enabled`), leaving the reader at the shape-specific fields.
fn collider_base(r: &mut Reader, sf: &SerializedFile) -> Result<(PPtr, bool, bool)> {
    let game_object = pptr(r)?;
    pptr(r)?; // m_Material
    if is_u6(sf) {
        skip(r, 12)?; // m_IncludeLayers, m_ExcludeLayers (BitField), m_LayerOverridePriority
        let trigger = r.bool()?;
        r.bool()?; // m_ProvidesContacts
        r.align(4);
        let enabled = r.bool()?;
        r.align(4);
        Ok((game_object, trigger, enabled))
    } else {
        let trigger = r.bool()?;
        let enabled = r.bool()?;
        r.align(4);
        Ok((game_object, trigger, enabled))
    }
}

pub fn box_collider(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<BoxCollider> {
    let mut r = reader(sf, o, classes::BOX_COLLIDER, "BoxCollider")?;
    let (game_object, trigger, enabled) = collider_base(&mut r, sf)?;
    let size = floats::<3>(&mut r)?;
    let center = floats::<3>(&mut r)?;
    done(&r, "BoxCollider")?;
    Ok(BoxCollider { game_object, trigger, enabled, size, center })
}

/// SphereCollider (135).
#[derive(Debug, Clone, PartialEq)]
pub struct SphereCollider {
    pub game_object: PPtr,
    pub trigger: bool,
    pub enabled: bool,
    pub radius: f32,
    pub center: [f32; 3],
}

pub fn sphere_collider(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<SphereCollider> {
    let mut r = reader(sf, o, classes::SPHERE_COLLIDER, "SphereCollider")?;
    let (game_object, trigger, enabled) = collider_base(&mut r, sf)?;
    let radius = r.f32()?;
    let center = floats::<3>(&mut r)?;
    done(&r, "SphereCollider")?;
    Ok(SphereCollider { game_object, trigger, enabled, radius, center })
}

/// CapsuleCollider (136). `direction`: 0 = X, 1 = Y, 2 = Z (the axis the height runs along).
#[derive(Debug, Clone, PartialEq)]
pub struct CapsuleCollider {
    pub game_object: PPtr,
    pub trigger: bool,
    pub enabled: bool,
    pub radius: f32,
    pub height: f32,
    pub direction: i32,
    pub center: [f32; 3],
}

pub fn capsule_collider(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<CapsuleCollider> {
    let mut r = reader(sf, o, classes::CAPSULE_COLLIDER, "CapsuleCollider")?;
    let (game_object, trigger, enabled) = collider_base(&mut r, sf)?;
    let radius = r.f32()?;
    let height = r.f32()?;
    let direction = r.i32()?;
    let center = floats::<3>(&mut r)?;
    done(&r, "CapsuleCollider")?;
    Ok(CapsuleCollider { game_object, trigger, enabled, radius, height, direction, center })
}

/// MeshCollider (64): a collision surface from an arbitrary mesh (`mesh` points at the Mesh to use).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshCollider {
    pub game_object: PPtr,
    pub trigger: bool,
    pub enabled: bool,
    pub convex: bool,
    pub mesh: PPtr,
}

pub fn mesh_collider(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<MeshCollider> {
    let mut r = reader(sf, o, classes::MESH_COLLIDER, "MeshCollider")?;
    let (game_object, trigger, enabled) = collider_base(&mut r, sf)?;
    let convex = r.bool()?;
    r.align(4);
    r.i32()?; // m_CookingOptions
    let mesh = pptr(&mut r)?;
    done(&r, "MeshCollider")?;
    Ok(MeshCollider { game_object, trigger, enabled, convex, mesh })
}

/// Out-of-line data (`.resS` / `.resource`) of plain files in a player's data folder.
pub struct DirResources(pub std::path::PathBuf);

impl crate::objects::Resources for DirResources {
    fn resource(&self, path: &str, offset: u64, size: u64) -> Result<Vec<u8>> {
        MultiResources(vec![self.0.clone()]).resource(path, offset, size)
    }
}

/// Out-of-line data looked up across several folders, first hit wins. The decrypted 2026 base files live in one
/// folder while their `.resS` streams live in the game install's `RecRoom_Data`, so both are searched.
pub struct MultiResources(pub Vec<std::path::PathBuf>);

impl crate::objects::Resources for MultiResources {
    fn resource(&self, path: &str, offset: u64, size: u64) -> Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let mut last = None;
        for dir in &self.0 {
            match std::fs::File::open(dir.join(name)) {
                Ok(mut f) => {
                    f.seek(SeekFrom::Start(offset))?;
                    let mut v = vec![0; size as usize];
                    match f.read_exact(&mut v) {
                        Ok(()) => return Ok(v),
                        Err(e) => last = Some(e),
                    }
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.map_or_else(|| crate::Error::Format(format!("resource {name} not found")), crate::Error::Io))
    }
}

fn byte_vec(r: &mut Reader) -> Result<Vec<u8>> {
    let n = r.len_prefix(1)?;
    let v = r.bytes(n)?.to_vec();
    r.align(4);
    Ok(v)
}

/// `StreamingInfo`: offset (u32 before 2020, u64 since), size, path. `None` when the data is inline.
fn stream_info(r: &mut Reader, v: (u32, u32)) -> Result<Option<(u64, u64, String)>> {
    let offset = if v >= (2020, 1) { r.u64()? } else { r.u32()? as u64 };
    let size = r.u32()? as u64;
    let path = r.aligned_string()?;
    Ok((size > 0 && !path.is_empty()).then_some((offset, size, path)))
}

/// Mesh (43) without type tree, Unity 2019.3 .. 2021.3 player layout; uncompressed meshes only.
pub fn mesh(sf: &SerializedFile, o: &crate::ObjectInfo, res: &dyn crate::objects::Resources) -> Result<crate::Mesh> {
    let mut r = reader(sf, o, classes::MESH, "Mesh")?;
    let v = version_of(&sf.unity_version);
    let name = r.aligned_string()?;
    let submeshes = vec_of(&mut r, 48, |r| {
        let s = crate::SubMesh {
            first_byte: r.u32()?,
            index_count: r.u32()?,
            topology: r.i32()?,
            base_vertex: r.u32()?,
            first_vertex: r.u32()?,
            vertex_count: r.u32()?,
        };
        skip(r, 24)?; // localAABB
        Ok(s)
    })?;
    // m_Shapes: vertices, shapes, channels, full weights
    vec_of(&mut r, 40, |r| skip(r, 40))?;
    vec_of(&mut r, 12, |r| skip(r, 12))?;
    vec_of(&mut r, 16, |r| {
        r.aligned_string()?;
        skip(r, 12)
    })?;
    vec_of(&mut r, 4, |r| skip(r, 4))?;
    vec_of(&mut r, 64, |r| skip(r, 64))?; // m_BindPose
    vec_of(&mut r, 4, |r| skip(r, 4))?; // m_BoneNameHashes
    r.u32()?; // m_RootBoneNameHash
    vec_of(&mut r, 24, |r| skip(r, 24))?; // m_BonesAABB
    vec_of(&mut r, 4, |r| skip(r, 4))?; // m_VariableBoneCountWeights
    let compression = r.u8()?;
    skip(&mut r, 3)?; // readable, keep vertices, keep indices
    r.align(4);
    if compression != 0 {
        return crate::unsupported(format!("compressed mesh {name}"));
    }
    let index_format = r.i32()?;
    let index_buffer = byte_vec(&mut r)?;
    let vertex_count = r.u32()? as usize;
    let channels = vec_of(&mut r, 4, |r| {
        Ok(crate::VertexChannel { stream: r.u8()?, offset: r.u8()?, format: r.u8()?, dimension: r.u8()? & 0xf })
    })?;
    let inline = byte_vec(&mut r)?;
    // m_CompressedMesh: vertices, uv, normals, tangents (float packed), weights, normal signs, tangent signs
    // (int packed), float colours, bone indices, triangles, then m_UVInfo
    let packed = |r: &mut Reader, float: bool| -> Result<()> {
        r.u32()?;
        if float {
            skip(r, 8)?;
        }
        byte_vec(r)?;
        r.u8()?;
        r.align(4);
        Ok(())
    };
    for float in [true, true, true, true, false, false, false, true, false, false] {
        packed(&mut r, float)?;
    }
    r.u32()?; // m_UVInfo
    // m_LocalAABB (24), m_MeshUsageFlags (4), and in Unity 6 an added m_CookingOptions (4)
    skip(&mut r, 24 + 4 + if is_u6(sf) { 4 } else { 0 })?;
    byte_vec(&mut r)?; // m_BakedConvexCollisionMesh
    byte_vec(&mut r)?; // m_BakedTriangleCollisionMesh
    skip(&mut r, 8)?; // m_MeshMetrics
    let streamed = stream_info(&mut r, v)?;
    done(&r, "Mesh")?;
    let vertex_data = match streamed {
        Some((offset, size, path)) => res.resource(&path, offset, size)?,
        None => inline,
    };
    Ok(crate::Mesh { name, vertex_count, channels, vertex_data, index_format, index_buffer, submeshes })
}

/// Texture2D (28) or Cubemap (89, six images) without type tree, Unity 2019.3 .. 2021.3 player layout.
pub fn texture(sf: &SerializedFile, o: &crate::ObjectInfo, res: &dyn crate::objects::Resources) -> Result<crate::Texture2D> {
    let class = if o.class_id == classes::CUBEMAP { classes::CUBEMAP } else { classes::TEXTURE_2D };
    let mut r = reader(sf, o, class, "Texture2D")?;
    let v = version_of(&sf.unity_version);
    let name = r.aligned_string()?;
    let (width, height, format, mip_count, image_count, inline, streamed) = if is_u6(sf) {
        // Unity 6 Texture2D player layout (from the type tree): no m_ForcedFallbackFormat/m_DownscaleFallback;
        // adds m_MipsStripped, m_MipmapLimitGroupName, m_IsPreProcessed, m_IgnoreMipmapLimit.
        r.bool()?; // m_IsAlphaChannelOptional
        r.align(4);
        let width = r.i32()? as usize;
        let height = r.i32()? as usize;
        r.i32()?; // m_CompleteImageSize
        r.i32()?; // m_MipsStripped
        let format = r.i32()?;
        let mip_count = r.i32()?;
        skip(&mut r, 3)?; // m_IsReadable, m_IsPreProcessed, m_IgnoreMipmapLimit
        r.align(4);
        r.aligned_string()?; // m_MipmapLimitGroupName
        r.bool()?; // m_StreamingMipmaps
        r.align(4);
        r.i32()?; // m_StreamingMipmapsPriority
        let image_count = r.i32()?;
        skip(&mut r, 4 + 24 + 8)?; // m_TextureDimension, m_TextureSettings, lightmap format, colour space
        byte_vec(&mut r)?; // m_PlatformBlob
        let inline = byte_vec(&mut r)?;
        let streamed = stream_info(&mut r, v)?;
        if class == classes::CUBEMAP {
            vec_of(&mut r, 12, pptr)?; // m_SourceTextures
        }
        (width, height, format, mip_count, image_count, inline, streamed)
    } else {
        r.i32()?; // m_ForcedFallbackFormat
        r.bool()?; // m_DownscaleFallback (+ m_IsAlphaChannelOptional since 2020.2)
        r.align(4);
        let width = r.i32()? as usize;
        let height = r.i32()? as usize;
        r.i32()?; // m_CompleteImageSize
        let format = r.i32()?;
        let mip_count = r.i32()?;
        skip(&mut r, 3)?; // readable, ignore master texture limit, streaming mipmaps
        r.align(4);
        r.i32()?; // m_StreamingMipmapsPriority
        let image_count = r.i32()?;
        skip(&mut r, 4 + 24 + 8)?; // dimension, texture settings, lightmap format, colour space
        if v >= (2020, 2) {
            byte_vec(&mut r)?; // m_PlatformBlob
        }
        let inline = byte_vec(&mut r)?;
        let streamed = stream_info(&mut r, v)?;
        if class == classes::CUBEMAP {
            vec_of(&mut r, 12, pptr)?; // m_SourceTextures
        }
        (width, height, format, mip_count, image_count, inline, streamed)
    };
    done(&r, "Texture2D")?;
    let data = match streamed {
        Some((offset, size, path)) => res.resource(&path, offset, size)?,
        None => inline,
    };
    Ok(crate::Texture2D { name, width, height, format, mip_count, image_count, data })
}

/// Material (21) without type tree, Unity 2019.3 .. 2021.3 player layout.
pub fn material(sf: &SerializedFile, o: &crate::ObjectInfo) -> Result<crate::Material> {
    let mut r = reader(sf, o, classes::MATERIAL, "Material")?;
    let v = version_of(&sf.unity_version);
    let name = r.aligned_string()?;
    let shader = pptr(&mut r)?;
    let keywords = if v >= (2021, 2) {
        let valid = vec_of(&mut r, 4, |r| r.aligned_string())?;
        vec_of(&mut r, 4, |r| r.aligned_string())?; // m_InvalidKeywords
        valid
    } else {
        r.aligned_string()?.split_whitespace().map(str::to_string).collect()
    };
    r.u32()?; // m_LightmapFlags
    skip(&mut r, 2)?; // instancing variants, double-sided GI
    r.align(4);
    r.i32()?; // m_CustomRenderQueue
    vec_of(&mut r, 8, |r| Ok((r.aligned_string()?, r.aligned_string()?)))?; // stringTagMap
    vec_of(&mut r, 4, |r| r.aligned_string())?; // disabledShaderPasses
    if ((2020, 1)..(2022, 0)).contains(&v) {
        // m_BuildTextureStacks (2020.1 .. 2021.x only; Unity 6 player materials do not serialize it)
        vec_of(&mut r, 8, |r| Ok((r.aligned_string()?, r.aligned_string()?)))?;
    }
    let textures = vec_of(&mut r, 32, |r| {
        let name = r.aligned_string()?;
        let texture = pptr(r)?;
        let [sx, sy, ox, oy] = floats::<4>(r)?;
        Ok(crate::TexEnv { name, texture, scale: [sx, sy], offset: [ox, oy] })
    })?;
    let ints = if v >= (2021, 1) { vec_of(&mut r, 8, |r| Ok((r.aligned_string()?, r.i32()? as i64)))? } else { Vec::new() };
    let floats = vec_of(&mut r, 8, |r| Ok((r.aligned_string()?, r.f32()?)))?;
    let colors = vec_of(&mut r, 20, |r| Ok((r.aligned_string()?, color(r)?)))?;
    if is_u6(sf) {
        // Unity 6 serializes m_BuildTextureStacks at the end, after m_SavedProperties (groupName, itemName strings).
        vec_of(&mut r, 8, |r| {
            r.aligned_string()?;
            r.aligned_string()?;
            Ok(())
        })?;
    }
    done(&r, "Material")?;
    Ok(crate::Material { name, shader, keywords, textures, floats, ints, colors })
}
