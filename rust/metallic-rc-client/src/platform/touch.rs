//! Touch controls in the player's terms: the left half of the screen is a movement stick (the touch's travel from
//! where it went down, full speed at `STICK` pixels), the right half looks around (drag) and jumps (a short tap).
//! They drive the same inputs as keyboard and mouse (W/A/S/D and Space in `ButtonInput<KeyCode>`, the drag in
//! `AccumulatedMouseMotion`), so the player code needs no touch path of its own. INFERRED layout: the game's mobile
//! controls are not in the bundles we read.
use bevy::{
    input::{mouse::AccumulatedMouseMotion, touch::Touches, InputSystems},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

/// Stick travel for full speed, in logical pixels.
pub const STICK: f32 = 60.0;
/// Below this share of `STICK` the stick does nothing.
const DEAD: f32 = 0.3;
/// A right-half touch shorter than this (seconds) and shorter than `TAP_MOVE` pixels is a jump.
const TAP_TIME: f32 = 0.2;
const TAP_MOVE: f32 = 12.0;
/// Look speed relative to mouse pixels (the player scales it by the sensitivity settings).
const LOOK: f32 = 1.5;

pub struct TouchControlsPlugin;

impl Plugin for TouchControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchState>().add_systems(PreUpdate, touch_controls.after(InputSystems));
    }
}

/// The keys the stick holds down (released when it lets go) and when each right-half touch started.
#[derive(Resource, Default)]
struct TouchState {
    held: Vec<KeyCode>,
    jump: bool,
    started: Vec<(u64, f32)>,
}

/// The direction keys a stick offset (screen pixels, y down) asks for.
pub fn stick_keys(offset: Vec2) -> Vec<KeyCode> {
    let v = offset / STICK;
    let mut keys = vec![];
    if v.length() < DEAD {
        return keys;
    }
    let d = v.normalize();
    // within 67.5 degrees of an axis counts, so diagonals press two keys
    let cos = 22.5f32.to_radians().sin();
    if -d.y > cos {
        keys.push(KeyCode::KeyW);
    }
    if d.y > cos {
        keys.push(KeyCode::KeyS);
    }
    if d.x > cos {
        keys.push(KeyCode::KeyD);
    }
    if -d.x > cos {
        keys.push(KeyCode::KeyA);
    }
    keys
}

fn touch_controls(
    time: Res<Time<Real>>,
    touches: Res<Touches>,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut motion: ResMut<AccumulatedMouseMotion>,
    mut state: ResMut<TouchState>,
) {
    let (window, mut cursor) = window.into_inner();
    let half = window.width() / 2.0;
    let now = time.elapsed_secs();
    let mut want = vec![];
    for t in touches.iter() {
        if t.start_position().x < half {
            want = stick_keys(t.position() - t.start_position());
        } else {
            motion.delta += t.delta() * LOOK;
            // the player only turns the view while the cursor is captured; a touch screen has none to free
            if cursor.grab_mode == CursorGrabMode::None {
                cursor.grab_mode = CursorGrabMode::Locked;
            }
        }
    }
    for t in touches.iter_just_pressed().filter(|t| t.start_position().x >= half) {
        state.started.push((t.id(), now));
    }
    if state.jump {
        keys.release(KeyCode::Space);
        state.jump = false;
    }
    for t in touches.iter_just_released() {
        let Some(i) = state.started.iter().position(|(id, _)| *id == t.id()) else { continue };
        let (_, at) = state.started.remove(i);
        if now - at < TAP_TIME && t.position().distance(t.start_position()) < TAP_MOVE {
            keys.press(KeyCode::Space);
            state.jump = true;
        }
    }
    for k in state.held.clone() {
        if !want.contains(&k) {
            keys.release(k);
        }
    }
    for &k in &want {
        if !keys.pressed(k) {
            keys.press(k);
        }
    }
    state.held = want;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_directions() {
        assert!(stick_keys(Vec2::new(5.0, -5.0)).is_empty());
        assert_eq!(stick_keys(Vec2::new(0.0, -50.0)), vec![KeyCode::KeyW]);
        assert_eq!(stick_keys(Vec2::new(-40.0, 0.0)), vec![KeyCode::KeyA]);
        assert_eq!(stick_keys(Vec2::new(40.0, 40.0)), vec![KeyCode::KeyS, KeyCode::KeyD]);
    }
}
