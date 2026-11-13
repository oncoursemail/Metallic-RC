//! Real authored Rec Room animation played on the 111-joint rig. The game ships 795 Mecanim generic AnimationClips in
//! `resources.assets` (Avatar_Skeleton|Walk_InPlace_Forward, Run_Forward, Jump_InPlace_*, Idle_Standing01_Pose, ...). We
//! decode them with `unity_assets::AnimationClip` (Hermite/dense/constant curves), resolve each clip's bone bindings to
//! our joints via the Avatar's `m_TOS` (curve path-hash → bone path → `Jnt.*` → rig index), and in `real::animate_skeleton`
//! sample the current state's clip per frame, applying the authored LOCAL rotations onto the joints (in place) — so the
//! avatar moves with authentic Rec Room motion. The head-look and finger grips are layered on top; the hand-tuned
//! procedural poser stays the fallback for any state without a clip, or when the clips can't be loaded.
//!
//! `resources.assets` is RR-encrypted in a normal install (so loading silently falls back to procedural); point
//! `METALLIC_ANIM_CORPUS` at a decrypted `resources.assets` to get the real motion.
use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use unity_assets::{value, AnimationClip, Attribute, Binding, Bundle, Reader, SerializedFile, TypeTree};

/// Unity class id for Avatar (Mecanim avatar definition, carries `m_TOS`).
const AVATAR_CLASS: i32 = 90;

use super::skin::RigDef;
use crate::room;

/// A bundle that ships the AnimationClip (74) type tree (corpus clips carry none), and one that ships the Avatar (90) tree.
const CLIP_TREE_BUNDLE: &str = "0e46772fa7e018f83b8b8f20adcbbd96.bundle";
const AVATAR_TREE_BUNDLE: &str = "04e19ef28a99fce60ffaf4249bb21d48.bundle";

/// Which authored clip plays for each locomotion state. The *_InPlace_* clips keep the body stationary (we drive the
/// avatar's own position); Run has no in-place variant, so we take its rotations only (root translation is ignored).
pub const STATE_CLIPS: &[(&str, &str)] = &[
    ("idle", "Avatar_Skeleton|Idle_Standing01_Pose"),
    ("walk", "Avatar_Skeleton|Walk_Forward"),
    ("run", "Avatar_Skeleton|Run_Forward"),
    ("jump", "Avatar_Skeleton|Jump_Run_RisingLoop"),
    ("fall", "Avatar_Skeleton|Jump_Run_FallingLoop"),
    ("crouch_idle", "Avatar_Skeleton|Crouch_Idle"),
    ("crouch_walk", "Avatar_Skeleton|Crouch_Walk_Forward"),
    ("fly", "Avatar_Skeleton|Fly_Idle"),
    // emotes/poses, triggered via Drive.emote. Idle_Sit/Idle_Prone are forward-kinematic; the Fidget_* fidgets are
    // authored as VR hand-target poses (Vr.Hand.{L,R} positions) and play via the 2-bone arm IK in `pose`/`solve_arm`.
    ("sit", "Avatar_Skeleton|Idle_Sit"),
    ("prone", "Avatar_Skeleton|Idle_Prone"),
    ("hips", "Avatar_Skeleton|Fidget_HandsOnHips01_static"),
    ("flex", "Avatar_Skeleton|Fidget_Flex01_static"),
    ("crossed", "Avatar_Skeleton|Fidget_CrossedArms01_static"),
    ("cute", "Avatar_Skeleton|Fidget_Cute01"),
];

/// A clip's VR-IK hand targets: Rec Room's "fidget" emotes (flex, hands-on-hips, wave, …) are authored not as joint
/// rotations but as `Vr.Hand.L/R` effector POSITIONS (where the hands should be), with the arms solved by IK. We store
/// those two position bindings and solve a 2-bone arm IK each frame to reach them.
struct IkArms {
    /// the Vr.Hand.{L,R} position bindings (hand targets in the avatar-root space)
    hand: [Binding; 2],
}

