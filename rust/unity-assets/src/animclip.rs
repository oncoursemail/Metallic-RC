//! Rec Room 2026 (Unity 6) generic AnimationClip (class 74) decode + sampling.
//!
//! The built game's `AnimationClip`s are Mecanim **generic** clips: the legacy transform-curve
//! vectors are empty and the real data lives in `m_MuscleClip` (a `ClipMuscleConstant`) as a
//! `Clip` = `StreamedClip` (cubic-Hermite keyframes) + `DenseClip` (per-frame samples) +
//! `ConstantClip` (constants), with `m_ClipBindingConstant.genericBindings` mapping curve ranges to
//! skeleton bones (`path` = CRC32 of the bone's transform path; `attribute` = what is animated).
//!
//! The corpus `AnimationClip`s are stored WITHOUT a type tree, but an identical class in any
//! Addressables bundle carries one (`bundles ship type trees`). So decode a clip by reading its
//! bytes with a bundle-sourced `AnimationClip` [`TypeTree`] via [`crate::value::read`], then
//! [`AnimationClip::from_value`]. [`AnimationClip::sample`] evaluates every curve at a time, and
//! [`AnimationClip::bindings`] says which bone/attribute each curve range drives.

use crate::value::Value;
use crate::{Error, Result};

/// What a binding animates, with how many consecutive curves it occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    /// local position (x,y,z) — 3 curves
    Position,
    /// local rotation quaternion (x,y,z,w) — 4 curves
    Rotation,
    /// local euler angles (x,y,z) — 3 curves
    EulerRotation,
    /// local scale (x,y,z) — 3 curves
    Scale,
    /// a single scalar (muscle / blendshape / float property) — 1 curve
    Float(u32),
}

impl Attribute {
    fn from_code(a: u32) -> Self {
        match a {
            1 => Attribute::Position,
            2 => Attribute::Rotation,
            3 => Attribute::EulerRotation,
            4 => Attribute::Scale,
            other => Attribute::Float(other),
        }
    }
    /// Number of consecutive curves this attribute occupies.
    pub fn curve_count(self) -> usize {
        match self {
            Attribute::Rotation => 4,
            Attribute::Position | Attribute::EulerRotation | Attribute::Scale => 3,
            Attribute::Float(_) => 1,
        }
    }
}

/// One generic binding: a skeleton bone (by hashed path) and what is animated on it. Its values are
/// curves `[curve_start .. curve_start + attribute.curve_count())` of the clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Binding {
    /// CRC32 of the bone's transform path (resolve against the skeleton to get the bone).
    pub path: u32,
    pub attribute: Attribute,
    /// index of this binding's first curve in the clip's global curve space.
    pub curve_start: usize,
}

/// A cubic-Hermite key of one streamed curve: value over `[time, next key's time)` is
/// `((coeff[0]*dt + coeff[1])*dt + coeff[2])*dt + coeff[3]`, `dt = t - time`.
#[derive(Debug, Clone, Copy)]
struct StreamKey {
    time: f32,
    coeff: [f32; 4],
}

/// A decoded generic AnimationClip: curve data split across streamed/dense/constant, plus the
/// bone bindings. Global curve index space is `[0, streamed) ++ [streamed, +dense) ++ [.., +constant)`.
#[derive(Debug, Clone, Default)]
pub struct AnimationClip {
    pub name: String,
    pub sample_rate: f32,
    pub stop_time: f32,
    /// per streamed curve index, its Hermite keys in time order
    streamed: Vec<Vec<StreamKey>>,
    dense_frame_count: usize,
    dense_curve_count: usize,
    dense_begin: f32,
    dense_rate: f32,
    /// frame-major: `dense_samples[frame * dense_curve_count + c]`
    dense_samples: Vec<f32>,
    constant: Vec<f32>,
    pub bindings: Vec<Binding>,
}

fn u32s(v: Option<&Value>) -> Vec<u32> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|e| e.as_u64().map(|u| u as u32)).collect()).unwrap_or_default()
}
fn f32s(v: Option<&Value>) -> Vec<f32> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|e| e.as_f32()).collect()).unwrap_or_default()
}

