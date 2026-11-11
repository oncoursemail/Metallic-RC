//! Real per-joint GPU skinning for the Rec Room avatar body. The owned bundle ships the body (and nose) as skinned
//! meshes over a 111-joint rig (`Jnt.Spine.Root` down): four bone weights + indices per vertex, a bind pose per joint,
//! and a Transform hierarchy. We read all of it, convert it from Unity's left-handed space into Bevy's the same way the
//! mesh is (a z reflection `M = diag(1,1,-1)`; every transform and bind pose is conjugated `M·A·M` so it stays
//! consistent with the z-flipped vertices), and hand Bevy a `SkinnedMesh`. The joints are spawned as an entity
//! hierarchy under each avatar so posing a joint deforms the mesh. See `anim`/`real` for how locomotion drives them.
use std::collections::HashMap;
use std::path::Path;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use unity_assets::{classes, Bundle, GameObject, Mesh as UMesh, SerializedFile, Transform as UTransform, Value};

use crate::room;

// The body + body-mesh id now come from the caller (`load` is parameterised per body type); only the nose id is fixed.
const NOSE_MESH_PATHID: i64 = 62350462907586393; // Nose_01Base_LOD0

/// One joint of the rig: its name, its local transform (Bevy space) and its parent among the joints (None = a root,
/// hung straight under the avatar's skeleton node).
pub struct Joint {
    pub name: String,
    pub local: Transform,
    pub parent: Option<usize>,
}

/// The whole rig, shared by every avatar.
pub struct RigDef {
    pub joints: Vec<Joint>,
    /// `Jnt.Head`, for hanging the face decals
    pub head: usize,
    pub name_to_index: HashMap<String, usize>,
    /// each joint's bind-pose transform in skeleton space (before the stand-up), for decal placement and as the base
    /// the animation rotates around
    pub bind_world: Vec<Mat4>,
}

impl RigDef {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.name_to_index.get(name).copied()
    }
}

/// Everything read from the bundle for the real body: the three region meshes (skin / shirt / pants, each carrying the
/// joint weights + indices), the inverse bind poses and the rig.
pub struct Loaded {
    pub meshes: [Mesh; 3],
    pub inverse_bindposes: Vec<Mat4>,
    pub rig: RigDef,
}

/// The z reflection that maps Unity's left-handed space to Bevy's (same as the mesh's `z -> -z`).
fn flip() -> Mat4 {
    Mat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0])
}

/// Conjugate a Unity transform/matrix into Bevy space: `M·A·M` (M is its own inverse).
fn conj(a: Mat4) -> Mat4 {
    let m = flip();
    m * a * m
}

/// A Unity `Matrix4x4f`'s 16 serialized floats are row-major (`e00,e01,e02,e03,e10,...`); glam is column-major.
fn mat_from_unity(f: &[f32]) -> Mat4 {
    let g = |r: usize, c: usize| f.get(r * 4 + c).copied().unwrap_or(0.0);
    Mat4::from_cols(
        Vec4::new(g(0, 0), g(1, 0), g(2, 0), g(3, 0)),
        Vec4::new(g(0, 1), g(1, 1), g(2, 1), g(3, 1)),
        Vec4::new(g(0, 2), g(1, 2), g(2, 2), g(3, 2)),
        Vec4::new(g(0, 3), g(1, 3), g(2, 3), g(3, 3)),
    )
}

/// The GameObject name a Transform object belongs to.
fn tf_name(sf: &SerializedFile, tf: &UTransform) -> String {
    sf.object(tf.game_object.path_id)
        .and_then(|o| sf.read_value(o).ok())
        .and_then(|gv| GameObject::from_value(&gv).ok())
        .map(|g| g.name)
        .unwrap_or_default()
}

/// The bone PPtr list and root path id of the first SkinnedMeshRenderer that draws `mesh_pathid`.
fn skinned_renderer(sf: &SerializedFile, mesh_pathid: i64) -> Option<(Vec<i64>, i64)> {
    for o in sf.objects.iter().filter(|o| o.class_id == classes::SKINNED_MESH_RENDERER) {
        let v = sf.read_value(o).ok()?;
        if v.get("m_Mesh").and_then(Value::as_pptr).map(|p| p.path_id) != Some(mesh_pathid) {
            continue;
        }
        let bones: Vec<i64> =
            v.get("m_Bones").and_then(Value::as_array)?.iter().filter_map(|p| p.as_pptr()).map(|p| p.path_id).collect();
        let root = v.get("m_RootBone").and_then(Value::as_pptr).map(|p| p.path_id).unwrap_or(0);
        return Some((bones, root));
    }
    None
}