/// One authored clip, pre-resolved to our rig: either forward-kinematic joint rotations (locomotion, Idle_Sit, …) or,
/// for the VR-authored fidget emotes, hand IK targets.
pub struct Clip {
    pub stop_time: f32,
    clip: AnimationClip,
    /// (joint index in the rig, quaternion rotation binding) for every bone the clip animates that is in our rig
    bones: Vec<(usize, Binding)>,
    /// hand IK targets, for the VR-authored emotes (then `bones` is usually empty)
    ik: Option<IkArms>,
    /// the root (Jnt.Spine.Root) position binding, so a pose's hip height can ground the body (e.g. sit/prone)
    root: Option<Binding>,
}

impl Clip {
    /// The authored local joint rotations at time `t` (seconds), in Bevy space, as (joint index, rotation): the clip's
    /// own rotations for an FK clip, or the 2-bone arm-IK solution for a VR hand-target (emote) clip.
    pub fn pose(&self, t: f32, rig: &RigDef) -> Vec<(usize, Quat)> {
        if let Some(ik) = &self.ik {
            let mut out = Vec::new();
            for (i, side) in ["L", "R"].into_iter().enumerate() {
                let v = self.clip.sample_binding(&ik.hand[i], t);
                if v.len() >= 3 {
                    // the hand target in the avatar root frame, converted Unity→Bevy (z-flip), then solve the arm to it
                    out.extend(solve_arm(rig, side, Vec3::new(v[0], v[1], -v[2])));
                }
            }
            out
        } else {
            self.bones.iter().filter_map(|&(j, b)| {
                let v = self.clip.sample_binding(&b, t);
                (v.len() == 4).then(|| (j, conj_rot(Quat::from_xyzw(v[0], v[1], v[2], v[3]))))
            }).collect()
        }
    }

    /// The world-space vertical offset of the body at time `t` from the clip's root (hip) height versus the bind pose —
    /// so a pose that sits/lies down rests on the floor instead of floating. `stand` is the feet-up stand-up rotation.
    pub fn root_y(&self, t: f32, rig: &RigDef, stand: Quat) -> Option<f32> {
        let b = self.root?;
        let v = self.clip.sample_binding(&b, t);
        let ri = rig.index("Jnt.Spine.Root")?;
        (v.len() >= 3).then(|| {
            let clip_root = Vec3::new(v[0], v[1], -v[2]);
            let bind_root = rig.bind_world[ri].to_scale_rotation_translation().2;
            (stand * (clip_root - bind_root)).y
        })
    }
}

