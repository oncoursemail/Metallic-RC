//! Unity 6 ParticleSystem (class 198) + ParticleSystemRenderer (199) decode into a compact,
//! Bevy-mappable VFX spec.
//!
//! Room VFX live in Addressables prefab bundles, which carry type trees, so decode the component
//! with [`crate::SerializedFile::read_value`] and pass the [`Value`] to [`ParticleSystem::from_value`].
//! The requested modules are covered: main (lifetime/looping/max particles), start
//! size/color/lifetime/speed/rotation/gravity (InitialModule), emission (rate + bursts), shape,
//! velocity-over-lifetime, and color/size-over-lifetime. The renderer's material is read separately
//! from the ParticleSystemRenderer via [`renderer_materials`].
//!
//! Unity value types: a `MinMaxCurve` is a scalar / curve / random pair (see [`MinMaxCurve`]); a
//! `MinMaxGradient` is a constant / random / gradient colour (see [`MinMaxGradient`]).

use crate::value::{PPtr, Value};

/// One AnimationCurve keyframe (time, value, in/out tangents).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Keyframe {
    pub time: f32,
    pub value: f32,
    pub in_slope: f32,
    pub out_slope: f32,
}

fn curve(v: Option<&Value>) -> Vec<Keyframe> {
    v.and_then(|c| c.get("m_Curve")).and_then(Value::as_array).map(|a| {
        a.iter().map(|k| {
            let f = |n: &str| k.get(n).and_then(Value::as_f32).unwrap_or(0.0);
            Keyframe { time: f("time"), value: f("value"), in_slope: f("inSlope"), out_slope: f("outSlope") }
        }).collect()
    }).unwrap_or_default()
}

/// Unity `MinMaxCurve`: `mode` 0 = constant (`scalar`), 1 = curve (`max_curve` × `scalar`),
/// 2 = random between `min_curve` and `max_curve` (× scalar), 3 = random between two constants
/// (`min_scalar`..`scalar`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MinMaxCurve {
    pub mode: u16,
    pub scalar: f32,
    pub min_scalar: f32,
    pub max_curve: Vec<Keyframe>,
    pub min_curve: Vec<Keyframe>,
}

impl MinMaxCurve {
    pub fn from_value(v: &Value) -> MinMaxCurve {
        let f = |n: &str| v.get(n).and_then(Value::as_f32).unwrap_or(0.0);
        MinMaxCurve {
            mode: v.get("minMaxState").and_then(Value::as_u64).unwrap_or(0) as u16,
            scalar: f("scalar"),
            min_scalar: f("minScalar"),
            max_curve: curve(v.get("maxCurve")),
            min_curve: curve(v.get("minCurve")),
        }
    }
    /// A representative constant value: the scalar, or the midpoint for a random-between-constants.
    pub fn constant(&self) -> f32 {
        if self.mode == 3 { (self.min_scalar + self.scalar) * 0.5 } else { self.scalar }
    }
}

/// RGBA colour (linear, as stored).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

fn color(v: Option<&Value>) -> Color {
    let f = |n: &str| v.and_then(|c| c.get(n)).and_then(Value::as_f32).unwrap_or(0.0);
    Color { r: f("r"), g: f("g"), b: f("b"), a: f("a") }
}

/// Unity `MinMaxGradient`: `mode` 0 = constant (`max_color`), 1 = gradient (`stops`), 2 = random
/// between two colours (`min_color`..`max_color`), 3 = random between two gradients.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MinMaxGradient {
    pub mode: u16,
    pub min_color: Color,
    pub max_color: Color,
    /// gradient colour stops `(time 0..1, colour)` from the max gradient.
    pub stops: Vec<(f32, Color)>,
}

impl MinMaxGradient {
    pub fn from_value(v: &Value) -> MinMaxGradient {
        let mut stops = Vec::new();
        if let Some(g) = v.get("maxGradient") {
            let n = g.get("m_NumColorKeys").and_then(Value::as_u64).unwrap_or(0) as usize;
            for i in 0..n.min(8) {
                let t = g.get(&format!("ctime{i}")).and_then(Value::as_u64).unwrap_or(0) as f32 / 65535.0;
                stops.push((t, color(g.get(&format!("key{i}")))));
            }
        }
        MinMaxGradient {
            mode: v.get("minMaxState").and_then(Value::as_u64).unwrap_or(0) as u16,
            min_color: color(v.get("minColor")),
            max_color: color(v.get("maxColor")),
            stops,
        }
    }
}

