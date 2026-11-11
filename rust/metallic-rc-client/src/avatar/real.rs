//! The REAL Rec Room avatar from the owned game bundle, replacing the procedural "Pip" parts when the game files are
//! present. The real body mesh (plus its nose) is split by region into skin / shirt / pants so each avatar wears its
//! look's colours, and the real Rec Room eye and mouth sprites are placed on the face as decals. Per-joint skinning and
//! real store outfits come next; until then the body is static in its bind pose but still moves with its root.
use std::path::Path;
use std::sync::Arc;

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    image::Image,
    mesh::{
        skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
        Indices, PrimitiveTopology,
    },
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use unity_assets::{Bundle, Mesh as UMesh, SerializedFile, Texture2D};

use super::{anim, skin, Drive, Rig};
use crate::room;

/// The avatar skin bundle inside the install, and the body + nose mesh objects in it.
const AVATAR_BUNDLE: &str = "edf7f2d64a5698a6cec7ec8a1f821ab8.bundle";
const BODY_MESH_PATHID: i64 = 8332635914843264857; // BodyMesh_LOD0
const NOSE_MESH_PATHID: i64 = 62350462907586393; // Nose_01Base_LOD0
/// The real Rec Room face sprite sheets: a default open eye and a neutral mouth (one bundle + object each).
const EYE_BUNDLE: &str = "84554899b3243253ba56b4d0262997b6.bundle";
const EYE_PATHID: i64 = -6280590809355750825; // Eye_Default
const MOUTH_BUNDLE: &str = "8fcb685a7a49de356df5b83ada6b0c86.bundle";
const MOUTH_PATHID: i64 = -8488847646716117003; // Mouth_Blank (neutral)

/// The loaded real avatar meshes, shared by every avatar. Absent when the game files are not installed.
#[derive(Resource)]
pub struct Roomie {
    /// the body split by region, so each avatar colours them from its look
    pub skin: Handle<Mesh>,
    pub shirt: Handle<Mesh>,
    pub pants: Handle<Mesh>,
    /// face sprite decals (an eye, mirrored for both sides, and a mouth), facing -z
    pub eye_quad: Handle<Mesh>,
    pub eye_mat: Handle<StandardMaterial>,
    pub mouth_quad: Handle<Mesh>,
    pub mouth_mat: Handle<StandardMaterial>,
    /// the real joint rig and its inverse bind poses, when the body is skinned (else the body is the static bind pose)
    pub rig: Option<Arc<skin::RigDef>>,
    pub inverse_bindposes: Option<Handle<SkinnedMeshInverseBindposes>>,
}

/// The real avatar's spawned joint entities (rig order) and the standing skeleton node, kept on the avatar root so the
/// animation can pose the joints and bob/lower the whole body.
#[derive(Component)]
pub struct Skeleton {
    pub joints: Vec<Entity>,
    /// the node carrying the stand-up; its vertical offset bobs/lowers the body
    pub root_node: Entity,
    /// eased finger curls per hand (outer three fingers, index, thumb) so grips don't snap
    pub curls: [[f32; 3]; 2],
}

/// Marks a `Rig` whose real body has been attached, so it is attached once.
#[derive(Component)]
pub struct RealBuilt;

/// The node that all of one avatar's real parts hang from, so a gentle idle (breathe + sway) can move them together
/// until real per-joint skinning lands. `phase` offsets each avatar so they don't breathe in lockstep.
#[derive(Component)]
pub struct Visual {
    phase: f32,
}

/// Where the face sits in the avatar's upright local space (feet at 0, facing -z): each eye, and the mouth.
const EYE_POS: [Vec3; 2] = [Vec3::new(-0.045, 1.528, -0.14), Vec3::new(0.045, 1.528, -0.14)];
const MOUTH_POS: Vec3 = Vec3::new(0.0, 1.478, -0.158);
const EYE_W: f32 = 0.13;
const MOUTH_W: f32 = 0.12;