/// Analytic 2-bone IK for one arm: rotate Jnt.UpperArm.{side} and Jnt.LowerArm.{side} so the hand reaches `target`
/// (in the skeleton's root space, Bevy coords), elbow biased by a fixed pole hint. Returns the two joints' new LOCAL
/// rotations (empty if the arm joints aren't in the rig). Bone positions come from the bind pose.
fn solve_arm(rig: &RigDef, side: &str, target: Vec3) -> Vec<(usize, Quat)> {
    let (Some(up), Some(lo), Some(hand)) =
        (rig.index(&format!("Jnt.UpperArm.{side}")), rig.index(&format!("Jnt.LowerArm.{side}")), rig.index(&format!("Jnt.Hand.{side}")))
    else {
        return Vec::new();
    };
    let pos = |j: usize| rig.bind_world[j].to_scale_rotation_translation().2;
    let rot = |j: usize| rig.bind_world[j].to_scale_rotation_translation().1;
    let (sh, el, hd) = (pos(up), pos(lo), pos(hand));
    let (l1, l2) = (sh.distance(el), el.distance(hd));
    let to_t = target - sh;
    let dist = to_t.length();
    if dist < 1e-4 || l1 < 1e-4 || l2 < 1e-4 {
        return Vec::new();
    }
    let d = dist.clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
    let dir = to_t / dist;
    // shoulder elevation off the shoulder→target line so the two-bone triangle closes (law of cosines)
    let a_sh = (((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0)).acos();
    // pole hint: bend the elbow toward "back + down" in skeleton space (forward is −y, down is +z)
    let pole = Vec3::new(0.0, 1.0, 0.6).normalize();
    let mut n = dir.cross(pole);
    n = if n.length() < 1e-4 { dir.cross(Vec3::X).normalize() } else { n.normalize() };
    let upper_dir = Quat::from_axis_angle(n, a_sh) * dir;
    // world-space rotations that aim the upper arm at the solved direction, then the forearm at the hand
    let r_up = Quat::from_rotation_arc((el - sh).normalize(), upper_dir);
    let elbow = sh + upper_dir * l1;
    let hand_dir = (target - elbow).normalize();
    let r_lo = Quat::from_rotation_arc(r_up * (hd - el).normalize(), hand_dir) * r_up;
    // convert the world rotations to LOCAL (parent of the upper arm is near bind; the forearm's parent is the upper arm)
    let pw = rig.joints[up].parent.map_or(Quat::IDENTITY, rot);
    let new_up = r_up * rot(up);
    let new_lo = r_lo * rot(lo);
    vec![(up, pw.inverse() * new_up), (lo, new_up.inverse() * new_lo)]
}

/// Every loaded state clip, shared by all avatars. Absent when the corpus can't be read (→ procedural fallback).
#[derive(Resource)]
pub struct Clips {
    by_state: HashMap<&'static str, Clip>,
}

impl Clips {
    pub fn get(&self, state: &str) -> Option<&Clip> {
        self.by_state.get(state)
    }
}

/// Mirror a rotation across the z=0 plane (Unity's left-handed space → Bevy's), the rotation half of `skin::conj`'s
/// `M·A·M` (M = diag(1,1,-1)), so authored local rotations compose with the z-flipped bind rig.
fn conj_rot(q: Quat) -> Quat {
    let m = Mat3::from_cols(Vec3::X, Vec3::Y, Vec3::NEG_Z);
    Quat::from_mat3(&(m * Mat3::from_quat(q) * m))
}

/// First type tree for `class_id` across a bundle's serialized files.
fn tree_for(b: &Bundle, class_id: i32) -> Option<TypeTree> {
    b.serialized_files().ok()?.into_iter().flat_map(|(_, sf)| sf.types).find(|t| t.class_id == class_id && t.tree.is_some()).and_then(|t| t.tree)
}

/// The union bone table (curve path-hash → bone transform path) across every Avatar in the file: the player rig
/// contributes the `Jnt.*` joints, the VR control rig the `Vr.*` effector handles the fidget emotes target.
fn union_tos(res: &SerializedFile, avatar_tree: &TypeTree) -> HashMap<u32, String> {
    let mut union: HashMap<u32, String> = HashMap::new();
    for o in res.objects.iter().filter(|o| o.class_id == AVATAR_CLASS) {
        let Ok(av) = value::read(avatar_tree, &res.ref_types, &mut Reader::new(res.object_bytes(o), res.big_endian)) else { continue };
        if let Some(t) = av.get("m_TOS") {
            for (k, v) in t.pairs() {
                if let (Some(k), Some(v)) = (k.as_u64(), v.as_str()) {
                    union.entry(k as u32).or_insert_with(|| v.to_string());
                }
            }
        }
    }
    union
}

/// Loads the real locomotion clips and resolves them to `rig`. `None` (→ procedural) when the corpus is missing or
/// encrypted, or the type trees / bone table can't be read.
pub fn load(dir: &Path, rig: &RigDef) -> Option<Clips> {
    // Source the clips' resources.assets, in order of preference:
    //  1. METALLIC_ANIM_CORPUS — an explicit, already-decrypted resources.assets.
    //  2. RR_CIPHER_DIR — the local keystream pad (ks.bin, gitignored): decrypt the install's OWN encrypted
    //     resources.assets on the fly, so the real clips load from the user's install with no pre-decrypted corpus.
    //  3. the install's raw resources.assets — RR-encrypted in a normal install, so this won't parse and we fall back.
    let data = dir.join("RecRoom_Data");
    let bytes = if let Ok(corpus) = std::env::var("METALLIC_ANIM_CORPUS") {
        std::fs::read(&corpus).ok()?
    } else if let Ok(cipher_dir) = std::env::var("RR_CIPHER_DIR") {
        match unity_assets::decrypt::Cipher::load(std::path::Path::new(&cipher_dir)) {
            Ok(c) => c.decrypt_install_file(&data, "resources.assets").ok()?,
            Err(e) => {
                info!("avatar: RR_CIPHER_DIR set but the cipher didn't load ({e}); using the procedural poser");
                return None;
            }
        }
    } else {
        std::fs::read(data.join("resources.assets")).ok()?
    };
    let res = match SerializedFile::parse(bytes) {
        Ok(r) => r,
        Err(e) => {
            info!("avatar: no authored clips (resources.assets not readable/decrypted: {e}); using the procedural poser");
            return None;
        }
    };
    let bundles = room::bundle_dir(dir);
    let clip_tree = tree_for(&Bundle::open(bundles.join(CLIP_TREE_BUNDLE)).ok()?, 74)?;
    let avatar_tree = Bundle::open(bundles.join(AVATAR_TREE_BUNDLE)).ok().and_then(|b| tree_for(&b, 90))?;

    // the union bone table across all avatars (path-hash → bone path): the player rig has the Jnt.* joints; the VR
    // control rig (used by the fidget emotes) has the Vr.* effector handles.
    let tos = union_tos(&res, &avatar_tree);
    // path-hash → our joint index (by the "Jnt.*" leaf), and → VR handle leaf name (Vr.Hand.L, …) for the IK targets
    let hash_to_joint: HashMap<u32, usize> =
        tos.iter().filter_map(|(&h, path)| rig.index(path.rsplit('/').next().unwrap_or(path)).map(|j| (h, j))).collect();
    let vr_name: HashMap<u32, &str> =
        tos.iter().filter_map(|(&h, path)| path.rsplit('/').next().filter(|l| l.starts_with("Vr.")).map(|l| (h, l))).collect();
    if hash_to_joint.is_empty() {
        info!("avatar: authored-clip bone table didn't resolve to the rig; using the procedural poser");
        return None;
    }

    // index clip objects by name, then decode each state clip and resolve its bindings — to joint rotations (FK) and,
    // for the VR fidget emotes, to the Vr.Hand.{L,R} position targets (IK).
    let by_name: HashMap<String, &unity_assets::ObjectInfo> =
        res.objects.iter().filter(|o| o.class_id == 74).filter_map(|o| res.object_name(o).map(|n| (n, o))).collect();
    let mut by_state = HashMap::new();
    for &(state, clip_name) in STATE_CLIPS {
        let Some(&o) = by_name.get(clip_name) else { continue };
        let Ok(v) = value::read(&clip_tree, &res.ref_types, &mut Reader::new(res.object_bytes(o), res.big_endian)) else { continue };
        let Ok(clip) = AnimationClip::from_value(&v) else { continue };
        let bones: Vec<(usize, Binding)> = clip.bindings.iter()
            .filter(|b| b.attribute == Attribute::Rotation)
            .filter_map(|b| hash_to_joint.get(&b.path).map(|&j| (j, *b)))
            .collect();
        let hand_target = |want: &str| {
            clip.bindings.iter().find(|b| b.attribute == Attribute::Position && vr_name.get(&b.path) == Some(&want)).copied()
        };
        let ik = match (hand_target("Vr.Hand.L"), hand_target("Vr.Hand.R")) {
            (Some(l), Some(r)) => Some(IkArms { hand: [l, r] }),
            _ => None,
        };
        let root = clip.bindings.iter()
            .find(|b| b.attribute == Attribute::Position && hash_to_joint.get(&b.path).and_then(|&j| rig.joints.get(j)).map(|jt| jt.name.as_str()) == Some("Jnt.Spine.Root"))
            .copied();
        if !bones.is_empty() || ik.is_some() {
            by_state.insert(state, Clip { stop_time: clip.stop_time.max(0.01), clip, bones, ik, root });
        }
    }
    if by_state.is_empty() {
        return None;
    }
    info!("avatar: authored clips loaded ({} states: {:?})", by_state.len(), by_state.keys().collect::<Vec<_>>());
    Some(Clips { by_state })
}
