//! The screen-mode player's tuning, read at start-up from the game's own data in the owner's install: the
//! `(Player)` prefab's `CrossPlatformPlayerLocomotionContent`, the screen player's `ScreenPlayerLocomotionContent`,
//! camera settings and the interaction configs (docs/agents/rc-spec/player.md in the private notes). Anything that is
//! not found keeps the value written here, which is the one seen in those assets for build 23191908.
use std::path::Path;

use bevy::{ecs::resource::Resource, math::Vec3};
use unity_assets::{Bundle, Value};

/// The `(Player)` prefab's bundle.
const PLAYER_BUNDLE: &str = "c513202af4f2b6439095734ab5caead0.bundle";
/// Screen-player configs (locomotion content, sprint camera, targeting, object manipulation, drop).
const SCREEN_BUNDLE: &str = "063b6b6653aa642616fd21d3d4701899.bundle";
/// More camera settings and the default throw config.
const CAMERA_BUNDLE: &str = "1df8e0efac4f496ebc58d7e6f86480c3.bundle";

/// A Unity AnimationCurve: keys of (time, value, in slope, out slope), evaluated as Unity does for unweighted keys
/// (cubic Hermite between keys, clamped outside them).
#[derive(Clone, Debug, PartialEq)]
pub struct Curve(pub Vec<[f32; 4]>);

impl Curve {
    pub fn eval(&self, t: f32) -> f32 {
        let k = &self.0;
        match k.len() {
            0 => return 0.0,
            1 => return k[0][1],
            _ => {}
        }
        if t <= k[0][0] {
            return k[0][1];
        }
        let Some(i) = k.windows(2).position(|w| t <= w[1][0]) else { return k[k.len() - 1][1] };
        let ([t0, v0, _, out0], [t1, v1, in1, _]) = (k[i], k[i + 1]);
        let dt = t1 - t0;
        let s = (t - t0) / dt;
        let (s2, s3) = (s * s, s * s * s);
        (2.0 * s3 - 3.0 * s2 + 1.0) * v0 + (s3 - 2.0 * s2 + s) * out0 * dt + (-2.0 * s3 + 3.0 * s2) * v1 + (s3 - s2) * in1 * dt
    }
}

/// One camera preset (`ScreenPlayerCameraSettings.TargetingData`), in Unity's axes (z forward).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPreset {
    /// from the feet to the pivot the camera turns about
    pub pivot: Vec3,
    /// from the pivot to the camera, turned with the view
    pub offset: Vec3,
    pub fov: f32,
    /// how close to straight up the view may turn, degrees (0: no limit)
    pub min_pitch_from_up: f32,
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Tuning {
    pub walk_speed: f32,
    pub walk_accel_time: f32,
    pub sprint_mult: f32,
    pub sprint_accel_time: f32,
    pub sprint_lateral: f32,
    pub sprint_cancel_deg: f32,
    pub step_height: f32,
    pub fall_terminal: f32,
    pub air_control: f32,
    pub air_drag: f32,
    pub ground_drag: f32,
    pub jump_cutoff_mult: f32,
    pub crouch_height: f32,
    pub crouch_speed: f32,
    pub fly_speed: f32,
    pub fly_accel_time: f32,
    pub fly_brake_time: f32,
    /// the fly speed (as a part of the most) over the seconds flown, over `fly_curve_time`
    pub fly_curve: Curve,
    pub fly_curve_time: f32,
    /// `PlayerMovement.Awake`'s max jump height token default (code, not an asset)
    pub jump_height: f32,
    /// INFERRED: the project's gravity (the settings file did not parse); Unity's default
    pub gravity: f32,
    /// INFERRED: how long after walking off a ledge a jump still counts (the state machine has a 0.1 const)
    pub coyote: f32,
    /// normalized height per second when crouching or standing up
    pub height_change_speed: f32,
    pub first_person: CameraPreset,
    pub sprint_camera: CameraPreset,
    pub sprint_camera_blend: f32,
    pub third_person: CameraPreset,
    pub camera_blend: f32,
    pub reach: f32,
    pub hold_distance: f32,
    pub min_hold_distance: f32,
    pub throw_speed: f32,
    /// degrees per second
    pub throw_spin: f32,
    pub drop_speed: f32,
    /// what was read from the game files, for the log
    pub found: Vec<&'static str>,
}

