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
    // emotes/poses, triggered via Drive.emote. Only forward-kinematic clips work here: the Fidget_*_static "fidget"
    // emotes (flex, hands-on-hips, wave, dance) are authored as IK effector targets (Position bindings on hand/foot
    // handles, no joint rotations), so they'd need an IK solver to play — a follow-up. Idle_Sit/Idle_Prone are FK.
    ("sit", "Avatar_Skeleton|Idle_Sit"),
    ("prone", "Avatar_Skeleton|Idle_Prone"),
];

/// One authored clip, with its rotation bindings pre-resolved to our joint indices.
pub struct Clip {
    pub stop_time: f32,
    clip: AnimationClip,
    /// (joint index in the rig, quaternion rotation binding) for every bone the clip animates that is in our rig
    bones: Vec<(usize, Binding)>,
}

impl Clip {
    /// The authored local rotations at time `t` (seconds), already converted into Bevy space, as (joint index, rotation).
    pub fn pose(&self, t: f32) -> impl Iterator<Item = (usize, Quat)> + '_ {
        self.bones.iter().filter_map(move |&(j, b)| {
            let v = self.clip.sample_binding(&b, t);
            (v.len() == 4).then(|| (j, conj_rot(Quat::from_xyzw(v[0], v[1], v[2], v[3]))))
        })
    }
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

/// The player-body Avatar's bone table (curve path-hash → bone transform path), as the richest `m_TOS` among the file's
/// Avatars (the real rig has ~111 `Jnt.*` entries; props have a handful of non-Jnt bones).
fn player_tos(res: &SerializedFile, avatar_tree: &TypeTree) -> HashMap<u32, String> {
    let mut best: HashMap<u32, String> = HashMap::new();
    let mut best_jnt = 0;
    for o in res.objects.iter().filter(|o| o.class_id == AVATAR_CLASS) {
        let Ok(av) = value::read(avatar_tree, &res.ref_types, &mut Reader::new(res.object_bytes(o), res.big_endian)) else { continue };
        let map: HashMap<u32, String> = av.get("m_TOS").map(|t| t.pairs().iter().filter_map(|(k, v)| Some((k.as_u64()? as u32, v.as_str()?.to_string()))).collect()).unwrap_or_default();
        let jnt = map.values().filter(|p| p.contains("Jnt.")).count();
        if jnt > best_jnt {
            best_jnt = jnt;
            best = map;
            if best_jnt >= 100 {
                break; // the player rig; no need to scan the rest
            }
        }
    }
    best
}

/// Loads the real locomotion clips and resolves them to `rig`. `None` (→ procedural) when the corpus is missing or
/// encrypted, or the type trees / bone table can't be read.
pub fn load(dir: &Path, rig: &RigDef) -> Option<Clips> {
    let corpus = std::env::var("METALLIC_ANIM_CORPUS").map(std::path::PathBuf::from).unwrap_or_else(|_| dir.join(r"RecRoom_Data\resources.assets"));
    // the install's own resources.assets is RR-encrypted and won't parse — that's the normal fallback path, not an error
    let bytes = std::fs::read(&corpus).ok()?;
    let res = match SerializedFile::parse(bytes) {
        Ok(r) => r,
        Err(e) => {
            info!("avatar: no authored clips ({corpus:?} not a readable resources.assets: {e}); using the procedural poser");
            return None;
        }
    };
    let bundles = room::bundle_dir(dir);
    let clip_tree = tree_for(&Bundle::open(bundles.join(CLIP_TREE_BUNDLE)).ok()?, 74)?;
    let avatar_tree = Bundle::open(bundles.join(AVATAR_TREE_BUNDLE)).ok().and_then(|b| tree_for(&b, 90))?;

    // bone table → map each curve path-hash to our joint index (by the "Jnt.*" leaf of its bone path)
    let tos = player_tos(&res, &avatar_tree);
    let hash_to_joint: HashMap<u32, usize> =
        tos.iter().filter_map(|(&h, path)| rig.index(path.rsplit('/').next().unwrap_or(path)).map(|j| (h, j))).collect();
    if hash_to_joint.is_empty() {
        info!("avatar: authored-clip bone table didn't resolve to the rig; using the procedural poser");
        return None;
    }

    // index clip objects by name, then decode each state clip and resolve its rotation bindings to joints
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
        if !bones.is_empty() {
            by_state.insert(state, Clip { stop_time: clip.stop_time.max(0.01), clip, bones });
        }
    }
    if by_state.is_empty() {
        return None;
    }
    info!("avatar: authored clips loaded ({} states: {:?})", by_state.len(), by_state.keys().collect::<Vec<_>>());
    Some(Clips { by_state })
}
