//! A test driver: with `METALLIC_RC_DRIVE=<file>` set, the client reads commands appended to that file and plays them
//! as input of its own window only (posted OS input reaches whichever window is in front, and several test clients
//! run side by side). One command per line:
//!
//! - `click X Y`: the pointer to window pixel (X, Y), or a fraction of the window when both are <= 1, and a left click
//! - `key NAME` / `down NAME` / `up NAME`: a key (Bevy `KeyCode` name: Escape, Tab, KeyC, ControlLeft, ...) pressed and
//!   released, held, let go
//! - `turn YAW PITCH`: turns the player by degrees
//! - `shot FILE`: saves a screenshot of the window as a PNG
//!
//! Without the variable nothing is added.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::{
    input::{
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
        ButtonState, InputSystems,
    },
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::PrimaryWindow,
};

use crate::player::Player;

/// Where the driver last put the pointer (window logical pixels); the menu uses it instead of the OS cursor.
#[derive(Resource, Default)]
pub struct DrivenPointer(pub Option<Vec2>);

#[derive(Resource, Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

pub struct DrivePlugin;

impl Plugin for DrivePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrivenPointer>();
        let Some(path) = std::env::var_os("METALLIC_RC_DRIVE").map(PathBuf::from) else { return };
        let lines = Lines::default();
        let inbox = lines.0.clone();
        // the file is read from where it was when the client started: older commands are not replayed
        std::thread::spawn(move || {
            let mut done = std::fs::read_to_string(&path).map_or(0, |t| t.lines().count());
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let all: Vec<&str> = text.lines().collect();
                if all.len() > done {
                    inbox.lock().unwrap().extend(all[done..].iter().map(|l| l.to_string()));
                    done = all.len();
                }
            }
        });
        app.insert_resource(lines).add_systems(PreUpdate, play.before(InputSystems));
    }
}

fn key_code(name: &str) -> Option<KeyCode> {
    Some(match name {
        "Escape" => KeyCode::Escape,
        "Tab" => KeyCode::Tab,
        "Enter" => KeyCode::Enter,
        "Space" => KeyCode::Space,
        "F3" => KeyCode::F3,
        "ControlLeft" => KeyCode::ControlLeft,
        "ShiftLeft" => KeyCode::ShiftLeft,
        n if n.len() == 4 && n.starts_with("Key") => match n.as_bytes()[3] {
            b'A' => KeyCode::KeyA, b'C' => KeyCode::KeyC, b'D' => KeyCode::KeyD, b'E' => KeyCode::KeyE, b'F' => KeyCode::KeyF,
            b'N' => KeyCode::KeyN, b'P' => KeyCode::KeyP, b'Q' => KeyCode::KeyQ, b'S' => KeyCode::KeyS, b'V' => KeyCode::KeyV,
            b'W' => KeyCode::KeyW,
            _ => return None,
        },
        _ => return None,
    })
}

/// One command per frame (a click's release goes the frame after its press).
#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    lines: Res<Lines>,
    mut pending_release: Local<bool>,
    mut pointer: ResMut<DrivenPointer>,
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut player: Query<&mut Player>,
) {
    let (id, w) = *window;
    if std::mem::take(&mut *pending_release) {
        buttons.write(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Released, window: id });
        return;
    }
    let Some(line) = ({
        let mut l = lines.0.lock().unwrap();
        (!l.is_empty()).then(|| l.remove(0))
    }) else {
        return;
    };
    let words: Vec<&str> = line.split_whitespace().collect();
    let num = |i: usize| words.get(i).and_then(|w| w.parse::<f32>().ok()).unwrap_or(0.0);
    let key = |state| words.get(1).and_then(|n| key_code(n)).map(|k| KeyboardInput {
        key_code: k, logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified), state, text: None, repeat: false, window: id,
    });
    match words.first().copied() {
        Some("click") => {
            let (x, y) = (num(1), num(2));
            let at = if x <= 1.0 && y <= 1.0 { Vec2::new(x * w.width(), y * w.height()) } else { Vec2::new(x, y) };
            pointer.0 = Some(at);
            buttons.write(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Pressed, window: id });
            *pending_release = true;
        }
        Some("key") => {
            if let (Some(down), Some(up)) = (key(ButtonState::Pressed), key(ButtonState::Released)) {
                keys.write_batch([down, up]);
            }
        }
        Some("down") => drop(key(ButtonState::Pressed).map(|k| keys.write(k))),
        Some("up") => drop(key(ButtonState::Released).map(|k| keys.write(k))),
        Some("turn") => {
            for mut p in &mut player {
                p.yaw += num(1).to_radians();
                p.pitch += num(2).to_radians();
            }
        }
        Some("shot") => {
            if let Some(file) = words.get(1) {
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(file.to_string()));
            }
        }
        _ => warn!("drive: unknown command {line:?}"),
    }
}