/// Same as `skinned_renderer` but requires a given root bone, so the nose picks the skeleton instance the body uses.
fn skinned_renderer_with_root(sf: &SerializedFile, mesh_pathid: i64, root: i64) -> Option<Vec<i64>> {
    for o in sf.objects.iter().filter(|o| o.class_id == classes::SKINNED_MESH_RENDERER) {
        let Ok(v) = sf.read_value(o) else { continue };
        if v.get("m_Mesh").and_then(Value::as_pptr).map(|p| p.path_id) != Some(mesh_pathid) {
            continue;
        }
        if v.get("m_RootBone").and_then(Value::as_pptr).map(|p| p.path_id) != Some(root) {
            continue;
        }
        return v.get("m_Bones").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| p.as_pptr()).map(|p| p.path_id).collect());
    }
    None
}

/// The four bone weights and indices per vertex (channels 12/13); indices clamped into the joint range.
fn skin_channels(m: &UMesh, joints: usize) -> Option<(Vec<[f32; 4]>, Vec<[u16; 4]>, Vec<i64>)> {
    let (_, w) = m.channel(12)?;
    let (_, idx) = m.channel(13)?;
    let n = m.vertex_count;
    let mut weights = Vec::with_capacity(n);
    let mut indices = Vec::with_capacity(n);
    let mut raw_bone = Vec::with_capacity(n * 4); // original bone index, for the nose remap
    for v in 0..n {
        let wk: [f32; 4] = std::array::from_fn(|k| w.get(v * 4 + k).copied().unwrap_or(0.0));
        let ik: [u16; 4] = std::array::from_fn(|k| {
            let b = idx.get(v * 4 + k).copied().unwrap_or(0.0) as i64;
            raw_bone.push(b);
            b.clamp(0, joints as i64 - 1) as u16
        });
        weights.push(wk);
        indices.push(ik);
    }
    Some((weights, indices, raw_bone))
}

/// Which clothing zone a body triangle centre belongs to (height `h = -z`, left-right span `x`): head/forearms/hands
/// are skin (0), legs/feet are pants (2), the rest is shirt (1). (Kept in step with `real::zone`.) The legless Bean body
/// has no legs, so it gets NO pants zone — its whole torso is shirt (otherwise the torso bottom miscolours as pants).
fn zone(p: [f32; 3], bean: bool) -> usize {
    let (x, h) = (p[0].abs(), -p[2]);
    if h > 1.40 || x > 0.33 {
        0
    } else if !bean && h < 0.86 && x < 0.3 {
        2
    } else {
        1
    }
}

/// Positions, normals, uvs flipped into Bevy space (as `real`/`room` do).
fn read_geometry(m: &UMesh) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
    let pos: Vec<[f32; 3]> = m.positions().into_iter().map(flip).collect();
    let mut nrm: Vec<[f32; 3]> = m.normals().into_iter().map(flip).collect();
    nrm.resize(pos.len(), [0.0, 1.0, 0.0]);
    let mut uv = m.uv(0);
    uv.resize(pos.len(), [0.0, 0.0]);
    let uv: Vec<[f32; 2]> = uv.into_iter().map(|[u, v]| [u, 1.0 - v]).collect();
    let mut idx = Vec::new();
    for (i, sub) in m.submeshes.iter().enumerate() {
        if sub.topology != 0 {
            continue;
        }
        if let Some(mut s) = m.submesh_indices(i) {
            for tri in s.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
            idx.extend_from_slice(&s);
        }
    }
    (pos, nrm, uv, idx)
}