/// A mesh object read out of the bundle as Bevy-space geometry (Unity is left-handed: z negated, winding flipped, uv v
/// flipped — same convention as `room.rs`).
struct Raw {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

fn read_umesh(b: &Bundle, sf: &SerializedFile, pathid: i64) -> Option<Raw> {
    let o = sf.object(pathid)?;
    let v = sf.read_value(o).ok()?;
    let m = UMesh::from_value(&v, b).ok()?;
    let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
    let pos: Vec<[f32; 3]> = m.positions().into_iter().map(flip).collect();
    if pos.is_empty() {
        return None;
    }
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
    Some(Raw { pos, nrm, uv, idx })
}

/// Which clothing zone a body triangle belongs to, from its centre in the avatar's own space (height along -z after our
/// flip, so `h = -z`; `x` is the left-right span). Head and the forearms/hands stay skin; the legs are pants; the rest
/// (torso and upper arms) is the shirt.
fn zone(p: [f32; 3]) -> usize {
    let (x, h) = (p[0].abs(), -p[2]);
    if h > 1.40 || x > 0.33 {
        0 // skin: head, forearms, hands
    } else if h < 0.86 && x < 0.3 {
        2 // pants: legs and feet
    } else {
        1 // shirt: torso and upper arms
    }
}

/// Loads the real body (+ nose) and splits it into skin / shirt / pants meshes sharing one vertex buffer.
fn load_bodies(dir: &Path) -> Option<[Mesh; 3]> {
    let path = room::bundle_dir(dir).join(AVATAR_BUNDLE);
    let b = Bundle::open(&path).ok()?;
    let files = b.serialized_files().ok()?;
    let sf = files.iter().find_map(|(_, sf)| sf.object(BODY_MESH_PATHID).map(|_| sf))?;
    let mut raw = read_umesh(&b, sf, BODY_MESH_PATHID)?;
    // the nose is omitted by default (Rec Room's face is a flat screen; the low-poly nose reads as uncanny). Set
    // AVATAR_NOSE=1 to include it. (Kept in step with `skin::load`.)
    if std::env::var("AVATAR_NOSE").is_ok() {
        if let Some(nose) = read_umesh(&b, sf, NOSE_MESH_PATHID) {
            let base = raw.pos.len() as u32;
            raw.pos.extend_from_slice(&nose.pos);
            raw.nrm.extend_from_slice(&nose.nrm);
            raw.uv.extend_from_slice(&nose.uv);
            raw.idx.extend(nose.idx.iter().map(|i| i + base));
        }
    }
    // split the triangles into the three zones by their centre
    let mut zones: [Vec<u32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for tri in raw.idx.chunks_exact(3) {
        let c = [0, 1, 2].map(|k| raw.pos[tri[k] as usize]);
        let mid = [0, 1, 2].map(|a| (c[0][a] + c[1][a] + c[2][a]) / 3.0);
        zones[zone(mid)].extend_from_slice(tri);
    }
    let make = |idx: Vec<u32>| {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, raw.pos.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, raw.nrm.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, raw.uv.clone());
        mesh.insert_indices(Indices::U32(idx));
        mesh
    };
    Some([make(std::mem::take(&mut zones[0])), make(std::mem::take(&mut zones[1])), make(std::mem::take(&mut zones[2]))])
}

/// Reads a Rec Room face sprite and turns it into a black decal with alpha from the drawing's darkness, cropped to the
/// shape. Robust to whether the source marks transparency in its alpha channel or paints the shape dark on white.
fn load_face_sprite(dir: &Path, bundle: &str, pathid: i64) -> Option<(Image, f32)> {
    let path = room::bundle_dir(dir).join(bundle);
    let b = Bundle::open(&path).ok()?;
    let files = b.serialized_files().ok()?;
    let sf = files.iter().find_map(|(_, sf)| sf.object(pathid).map(|_| sf))?;
    let o = sf.object(pathid)?;
    let v = sf.read_value(o).ok()?;
    let t = Texture2D::from_value(&v, &b).ok()?;
    let px = t.decode_rgba8().ok()?;
    let (w, h) = (t.width as usize, t.height as usize);
    if px.len() < w * h * 4 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    let mut alpha = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let lum = (px[i] as u32 + px[i + 1] as u32 + px[i + 2] as u32) / 3;
            let a = (255 - lum.min(255)) as u8;
            alpha[y * w + x] = a;
            if a > 24 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 || y1 < y0 {
        return None;
    }
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = vec![0u8; cw * ch * 4];
    for y in 0..ch {
        for x in 0..cw {
            out[(y * cw + x) * 4 + 3] = alpha[(y0 + y) * w + (x0 + x)];
        }
    }
    let img = Image::new(
        Extent3d { width: cw as u32, height: ch as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        out,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    Some((img, ch as f32 / cw as f32))
}

fn decal(materials: &mut Assets<StandardMaterial>, tex: Handle<Image>) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color_texture: Some(tex),
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        unlit: true,
        ..default()
    })
}