/// An emission burst: `count` particles at `time`, repeated `cycle_count` times every
/// `repeat_interval` seconds, each with `probability`.
#[derive(Debug, Clone, Default)]
pub struct Burst {
    pub time: f32,
    pub count: MinMaxCurve,
    pub cycle_count: i32,
    pub repeat_interval: f32,
    pub probability: f32,
}

/// A decoded ParticleSystem (the modules a renderer needs to reproduce the effect).
#[derive(Debug, Clone, Default)]
pub struct ParticleSystem {
    pub length: f32,
    pub looping: bool,
    pub prewarm: bool,
    pub play_on_awake: bool,
    pub simulation_speed: f32,
    pub max_particles: i32,
    // start (InitialModule)
    pub start_lifetime: MinMaxCurve,
    pub start_speed: MinMaxCurve,
    pub start_size: MinMaxCurve,
    pub start_size_y: MinMaxCurve,
    pub start_size_z: MinMaxCurve,
    pub size_3d: bool,
    pub start_rotation: MinMaxCurve,
    pub start_color: MinMaxGradient,
    pub gravity_modifier: MinMaxCurve,
    // emission
    pub emission_enabled: bool,
    pub rate_over_time: MinMaxCurve,
    pub rate_over_distance: MinMaxCurve,
    pub bursts: Vec<Burst>,
    // shape
    pub shape_enabled: bool,
    /// Unity shape type (0 Sphere, 4 Cone, 5 Box, 6 Mesh, 7 ConeVolume, …).
    pub shape_type: i32,
    pub shape_angle: f32,
    pub shape_radius: f32,
    pub shape_mesh: PPtr,
    // velocity over lifetime (x, y, z)
    pub velocity_enabled: bool,
    pub velocity: [MinMaxCurve; 3],
    // over lifetime
    pub color_over_lifetime: Option<MinMaxGradient>,
    pub size_over_lifetime: Option<MinMaxCurve>,
}

impl ParticleSystem {
    /// Builds the spec from a ParticleSystem (198) `Value` (decoded with the component's type tree).
    pub fn from_value(v: &Value) -> ParticleSystem {
        let mmc = |p: Option<&Value>, n: &str| p.and_then(|x| x.get(n)).map(MinMaxCurve::from_value).unwrap_or_default();
        let boolf = |p: Option<&Value>, n: &str| p.and_then(|x| x.get(n)).and_then(Value::as_bool).unwrap_or(false);
        let init = v.get("InitialModule");
        let emit = v.get("EmissionModule");
        let shape = v.get("ShapeModule");
        let vel = v.get("VelocityModule");
        let col = v.get("ColorModule");
        let siz = v.get("SizeModule");

        let bursts = emit.and_then(|e| e.get("m_Bursts")).and_then(Value::as_array).map(|a| {
            a.iter().map(|bv| Burst {
                time: bv.get("time").and_then(Value::as_f32).unwrap_or(0.0),
                count: bv.get("countCurve").map(MinMaxCurve::from_value).unwrap_or_default(),
                cycle_count: bv.get("cycleCount").and_then(Value::as_i64).unwrap_or(0) as i32,
                repeat_interval: bv.get("repeatInterval").and_then(Value::as_f32).unwrap_or(0.0),
                probability: bv.get("probability").and_then(Value::as_f32).unwrap_or(1.0),
            }).collect()
        }).unwrap_or_default();

        ParticleSystem {
            length: v.get("lengthInSec").and_then(Value::as_f32).unwrap_or(0.0),
            looping: v.get("looping").and_then(Value::as_bool).unwrap_or(false),
            prewarm: v.get("prewarm").and_then(Value::as_bool).unwrap_or(false),
            play_on_awake: v.get("playOnAwake").and_then(Value::as_bool).unwrap_or(false),
            simulation_speed: v.get("simulationSpeed").and_then(Value::as_f32).unwrap_or(1.0),
            max_particles: init.and_then(|m| m.get("maxNumParticles")).and_then(Value::as_i64).unwrap_or(0) as i32,
            start_lifetime: mmc(init, "startLifetime"),
            start_speed: mmc(init, "startSpeed"),
            start_size: mmc(init, "startSize"),
            start_size_y: mmc(init, "startSizeY"),
            start_size_z: mmc(init, "startSizeZ"),
            size_3d: boolf(init, "size3D"),
            start_rotation: mmc(init, "startRotation"),
            start_color: init.and_then(|m| m.get("startColor")).map(MinMaxGradient::from_value).unwrap_or_default(),
            gravity_modifier: mmc(init, "gravityModifier"),
            emission_enabled: boolf(emit, "enabled"),
            rate_over_time: mmc(emit, "rateOverTime"),
            rate_over_distance: mmc(emit, "rateOverDistance"),
            bursts,
            shape_enabled: boolf(shape, "enabled"),
            shape_type: shape.and_then(|m| m.get("type")).and_then(Value::as_i64).unwrap_or(-1) as i32,
            shape_angle: shape.and_then(|m| m.get("angle")).and_then(Value::as_f32).unwrap_or(0.0),
            shape_radius: shape.and_then(|m| m.get("radius")).and_then(|r| r.get("value")).and_then(Value::as_f32).unwrap_or(0.0),
            shape_mesh: shape.and_then(|m| m.get("m_Mesh")).and_then(Value::as_pptr).unwrap_or_default(),
            velocity_enabled: boolf(vel, "enabled"),
            velocity: [mmc(vel, "x"), mmc(vel, "y"), mmc(vel, "z")],
            color_over_lifetime: col.filter(|c| c.get("enabled").and_then(Value::as_bool).unwrap_or(false))
                .and_then(|c| c.get("gradient")).map(MinMaxGradient::from_value),
            size_over_lifetime: siz.filter(|s| s.get("enabled").and_then(Value::as_bool).unwrap_or(false))
                .map(|s| MinMaxCurve::from_value(s.get("curve").unwrap_or(s))),
        }
    }
}