/// Loads a body type from its own bundle + body-mesh path id (Rec Room ships a Full body and a legless Bean body) as
/// three skinned region meshes plus the rig. `None` when the bundle or its skin data is missing.
pub fn load(dir: &Path, bundle: &str, body_id: i64, bean: bool) -> Option<Loaded> {
    let path = room::bundle_dir(dir).join(bundle);
    let b = Bundle::open(&path).ok()?;
    let files = b.serialized_files().ok()?;
    let sf = files.iter().find_map(|(_, sf)| sf.object(body_id).map(|_| sf))?;

    // the body's skinned renderer: its bone list (order matches the bind poses) and which skeleton instance it uses
    let (bones, root) = skinned_renderer(sf, body_id)?;
    let path_to_index: HashMap<i64, usize> = bones.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    // the body mesh: geometry + per-vertex skin + bind poses
    let body_o = sf.object(body_id)?;
    let body_v = sf.read_value(body_o).ok()?;
    let body_m = UMesh::from_value(&body_v, &b).ok()?;
    let (mut pos, mut nrm, mut uv, mut idx) = read_geometry(&body_m);
    let (mut weights, mut indices, _) = skin_channels(&body_m, bones.len())?;

    let bind = body_v.get("m_BindPose").and_then(Value::as_array)?;
    let inverse_bindposes: Vec<Mat4> = bind.iter().map(|e| conj(mat_from_unity(&e.floats()))).collect();
    if inverse_bindposes.len() != bones.len() {
        return None;
    }

    // the nose is omitted by default: Rec Room's face is a flat screen (eyes + mouth decals), and the low-poly nose
    // mesh reads as an uncanny human protrusion on it. Set AVATAR_NOSE=1 to include the real nose mesh.
    if std::env::var("AVATAR_NOSE").is_ok() {
        // the nose: same skeleton instance; append it, remapping its bone indices to our joint order
        if let Some(nose_bones) = skinned_renderer_with_root(sf, NOSE_MESH_PATHID, root) {
            if let Some(no) = sf.object(NOSE_MESH_PATHID).and_then(|o| sf.read_value(o).ok()) {
                if let Ok(nose_m) = UMesh::from_value(&no, &b) {
                    let (npos, nnrm, nuv, nidx) = read_geometry(&nose_m);
                    if let Some((nw, _, nraw)) = skin_channels(&nose_m, nose_bones.len()) {
                        let base = pos.len() as u32;
                        let remap = |orig: i64| -> u16 {
                            nose_bones.get(orig as usize).and_then(|id| path_to_index.get(id)).map(|&i| i as u16).unwrap_or(0)
                        };
                        for v in 0..nose_m.vertex_count {
                            let ik: [u16; 4] = std::array::from_fn(|k| remap(nraw[v * 4 + k]));
                            indices.push(ik);
                        }
                        weights.extend(nw);
                        pos.extend(npos);
                        nrm.extend(nnrm);
                        uv.extend(nuv);
                        idx.extend(nidx.iter().map(|i| i + base));
                    }
                }
            }
        }
    }

    // split the triangles into the three regions by their centre, sharing one vertex buffer
    let mut zones: [Vec<u32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for tri in idx.chunks_exact(3) {
        let c = [0, 1, 2].map(|k| pos[tri[k] as usize]);
        let mid = [0, 1, 2].map(|a| (c[0][a] + c[1][a] + c[2][a]) / 3.0);
        zones[zone(mid, bean)].extend_from_slice(tri);
    }
    let make = |region: Vec<u32>| {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(indices.clone()));
        mesh.insert_indices(Indices::U32(region));
        mesh
    };
    let meshes = [make(std::mem::take(&mut zones[0])), make(std::mem::take(&mut zones[1])), make(std::mem::take(&mut zones[2]))];

    // the joint hierarchy: each bone's name, local transform (conjugated) and parent among the bones
    let mut joints = Vec::with_capacity(bones.len());
    for &id in &bones {
        let o = sf.object(id)?;
        let v = sf.read_value(o).ok()?;
        let tf = UTransform::from_value(&v).ok()?;
        let name = tf_name(sf, &tf);
        let local = Transform::from_matrix(conj(Mat4::from_scale_rotation_translation(
            Vec3::from(tf.local_scale),
            Quat::from_xyzw(tf.local_rotation[0], tf.local_rotation[1], tf.local_rotation[2], tf.local_rotation[3]),
            Vec3::from(tf.local_position),
        )));
        let parent = path_to_index.get(&tf.parent.path_id).copied();
        joints.push(Joint { name, local, parent });
    }
    let name_to_index: HashMap<String, usize> = joints.iter().enumerate().map(|(i, j)| (j.name.clone(), i)).collect();
    let head = name_to_index.get("Jnt.Head").copied().unwrap_or(0);

    // bind-pose world of each joint in skeleton space (compose locals down from the roots)
    let mut bind_world = vec![None; joints.len()];
    fn world(i: usize, joints: &[Joint], cache: &mut Vec<Option<Mat4>>) -> Mat4 {
        if let Some(m) = cache[i] {
            return m;
        }
        let l = joints[i].local.to_matrix();
        let m = match joints[i].parent {
            Some(p) => world(p, joints, cache) * l,
            None => l,
        };
        cache[i] = Some(m);
        m
    }
    let bind_world: Vec<Mat4> = (0..joints.len()).map(|i| world(i, &joints, &mut bind_world)).collect();

    Some(Loaded { meshes, inverse_bindposes, rig: RigDef { joints, head, name_to_index, bind_world } })
}

/// Spawns the joint hierarchy under `parent` (the avatar's standing skeleton node) and returns an entity per joint,
/// indexed as the rig is. Roots hang straight under `parent`.
pub fn spawn_skeleton(commands: &mut Commands, rig: &RigDef, parent: Entity) -> Vec<Entity> {
    let mut entities: Vec<Entity> = Vec::with_capacity(rig.joints.len());
    // spawn in order; a parent always has a lower index than its child in this rig, but fall back to `parent` if not
    for (i, j) in rig.joints.iter().enumerate() {
        let under = match j.parent {
            Some(p) if p < i => entities[p],
            _ => parent,
        };
        let e = commands.spawn((j.local, Visibility::default(), ChildOf(under))).id();
        entities.push(e);
    }
    entities
}