/// Startup: find the install and load the real body, nose and face sprites; absent if the game files are not there.
pub fn load_roomie(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut binds: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let Some(dir) = room::GAME_DIRS.iter().map(Path::new).find(|p| p.exists()) else {
        return;
    };
    // the skinned body (per-joint rig) is preferred; if its skin data can't be read, fall back to the static bind pose
    let (bodies, rig, inverse_bindposes) = match skin::load(dir) {
        Some(l) => {
            info!("avatar: real Rec Room body loaded skinned ({} joints)", l.rig.joints.len());
            (l.meshes, Some(Arc::new(l.rig)), Some(binds.add(SkinnedMeshInverseBindposes::from(l.inverse_bindposes))))
        }
        None => match load_bodies(dir) {
            Some(m) => {
                info!("avatar: real Rec Room body loaded static (no skin data)");
                (m, None, None)
            }
            None => {
                info!("avatar: no real body in the game files, using the procedural avatar");
                return;
            }
        },
    };
    let [skin, shirt, pants] = bodies;
    let (Some((eye_img, eye_a)), Some((mouth_img, mouth_a))) =
        (load_face_sprite(dir, EYE_BUNDLE, EYE_PATHID), load_face_sprite(dir, MOUTH_BUNDLE, MOUTH_PATHID))
    else {
        info!("avatar: real body loaded, but the face sprites could not be read");
        return;
    };
    commands.insert_resource(Roomie {
        skin: meshes.add(skin),
        shirt: meshes.add(shirt),
        pants: meshes.add(pants),
        eye_quad: meshes.add(Rectangle::new(EYE_W, EYE_W * eye_a)),
        eye_mat: decal(&mut materials, images.add(eye_img)),
        mouth_quad: meshes.add(Rectangle::new(MOUTH_W, MOUTH_W * mouth_a)),
        mouth_mat: decal(&mut materials, images.add(mouth_img)),
        rig,
        inverse_bindposes,
    });
}

/// The feet-at-origin, facing -z stand-up for the body mesh (authored lying along -z after our z-flip).
fn stand() -> Quat {
    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
}

fn solid(materials: &mut Assets<StandardMaterial>, c: [u8; 3], roughness: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: Color::srgb_u8(c[0], c[1], c[2]), perceptual_roughness: roughness, ..default() })
}

/// Attaches the real body (skin/shirt/pants) and face under every `Rig` once. First-person rigs get no body yet.
/// With a skinned rig the body deforms per joint (the joint hierarchy is spawned under the avatar and the meshes get a
/// `SkinnedMesh`); otherwise it is the static bind pose. Either way the face decals ride the head.
pub fn attach_real(
    mut commands: Commands,
    roomie: Option<Res<Roomie>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    rigs: Query<(Entity, &Rig), Without<RealBuilt>>,
) {
    let Some(roomie) = roomie else { return };
    for (root, rig) in &rigs {
        commands.entity(root).insert(RealBuilt);
        if rig.first_person {
            continue;
        }
        // everything hangs from one visual node so the idle can move the whole avatar together
        let visual = commands
            .spawn((Transform::default(), Visibility::default(), Visual { phase: (rig.seed as f32) * 1.3 }, ChildOf(root)))
            .id();

        let look = &rig.look;
        let mats = [
            solid(&mut materials, look.skin, 0.55),
            solid(&mut materials, look.shirt, 0.8),
            solid(&mut materials, look.pants, 0.75),
        ];
        let regions = [roomie.skin.clone(), roomie.shirt.clone(), roomie.pants.clone()];

        // where the face decals hang: under the head joint when skinned (so they follow the head), else the visual node
        let face = Quat::from_rotation_y(std::f32::consts::PI);
        let decal_parent = match (&roomie.rig, &roomie.inverse_bindposes) {
            (Some(rigdef), Some(binds)) => {
                // the standing skeleton: a node with the stand-up, the joints under it, the meshes skinned to them
                let skeleton_node = commands.spawn((Transform::from_rotation(stand()), Visibility::default(), ChildOf(visual))).id();
                let joints = skin::spawn_skeleton(&mut commands, rigdef, skeleton_node);
                for (mesh, mat) in regions.into_iter().zip(mats) {
                    // skinned geometry is placed by the joints, not by this entity's transform, so its default AABB is
                    // in the wrong place and frustum culling would wrongly hide it up close; skip culling for it.
                    commands.spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(mat),
                        Transform::default(),
                        SkinnedMesh { inverse_bindposes: binds.clone(), joints: joints.clone() },
                        NoFrustumCulling,
                        ChildOf(visual),
                    ));
                }
                // a group under the head joint whose transform cancels the head's bind pose, so decals placed in the
                // standing avatar frame (EYE_POS / MOUTH_POS) land on the face and then follow the head joint.
                let head = rigdef.head;
                let cancel = (Mat4::from_quat(stand()) * rigdef.bind_world[head]).inverse();
                let group = commands.spawn((Transform::from_matrix(cancel), Visibility::default(), ChildOf(joints[head]))).id();
                commands.entity(root).insert(Skeleton { joints, root_node: skeleton_node, curls: [anim::Grip::Open.curls(); 2] });
                group
            }
            _ => {
                let stand = Transform::from_rotation(stand());
                for (mesh, mat) in regions.into_iter().zip(mats) {
                    commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), stand, ChildOf(visual)));
                }
                visual
            }
        };

        // the real face sprites as decals, facing -z; the right eye is the same sprite mirrored in x.
        for (i, p) in EYE_POS.into_iter().enumerate() {
            let scale = Vec3::new(if i == 0 { 1.0 } else { -1.0 }, 1.0, 1.0);
            commands.spawn((Mesh3d(roomie.eye_quad.clone()), MeshMaterial3d(roomie.eye_mat.clone()),
                Transform { translation: p, rotation: face, scale }, ChildOf(decal_parent)));
        }
        commands.spawn((Mesh3d(roomie.mouth_quad.clone()), MeshMaterial3d(roomie.mouth_mat.clone()),
            Transform { translation: MOUTH_POS, rotation: face, ..default() }, ChildOf(decal_parent)));
    }
}

