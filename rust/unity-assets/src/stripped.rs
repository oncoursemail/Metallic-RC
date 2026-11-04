//! Hand-written readers for player builds without type trees (`enableTypeTree = 0`).
//!
//! Layouts are the Unity 2021.3 player ones (no editor-only fields). GameObject, Transform,
//! MeshFilter and the `m_Name` of named objects have not changed since 2017; the Renderer flag
//! bytes depend on the Unity version, which [`renderer_flag_bytes`] takes into account.

use crate::objects::{GameObject, MeshFilter, MeshRenderer, Transform};
use crate::reader::Reader;
use crate::value::PPtr;
use crate::{format_err, Result};

fn pptr(r: &mut Reader) -> Result<PPtr> {
    Ok(PPtr { file_id: r.i32()?, path_id: r.i64()? })
}

fn pptrs(r: &mut Reader) -> Result<Vec<PPtr>> {
    let n = r.len_prefix(12)?;
    (0..n).map(|_| pptr(r)).collect()
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

/// GameObject (1): `m_Component` (vector of PPtr<Component>), `m_Layer`, `m_Name`, `m_Tag`, `m_IsActive`.
pub fn game_object(r: &mut Reader) -> Result<GameObject> {
    let components = pptrs(r)?;
    let layer = r.u32()?;
    let name = r.aligned_string()?;
    let tag = r.u16()?;
    let active = r.bool()?;
    // the object's byte size may or may not include the trailing alignment
    if r.remaining() > 0 {
        r.align(4);
    }
    done(r, "GameObject")?;
    Ok(GameObject { name, components, layer, tag, active })
}

/// Transform (4), or the leading Transform part of a RectTransform (224) when `exact` is false.
/// Order on disk: `m_GameObject`, `m_LocalRotation`, `m_LocalPosition`, `m_LocalScale`, `m_Children`, `m_Father`.
pub fn transform(r: &mut Reader, exact: bool) -> Result<Transform> {
    let game_object = pptr(r)?;
    let local_rotation = floats::<4>(r)?;
    let local_position = floats::<3>(r)?;
    let local_scale = floats::<3>(r)?;
    let children = pptrs(r)?;
    let parent = pptr(r)?;
    if exact {
        done(r, "Transform")?;
    }
    Ok(Transform { game_object, local_position, local_rotation, local_scale, parent, children })
}

/// MeshFilter (33): `m_GameObject`, `m_Mesh`.
pub fn mesh_filter(r: &mut Reader) -> Result<MeshFilter> {
    let game_object = pptr(r)?;
    let mesh = pptr(r)?;
    done(r, "MeshFilter")?;
    Ok(MeshFilter { game_object, mesh })
}

/// Number of single-byte Renderer settings after `m_Enabled` (CastShadows, ReceiveShadows,
/// DynamicOccludee, StaticShadowCaster, MotionVectors, LightProbeUsage, ReflectionProbeUsage,
/// RayTracingMode, RayTraceProcedural), by Unity version.
pub fn renderer_flag_bytes(unity_version: &str) -> usize {
    let mut it = unity_version.split(|c: char| !c.is_ascii_digit()).map(|s| s.parse::<u32>().unwrap_or(0));
    let (major, minor) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
    let mut n = 2; // CastShadows, ReceiveShadows
    if (major, minor) >= (2017, 2) {
        n += 1; // DynamicOccludee
    }
    if major >= 2021 {
        n += 1; // StaticShadowCaster
    }
    n += 3; // MotionVectors, LightProbeUsage, ReflectionProbeUsage
    if (major, minor) >= (2019, 3) {
        n += 1; // RayTracingMode
    }
    if major >= 2020 {
        n += 1; // RayTraceProcedural
    }
    n
}

/// MeshRenderer (23): reads up to `m_StaticBatchInfo`; the rest (probe anchors, sorting) is skipped.
pub fn mesh_renderer(r: &mut Reader, unity_version: &str) -> Result<MeshRenderer> {
    let game_object = pptr(r)?;
    let enabled = r.bool()?;
    let cast_shadows = r.bytes(renderer_flag_bytes(unity_version))?[0];
    r.align(4);
    r.u32()?; // m_RenderingLayerMask (2018+)
    r.i32()?; // m_RendererPriority (2018.3+)
    let lightmap_index = r.u16()?;
    r.u16()?; // m_LightmapIndexDynamic
    let lightmap_st = floats::<4>(r)?;
    floats::<4>(r)?; // m_LightmapTilingOffsetDynamic
    let materials = pptrs(r)?;
    // m_StaticBatchInfo (firstSubMesh, subMeshCount); absent only in truncated test objects
    let static_batch = if r.remaining() >= 4 { (r.u16()?, r.u16()?) } else { (0, 0) };
    Ok(MeshRenderer { game_object, enabled, materials, mesh: None, cast_shadows, lightmap_index, lightmap_st, static_batch })
}

/// Classes whose first field is `m_Name` (NamedObject): Material, Texture2D, Mesh, and a few others.
pub fn starts_with_name(class_id: i32) -> bool {
    matches!(class_id, 21 | 28 | 43 | 49 | 74 | 83 | 89 | 117 | 187 | 213)
}