impl Default for Tuning {
    fn default() -> Self {
        let fp = CameraPreset { pivot: Vec3::new(0.0, 1.35, -0.08), offset: Vec3::new(0.0, 0.2, 0.09), fov: 1.0, min_pitch_from_up: 0.0 };
        Tuning {
            walk_speed: 3.0,
            walk_accel_time: 0.15,
            sprint_mult: 1.5,
            sprint_accel_time: 0.225,
            sprint_lateral: 0.4,
            sprint_cancel_deg: 89.0,
            step_height: 0.25,
            fall_terminal: 20.0,
            air_control: 0.5,
            air_drag: 2.0,
            ground_drag: 10.0,
            jump_cutoff_mult: 0.5,
            crouch_height: 0.725,
            crouch_speed: 0.75,
            fly_speed: 35.0,
            fly_accel_time: 0.15,
            fly_brake_time: 0.15,
            fly_curve: Curve(vec![[0.0, 0.15, 0.2157, 0.2157], [1.0, 1.0, 1.5185, 1.5185]]),
            fly_curve_time: 2.0,
            jump_height: 0.75,
            gravity: 9.81,
            coyote: 0.1,
            height_change_speed: 1.38,
            first_person: fp,
            sprint_camera: CameraPreset { pivot: Vec3::new(0.0, 1.4, -0.08), fov: 1.15, ..fp },
            sprint_camera_blend: 1.0,
            third_person: CameraPreset { pivot: Vec3::new(0.0, 1.6, 0.0), offset: Vec3::new(0.4875, 0.0, -1.8), fov: 0.85, min_pitch_from_up: 30.0 },
            camera_blend: 0.1,
            reach: 2.5,
            hold_distance: 1.0,
            min_hold_distance: 0.33,
            throw_speed: 8.0,
            throw_spin: 360.0,
            drop_speed: 0.1,
            found: vec![],
        }
    }
}

/// The MonoBehaviours of a bundle, read through their type trees.
fn behaviours(dir: &Path, bundle: &str) -> Vec<Value> {
    let Ok(b) = Bundle::open(dir.join(bundle)) else { return vec![] };
    let Ok(files) = b.serialized_files() else { return vec![] };
    files.iter().flat_map(|(_, sf)| sf.objects.iter().filter(|o| o.class_id == 114).filter_map(|o| sf.read_value(o).ok()).collect::<Vec<_>>()).collect()
}

fn named<'a>(all: &'a [Value], name: &str) -> Option<&'a Value> {
    all.iter().find(|v| v.get("m_Name").and_then(Value::as_str) == Some(name))
}

fn with_field<'a>(all: &'a [Value], field: &str) -> Option<&'a Value> {
    all.iter().find(|v| v.get(field).is_some())
}

fn set(v: &Value, path: &str, to: &mut f32) {
    if let Some(x) = v.path(path).and_then(Value::as_f32) {
        *to = x;
    }
}

fn vec3(v: &Value, path: &str, to: &mut Vec3) {
    for (i, c) in ["x", "y", "z"].iter().enumerate() {
        set(v, &format!("{path}/{c}"), &mut to[i]);
    }
}

fn curve(v: &Value, path: &str) -> Option<Curve> {
    let keys = v.path(&format!("{path}/m_Curve"))?.as_array()?;
    let f = |k: &Value, n: &str| k.get(n).and_then(Value::as_f32).unwrap_or(0.0);
    Some(Curve(keys.iter().map(|k| [f(k, "time"), f(k, "value"), f(k, "inSlope"), f(k, "outSlope")]).collect())).filter(|c| !c.0.is_empty())
}

fn preset(v: &Value, to: &mut CameraPreset) {
    vec3(v, "TargetingData/TargetToPivotOffset", &mut to.pivot);
    vec3(v, "TargetingData/PivotToCameraOffset", &mut to.offset);
    set(v, "TargetingData/fieldOfViewMultiplier", &mut to.fov);
    set(v, "TargetingData/MinPitchAngleFromUp", &mut to.min_pitch_from_up);
}