/// The materials referenced by a ParticleSystemRenderer (class 199) `Value`.
pub fn renderer_materials(v: &Value) -> Vec<PPtr> {
    v.get("m_Materials").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_pptr).collect()).unwrap_or_default()
}

/// The ParticleSystemRenderer render mode (0 Billboard, 1 Stretch, 2 HorizontalBillboard,
/// 3 VerticalBillboard, 4 Mesh, 5 None), if present.
pub fn renderer_mode(v: &Value) -> Option<i32> {
    v.get("m_RenderMode").and_then(Value::as_i64).map(|x| x as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bundle;
    use std::path::Path;

    /// Decode real ParticleSystems from the install's Addressables bundles (which carry type trees)
    /// and check the extracted spec is sane. Gated on `RR_BUNDLE_DIR`; skipped when unset.
    #[test]
    fn decode_bundle_particles() {
        let Ok(bdir) = std::env::var("RR_BUNDLE_DIR") else {
            eprintln!("decode_bundle_particles: skipped (set RR_BUNDLE_DIR)");
            return;
        };
        let mut files: Vec<_> = std::fs::read_dir(&bdir).unwrap().filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "bundle").unwrap_or(false)).collect();
        files.sort();
        let mut tested = 0;
        'outer: for f in &files {
            let Ok(b) = Bundle::open(f) else { continue };
            let Ok(sfs) = b.serialized_files() else { continue };
            for (_, sf) in &sfs {
                for o in sf.objects.iter().filter(|o| o.class_id == 198) {
                    let Ok(v) = sf.read_value(o) else { continue };
                    let ps = ParticleSystem::from_value(&v);
                    // a real VFX: positive length and at least emission enabled with a source of particles
                    assert!(ps.length > 0.0, "zero length");
                    assert!(ps.max_particles > 0, "zero max particles");
                    let emits = ps.rate_over_time.constant() > 0.0 || ps.rate_over_time.mode != 0 || !ps.bursts.is_empty();
                    assert!(ps.emission_enabled && emits, "no emission");
                    assert!(ps.start_lifetime.constant() > 0.0 || ps.start_lifetime.mode != 0, "zero lifetime");
                    tested += 1;
                    if tested >= 30 { break 'outer; }
                }
            }
        }
        assert!(tested > 0, "no ParticleSystems found");
        eprintln!("decode_bundle_particles: {tested} ParticleSystems decoded + validated");
    }
}