impl AnimationClip {
    /// Builds a clip from a `Value` produced by reading the object with an `AnimationClip` type tree
    /// (`value::read`). Returns an error if the value is not a generic muscle clip.
    pub fn from_value(v: &Value) -> Result<AnimationClip> {
        let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let sample_rate = v.get("m_SampleRate").and_then(|x| x.as_f32()).unwrap_or(30.0);
        let muscle = v.get("m_MuscleClip").ok_or_else(|| Error::Format("no m_MuscleClip (not a generic clip)".into()))?;
        let stop_time = muscle.get("m_StopTime").and_then(|x| x.as_f32()).unwrap_or(0.0);
        let clip = muscle.get("m_Clip").and_then(|c| c.get("data")).ok_or_else(|| Error::Format("no m_Clip.data".into()))?;

        // StreamedClip: flat Vec<u32> of frames: [time, keyCount, (index, c0,c1,c2,c3) * keyCount]...
        let sc = clip.get("m_StreamedClip");
        let stream_count = sc.and_then(|s| s.get("curveCount")).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let data = u32s(sc.and_then(|s| s.get("data")));
        let mut streamed: Vec<Vec<StreamKey>> = vec![Vec::new(); stream_count];
        let mut i = 0usize;
        while i + 2 <= data.len() {
            let time = f32::from_bits(data[i]);
            let keys = data[i + 1] as usize;
            i += 2;
            for _ in 0..keys {
                if i + 5 > data.len() { break; }
                let idx = data[i] as usize;
                let coeff = [f32::from_bits(data[i + 1]), f32::from_bits(data[i + 2]), f32::from_bits(data[i + 3]), f32::from_bits(data[i + 4])];
                i += 5;
                if idx < stream_count {
                    streamed[idx].push(StreamKey { time, coeff });
                }
            }
        }

        // DenseClip
        let dc = clip.get("m_DenseClip");
        let dense_frame_count = dc.and_then(|d| d.get("m_FrameCount")).and_then(|x| x.as_i64()).unwrap_or(0).max(0) as usize;
        let dense_curve_count = dc.and_then(|d| d.get("m_CurveCount")).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let dense_begin = dc.and_then(|d| d.get("m_BeginTime")).and_then(|x| x.as_f32()).unwrap_or(0.0);
        let dense_rate = dc.and_then(|d| d.get("m_SampleRate")).and_then(|x| x.as_f32()).unwrap_or(sample_rate);
        let dense_samples = f32s(dc.and_then(|d| d.get("m_SampleArray")));

        // ConstantClip
        let constant = f32s(clip.get("m_ConstantClip").and_then(|c| c.get("data")));

        // Bindings -> curve ranges (each attribute occupies N consecutive curves, in binding order)
        let mut bindings = Vec::new();
        let mut curve = 0usize;
        if let Some(arr) = v.get("m_ClipBindingConstant").and_then(|b| b.get("genericBindings")).and_then(|x| x.as_array()) {
            for gb in arr {
                let path = gb.get("path").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let attribute = Attribute::from_code(gb.get("attribute").and_then(|x| x.as_u64()).unwrap_or(0) as u32);
                bindings.push(Binding { path, attribute, curve_start: curve });
                curve += attribute.curve_count();
            }
        }

        Ok(AnimationClip {
            name, sample_rate, stop_time, streamed, dense_frame_count, dense_curve_count,
            dense_begin, dense_rate, dense_samples, constant, bindings,
        })
    }

    /// Total number of curves (streamed + dense + constant).
    pub fn curve_count(&self) -> usize {
        self.streamed.len() + self.dense_curve_count + self.constant.len()
    }

    /// Sum of curves implied by the bindings; equals [`Self::curve_count`] for a well-formed clip.
    pub fn binding_curve_count(&self) -> usize {
        self.bindings.iter().map(|b| b.attribute.curve_count()).sum()
    }