impl Tuning {
    /// The tuning from the install's bundle folder; what is missing keeps its default.
    pub fn load(dir: &Path) -> Tuning {
        let mut t = Tuning::default();
        let player = behaviours(dir, PLAYER_BUNDLE);
        if let Some(c) = with_field(&player, "WalkDefaultMaxSpeed") {
            for (name, to) in [
                ("WalkDefaultMaxSpeed", &mut t.walk_speed),
                ("WalkAccelerationDuration", &mut t.walk_accel_time),
                ("SprintMaxSpeedDefaultWalkSpeedMultiplier", &mut t.sprint_mult),
                ("SprintAccelerationDuration", &mut t.sprint_accel_time),
                ("SprintMaxLateralScaler", &mut t.sprint_lateral),
                ("SprintCancelAngleFromForward", &mut t.sprint_cancel_deg),
                ("StepHeight", &mut t.step_height),
                ("FallTerminalSpeed", &mut t.fall_terminal),
                ("FallDefaultAirControlPercentage", &mut t.air_control),
                ("AirDragAcceleration", &mut t.air_drag),
                ("GroundDragAcceleration", &mut t.ground_drag),
                ("JumpCutoffHeightMultiplier", &mut t.jump_cutoff_mult),
                ("CrouchNormalizedHeight", &mut t.crouch_height),
                ("CrouchDefaultSpeedMultiplier", &mut t.crouch_speed),
                ("FlyMaxSpeed", &mut t.fly_speed),
                ("FlyForwardAccelerationDuration", &mut t.fly_accel_time),
                ("FlyBrakeAccelerationDuration", &mut t.fly_brake_time),
                ("FlyMaxSpeedNormalizedCurveMaxDuration", &mut t.fly_curve_time),
            ] {
                set(c, name, to);
            }
            if let Some(k) = curve(c, "FlyMaxSpeedNormalizedCurve") {
                t.fly_curve = k;
            }
            t.found.push("CrossPlatformPlayerLocomotionContent");
        }
        let screen = behaviours(dir, SCREEN_BUNDLE);
        if let Some(c) = named(&screen, "ScreenPlayerLocomotionContent") {
            set(c, "HeightChangeSpeed", &mut t.height_change_speed);
            set(c, "SprintCameraSettingsBlendTime", &mut t.sprint_camera_blend);
            t.found.push("ScreenPlayerLocomotionContent");
        }
        if let Some(c) = named(&screen, "FPS_Sprint_CameraSettings") {
            preset(c, &mut t.sprint_camera);
            t.found.push("FPS_Sprint_CameraSettings");
        }
        if let Some(c) = named(&screen, "ThirdPersonScreenPlayerModeConfig") {
            set(c, "CameraBlendTime", &mut t.camera_blend);
            t.found.push("ThirdPersonScreenPlayerModeConfig");
        }
        if let Some(c) = named(&screen, "ScreenInteractableTargetingSystemConfig") {
            set(c, "MaxSearchDistanceFromPlayer", &mut t.reach);
            t.found.push("ScreenInteractableTargetingSystemConfig");
        }
        if let Some(c) = named(&screen, "ObjectManipulationPerHandScreenPlayerBehaviorConfig") {
            set(c, "DefaultToolRelativeDistance", &mut t.hold_distance);
            set(c, "MinToolDistance", &mut t.min_hold_distance);
            t.found.push("ObjectManipulationPerHandScreenPlayerBehaviorConfig");
        }
        if let Some(c) = named(&screen, "DropToolPerHandButtonHoldScreenPlayerBehaviorConfig") {
            set(c, "dropSpeed", &mut t.drop_speed);
            t.found.push("DropToolPerHandButtonHoldScreenPlayerBehaviorConfig");
        }
        let cameras = behaviours(dir, CAMERA_BUNDLE);
        if let Some(c) = named(&cameras, "FirstPersonScreenPlayerCameraSettings") {
            preset(c, &mut t.first_person);
            t.found.push("FirstPersonScreenPlayerCameraSettings");
        }
        if let Some(c) = named(&cameras, "ThirdPersonOverShoulderCameraSettings") {
            preset(c, &mut t.third_person);
            t.found.push("ThirdPersonOverShoulderCameraSettings");
        }
        if let Some(c) = named(&cameras, "ThrowToolBehaviorConfig") {
            set(c, "throwSpeed", &mut t.throw_speed);
            set(c, "throwAngularSpeed", &mut t.throw_spin);
            t.found.push("ThrowToolBehaviorConfig");
        }
        t
    }

    /// The eyes above the feet when standing: pivot plus offset of the first-person camera (1.55 m).
    pub fn eye(&self) -> f32 {
        self.first_person.pivot.y + self.first_person.offset.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_matches_its_keys_and_eases_between() {
        let c = Tuning::default().fly_curve;
        assert!((c.eval(0.0) - 0.15).abs() < 1e-5 && (c.eval(1.0) - 1.0).abs() < 1e-5);
        assert_eq!(c.eval(-1.0), 0.15);
        assert_eq!(c.eval(3.0), 1.0);
        let (a, b) = (c.eval(0.25), c.eval(0.75));
        assert!(0.15 < a && a < b && b < 1.0, "{a} {b}");
        // the flat start: a quarter of the way in is still well under a quarter of the rise
        assert!(a < 0.15 + 0.25 * 0.85, "{a}");
    }

    #[test]
    fn missing_files_give_the_defaults() {
        let t = Tuning::load(Path::new("no/such/folder"));
        assert_eq!(t, Tuning::default());
        assert!((t.eye() - 1.55).abs() < 1e-6);
    }

    /// Reads the owner's install when it is there (the values are the ones in the spec).
    #[test]
    fn reads_the_installed_game() {
        let Some(dir) = crate::room::GAME_DIRS.iter().map(|d| crate::room::bundle_dir(Path::new(d))).find(|d| d.join(PLAYER_BUNDLE).exists()) else {
            eprintln!("no install: skipped");
            return;
        };
        let t = Tuning::load(&dir);
        assert_eq!(t.found.len(), 10, "{:?}", t.found);
        assert_eq!((t.walk_speed, t.sprint_mult, t.fall_terminal, t.fly_speed, t.throw_speed, t.reach), (3.0, 1.5, 20.0, 35.0, 8.0, 2.5));
        assert!((t.crouch_height - 0.725).abs() < 1e-6 && (t.height_change_speed - 1.38).abs() < 1e-6);
        assert!((t.third_person.offset.z + 1.8).abs() < 1e-6 && (t.sprint_camera.fov - 1.15).abs() < 1e-6);
        assert!((t.fly_curve.eval(0.0) - 0.15).abs() < 1e-6);
    }
}