/// A gentle idle for the *static* (un-skinned) body: it breathes and sways so it is never dead-still. Skinned avatars
/// are posed per joint (`animate_skeleton`) and skip this.
pub fn idle(time: Res<Time>, roomie: Option<Res<Roomie>>, mut visuals: Query<(&Visual, &mut Transform)>) {
    if roomie.as_ref().and_then(|r| r.rig.as_ref()).is_some() {
        return; // the real rig is posed per joint; don't also bob the whole visual
    }
    let t = time.elapsed_secs();
    for (v, mut tr) in &mut visuals {
        let b = t * 1.6 + v.phase;
        tr.translation.y = (b.sin() * 0.006) - 0.006;
        let s = 1.0 + b.sin() * 0.004;
        tr.scale = Vec3::new(1.0, s, 1.0);
        tr.rotation = Quat::from_rotation_z((t * 0.8 + v.phase).sin() * 0.012);
    }
}

// Pose angles (radians), in the rig's own skeleton space: the body-up axis is −z (feet→head), down is +z, forward
// (the way it faces) is −y, right is +x. Swings are expressed about those axes and conjugated into each joint's local
// frame by `skel_swing`, so no per-joint axis-guessing is needed. Tuned against `avatar_preview`.
const ARM_DOWN: f32 = 0.62; // swing the A-pose arms down to the sides, about the body-up axis
const ELBOW_REST: f32 = 0.18; // a slight resting elbow bend so the arms aren't ramrod straight
const ARM_SWING: f32 = 0.5; // fore/aft arm swing while walking (opposite the legs)
const KNEE_BEND: f32 = 1.15; // knee flex on the recovery (back) part of the stride
const SPINE_LEAN: f32 = 0.45; // share of the torso lean the spine takes (the rest leans the hips)
const CROUCH_KNEE: f32 = 0.9; // extra knee/hip flex when crouched

/// The skeleton-space swing `r` (about the axes above), conjugated into joint `i`'s local frame and composed onto its
/// bind local rotation: `local' = (Wₚ⁻¹·r·Wₚ)·L`. `Wₚ` is the parent's bind-pose world rotation, so the bone swings by
/// `r` in skeleton space about its own pivot (exact when the parent is at bind, a good approximation when it isn't).
fn skel_swing(rig: &skin::RigDef, i: usize, r: Quat) -> Quat {
    let wp = match rig.joints[i].parent {
        Some(p) => rig.bind_world[p].to_scale_rotation_translation().1,
        None => Quat::IDENTITY,
    };
    (wp.inverse() * r * wp) * rig.joints[i].local.rotation
}