    /// Evaluates one global curve index at `time` (seconds).
    pub fn sample_curve(&self, idx: usize, time: f32) -> f32 {
        let dense_base = self.streamed.len();
        let const_base = dense_base + self.dense_curve_count;
        if idx < dense_base {
            let keys = &self.streamed[idx];
            if keys.is_empty() { return 0.0; }
            // last key with time <= t
            let mut k = 0;
            while k + 1 < keys.len() && keys[k + 1].time <= time { k += 1; }
            let key = keys[k];
            let dt = (time - key.time).max(0.0);
            let c = key.coeff;
            ((c[0] * dt + c[1]) * dt + c[2]) * dt + c[3]
        } else if idx < const_base {
            let c = idx - dense_base;
            if self.dense_curve_count == 0 || self.dense_frame_count == 0 { return 0.0; }
            let ft = ((time - self.dense_begin) * self.dense_rate).clamp(0.0, (self.dense_frame_count - 1) as f32);
            let f0 = ft.floor() as usize;
            let f1 = (f0 + 1).min(self.dense_frame_count - 1);
            let frac = ft - f0 as f32;
            let s0 = self.dense_samples.get(f0 * self.dense_curve_count + c).copied().unwrap_or(0.0);
            let s1 = self.dense_samples.get(f1 * self.dense_curve_count + c).copied().unwrap_or(s0);
            s0 + (s1 - s0) * frac
        } else {
            self.constant.get(idx - const_base).copied().unwrap_or(0.0)
        }
    }

    /// Evaluates every curve at `time`, in global curve-index order.
    pub fn sample(&self, time: f32) -> Vec<f32> {
        (0..self.curve_count()).map(|i| self.sample_curve(i, time)).collect()
    }

    /// Reads `n` curves starting at a binding's `curve_start` at `time` (e.g. 4 for a quaternion).
    pub fn sample_binding(&self, b: &Binding, time: f32) -> Vec<f32> {
        (0..b.attribute.curve_count()).map(|k| self.sample_curve(b.curve_start + k, time)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bundle, Reader, SerializedFile};
    use std::path::Path;

    /// End-to-end: source an AnimationClip type tree from a bundle, decode real corpus clips with it,
    /// and check the generic-clip invariants (exact byte consumption, binding/curve counts match,
    /// rotation quaternions sample to ~unit length). Gated on `RR_ANIM_BUNDLE` (a .bundle holding an
    /// AnimationClip) and `RR_CORPUS_DIR` (the decrypted corpus with `resources.assets`); skipped when
    /// unset so CI stays data-free.
    #[test]
    fn decode_corpus_clips() {
        let (Ok(bundle), Ok(corpus)) = (std::env::var("RR_ANIM_BUNDLE"), std::env::var("RR_CORPUS_DIR")) else {
            eprintln!("decode_corpus_clips: skipped (set RR_ANIM_BUNDLE and RR_CORPUS_DIR)");
            return;
        };
        let b = Bundle::open(Path::new(&bundle)).expect("open bundle");
        let tree = b.serialized_files().expect("sfs").into_iter()
            .flat_map(|(_, sf)| sf.types)
            .find(|t| t.class_id == 74 && t.tree.is_some())
            .and_then(|t| t.tree)
            .expect("AnimationClip tree in bundle");
        let res = SerializedFile::parse(std::fs::read(Path::new(&corpus).join("resources.assets")).unwrap()).unwrap();
        let mut tested = 0;
        for o in res.objects.iter().filter(|o| o.class_id == 74) {
            let mut r = Reader::new(res.object_bytes(o), res.big_endian);
            let Ok(v) = crate::value::read(&tree, &res.ref_types, &mut r) else { continue };
            assert_eq!(r.pos(), r.len(), "clip did not consume all bytes");
            let clip = AnimationClip::from_value(&v).expect("from_value");
            // every generic clip: curves implied by bindings == actual curve count
            assert_eq!(clip.binding_curve_count(), clip.curve_count(), "{}: binding/curve mismatch", clip.name);
            // sample at t=0 shouldn't panic and rotation quaternions should be ~unit length
            for b in &clip.bindings {
                if b.attribute == Attribute::Rotation {
                    let q = clip.sample_binding(b, 0.0);
                    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                    assert!((0.5..1.5).contains(&len), "{}: quaternion not ~unit ({len})", clip.name);
                }
            }
            tested += 1;
            if tested >= 50 { break; }
        }
        assert!(tested > 0, "no clips tested");
        eprintln!("decode_corpus_clips: {tested} clips decoded + sampled OK");
    }
}