/// Drive the real 111-joint rig from the locomotion model: a natural rest (arms down, slight bend), a walk/run stride on
/// the hips/knees with the arms swinging against the legs, a forward lean, crouch, and a head that turns to the look.
/// Resets every joint to its bind pose first, then applies the swings; the body bobs/lowers via the skeleton node.
pub fn animate_skeleton(
    time: Res<Time>,
    roomie: Option<Res<Roomie>>,
    mut rigs: Query<(&mut Rig, &Drive, &Skeleton, &Transform)>,
    mut xf: Query<&mut Transform, Without<Rig>>,
) {
    const STAND_EYE: f32 = 1.55;
    const CROUCH_EYE: f32 = 1.55 * 0.725;
    let Some(roomie) = roomie else { return };
    let Some(rig) = &roomie.rig else { return };
    let dt = time.delta_secs().min(0.1);
    let (x_axis, y_axis, z_axis) = (Vec3::X, Vec3::Y, Vec3::Z);
    let rx = |a: f32| Quat::from_axis_angle(x_axis, a);
    let ry = |a: f32| Quat::from_axis_angle(y_axis, a);
    let rz = |a: f32| Quat::from_axis_angle(z_axis, a);

    for (mut r, drive, skel, root) in &mut rigs {
        let crouch = drive.head_height.map_or(0.0, |h| ((STAND_EYE - h) / (STAND_EYE - CROUCH_EYE)).clamp(0.0, 1.0));
        r.anim.step(dt, anim::Input { velocity: drive.velocity, grounded: drive.grounded, flying: drive.flying, talking: drive.talking, voice: 0.0, crouch });
        let pose = r.anim.pose();
        let phase = r.anim.phase;
        let swing = phase.sin() * (r.anim.speed / 3.0).min(1.0); // stride amplitude, 0 standing still

        // reset all joints to their bind pose; posed joints are overwritten below
        for (i, j) in rig.joints.iter().enumerate() {
            if let Ok(mut t) = xf.get_mut(skel.joints[i]) {
                *t = j.local;
            }
        }
        let mut set = |name: &str, rot: Quat| {
            if let Some(i) = rig.index(name) {
                if let Ok(mut t) = xf.get_mut(skel.joints[i]) {
                    t.rotation = skel_swing(rig, i, rot);
                }
            }
        };

        // --- torso lean (forward when moving/crouched): tilt the spine toward −y ---
        let lean = pose.lean;
        set("Jnt.Spine.Mid", rx(-lean * SPINE_LEAN));
        set("Jnt.Spine.Chest", rx(-lean * SPINE_LEAN * 0.6));

        // --- arms: down to the sides (about body-up), swinging fore/aft (about the left-right axis) opposite the legs ---
        let arm_l = swing * ARM_SWING; // left arm forward as the left leg goes back
        let arm_r = -swing * ARM_SWING;
        set("Jnt.UpperArm.L", rx(arm_l) * ry(ARM_DOWN));
        set("Jnt.UpperArm.R", rx(arm_r) * ry(-ARM_DOWN));
        set("Jnt.LowerArm.L", rx(ELBOW_REST + 0.25 * crouch));
        set("Jnt.LowerArm.R", rx(ELBOW_REST + 0.25 * crouch));

        // --- legs: swing the thighs fore/aft; bend the knee on the recovery; add a crouch squat ---
        let leg = [pose.legs[0], pose.legs[1]];
        for (k, side) in ["L", "R"].iter().enumerate() {
            let thigh = leg[k] - 0.5 * crouch * CROUCH_KNEE; // crouch flexes the hips forward
            let knee = KNEE_BEND * (-leg[k]).max(0.0) + CROUCH_KNEE * crouch;
            set(&format!("Jnt.UpperLeg.{side}"), rx(thigh));
            set(&format!("Jnt.LowerLeg.{side}"), rx(knee));
            set(&format!("Jnt.Foot.{side}"), rx(-0.4 * knee + 0.5 * leg[k].max(0.0)));
        }

        // --- head: turn to the synced look (relative to the avatar's facing); counter the spine lean a little ---
        let (yaw, pitch) = drive.head.map_or((0.0, 0.0), |h| {
            let (y, x, _) = (root.rotation.inverse() * h).to_euler(EulerRot::YXZ);
            (y.clamp(-1.1, 1.1), x.clamp(-0.7, 0.7))
        });
        set("Jnt.Neck", rz(yaw * 0.4) * rx(pitch * 0.4 + lean * SPINE_LEAN));
        set("Jnt.Head", rz(yaw * 0.6) * rx(pitch * 0.6));

        // the whole body bobs with the stride and lowers when crouching, via the standing skeleton node
        if let Ok(mut t) = xf.get_mut(skel.root_node) {
            let mut tr = Transform::from_rotation(stand());
            tr.translation.y = pose.bob - pose.drop;
            *t = tr;
        }
    }
}
