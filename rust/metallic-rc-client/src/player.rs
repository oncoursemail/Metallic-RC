//! The local player in screen mode: mouse look, movement with the game's own tuning (`tuning`, read from the
//! install), the camera (first person, or V for over the shoulder) and the player's own avatar. `PlayerPlugin` adds
//! all of it, the interaction (`interact`) and the avatar editor (`avatar::panel`).
//!
//! Keys: WASD walk, Shift sprint (the Auto Sprint setting turns it round; `sprint_toggle` makes it a toggle), Space
//! jump (hold for the full height), Ctrl crouch, F fly (Space/Ctrl up/down), V third person, C avatar editor.
pub mod camera;
pub mod motor;
pub mod tuning;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::{
    avatar::{self, Drive, Kit, LocalLook, LookPanel, Rig},
    collide::{self, Collider},
    interact::{self, LocalHands, PropSolids},
    settings::Settings,
};
use motor::Motor;
pub use tuning::Tuning;

/// Eyes above the feet when standing, before the tuning arrives (the first-person preset's 1.35 + 0.2).
pub const EYE: f32 = 1.55;

/// What the player bumps into: the room's box colliders, kept by `world::WorldPlugin`.
pub use crate::collide::Solids;

/// The settings and whether the keyboard belongs to a text field; the game fills them in each frame.
#[derive(Resource, Default)]
pub struct Controls {
    pub settings: Settings,
    pub typing: bool,
}

/// V toggles it.
#[derive(Resource, Default)]
pub struct ThirdPerson(pub bool);

/// The camera, a child of the player (at the eyes): there in first person, behind the shoulder in third.
#[derive(Component)]
pub struct View;

/// This player's own avatar: only its hands show in first person.
#[derive(Component)]
pub struct LocalAvatar;

/// The local player; its transform is at the eyes, turned with the view.
#[derive(Component)]
pub struct Player {
    pub yaw: f32,
    pub pitch: f32,
    pub motor: Motor,
    /// eyes above the feet now (lower when crouched)
    pub eye: f32,
    sprint_toggled: bool,
    /// 0..1: how far the sprint camera has blended in
    sprint_view: f32,
}

impl Player {
    pub fn feet(&self, t: &Transform) -> Vec3 {
        t.translation - Vec3::Y * self.eye
    }

    /// Put back on the floor facing `yaw` (a spawn from the room).
    pub fn respawn(&mut self, yaw: f32) {
        (self.yaw, self.pitch) = (yaw, 0.0);
        self.motor = Motor::new();
    }
}

/// The player's systems, in this order each frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlayerSystems {
    /// look, walk and the camera
    Move,
    /// targeting, grabbing, holding, throwing
    Interact,
    /// the player's own avatar follows
    Avatar,
}

/// The tuning as the loader thread hands it over.
#[derive(Resource, Clone, Default)]
struct TuningSlot(Arc<Mutex<Option<Tuning>>>);

pub struct PlayerPlugin {
    /// the install to read the tuning from
    pub game_dir: Option<PathBuf>,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        let slot = TuningSlot::default();
        if let Some(dir) = self.game_dir.clone().map(|g| crate::room::bundle_dir(&g)) {
            let s = slot.0.clone();
            std::thread::spawn(move || *s.lock().unwrap() = Some(Tuning::load(&dir)));
        }
        app.insert_resource(slot)
            .init_resource::<Tuning>()
            .init_resource::<Solids>()
            .init_resource::<Controls>()
            .init_resource::<ThirdPerson>()
            .init_resource::<LookPanel>()
            .init_resource::<LocalHands>()
            .add_plugins(interact::InteractPlugin)
            .configure_sets(Update, (PlayerSystems::Move, PlayerSystems::Interact, PlayerSystems::Avatar).chain())
            .add_systems(Update, ((take_tuning, look_and_walk, camera_view).chain().in_set(PlayerSystems::Move), local_avatar.in_set(PlayerSystems::Avatar)))
            .add_systems(Update, avatar::panel::look_panel.run_if(|c: Res<Controls>| !c.typing))
            .add_systems(PostUpdate, avatar::panel::free_cursor);
    }
}

fn take_tuning(slot: Res<TuningSlot>, mut tuning: ResMut<Tuning>) {
    if let Some(t) = slot.0.lock().unwrap().take() {
        info!("player tuning: {} from the game files", if t.found.is_empty() { "nothing".into() } else { t.found.join(", ") });
        *tuning = t;
    }
}

/// Spawns the player with its camera: `start` is x, y, z (eyes), yaw, pitch (degrees). A start above head height (an
/// overview) starts flying instead of falling to the floor.
pub fn spawn(commands: &mut Commands, start: [f32; 5], fov_degrees: f32) {
    let [x, y, z, yaw, pitch] = start;
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let mut motor = Motor::new();
    motor.flying = y > EYE + 0.5;
    commands.spawn((
        Player { yaw, pitch, motor, eye: EYE, sprint_toggled: false, sprint_view: 0.0 },
        Transform::from_xyz(x, y, z).with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)),
        Visibility::default(),
    )).with_child((
        Camera3d::default(),
        // the game's camera: near 0.06, far 1000
        Projection::Perspective(PerspectiveProjection { fov: fov_degrees.to_radians(), near: 0.06, far: 1000.0, ..default() }),
        View,
        Transform::default(),
    ));
}

fn key(keys: &ButtonInput<KeyCode>, k: &[KeyCode]) -> bool {
    keys.any_pressed(k.iter().copied())
}

#[allow(clippy::too_many_arguments)]
fn look_and_walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut player: Single<(&mut Player, &mut Transform)>,
    (solids, controls, tuning, third, panel): (Res<Solids>, Res<Controls>, Res<Tuning>, Res<ThirdPerson>, Res<LookPanel>),
    props: Res<PropSolids>,
) {
    let (p, t) = &mut *player;
    // typing into the menu does not walk; the editor keeps the arrow keys
    let idle = ButtonInput::default();
    let keys = if controls.typing || panel.open.is_some() { &idle } else { &*keys };
    let s = &controls.settings;
    let preset = if third.0 { &tuning.third_person } else { &tuning.first_person };
    if cursor.grab_mode != CursorGrabMode::None && panel.open.is_none() {
        // 0.002 rad per pixel at the game's default sensitivity, 50; INFERRED: the look sliders scale each axis the same way
        let k = 0.002 * s.get("mouse_sensitivity") as f32 / 50.0;
        let axis = |look: &str, invert: &str| k * s.get(look) as f32 / 50.0 * if s.on(invert) { -1.0 } else { 1.0 };
        p.yaw -= motion.delta.x * axis("look_horizontal", "invert_x");
        p.pitch -= motion.delta.y * axis("look_vertical", "invert_y");
    }
    p.pitch = p.pitch.clamp(-1.5, camera::max_pitch(preset));
    t.rotation = Quat::from_euler(EulerRot::YXZ, p.yaw, p.pitch, 0.0);

    // a frame that took long (the room arriving, a dragged window) moves no more than a tenth of a second's worth
    let dt = time.delta_secs().min(0.1);
    if keys.just_pressed(KeyCode::KeyF) {
        p.motor.toggle_fly();
    }
    let forward = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
    let mut wish = Vec3::ZERO;
    for (k, dir) in [(KeyCode::KeyW, forward), (KeyCode::KeyS, -forward), (KeyCode::KeyD, right), (KeyCode::KeyA, -right)] {
        if keys.pressed(k) {
            wish += dir;
        }
    }
    let ctrl = key(keys, &[KeyCode::ControlLeft, KeyCode::ControlRight]);
    if p.motor.flying {
        wish.y += keys.pressed(KeyCode::Space) as i32 as f32 - ctrl as i32 as f32;
    }
    let shift = [KeyCode::ShiftLeft, KeyCode::ShiftRight];
    let mut toggled = p.sprint_toggled;
    let sprint = motor::wants_sprint(s.on("auto_sprint"), s.on("sprint_toggle"), key(keys, &shift), keys.any_just_pressed(shift), &mut toggled);
    p.sprint_toggled = toggled;
    let feet = p.feet(t);
    let room = &solids.0[..];
    // room to stand: the top of a standing capsule touches nothing (the room's boxes or the props lying about)
    let head = feet + Vec3::Y * (collide::HEIGHT - collide::RADIUS);
    let head_room = collide::sphere_clear(room, head, collide::RADIUS * 0.95) && collide::sphere_clear(&props.0, head, collide::RADIUS * 0.95);
    let input = motor::Input {
        wish: wish.normalize_or_zero(),
        forward,
        sprint,
        jump: keys.just_pressed(KeyCode::Space),
        jump_held: keys.pressed(KeyCode::Space),
        crouch: ctrl && !p.motor.flying,
        head_room,
    };
    p.motor.step(&tuning, &input, dt);
    let asked = p.motor.velocity * dt;
    let (mut next, mut grounded) = if p.motor.flying { (feet + asked, false) } else { walk(room, &props.0, feet, asked) };
    // the stand-in room has no colliders: its floor is y = 0
    if room.is_empty() && next.y < 0.0 && !p.motor.flying {
        (next.y, grounded) = (0.0, true);
    }
    p.motor.after_move(grounded, next - feet, dt);
    p.eye = tuning.eye() * p.motor.height;
    t.translation = next + Vec3::Y * p.eye;
}

/// Moves the player's capsule from `feet` by `asked`: against the room's boxes, then out of the props sideways or up
/// onto them, never down (a prop falling on the player would press it through the floor) and not into the room's
/// walls. Returns where the feet end and whether they stand on something.
pub fn walk(room: &[Collider], props: &[Collider], feet: Vec3, asked: Vec3) -> (Vec3, bool) {
    let (mut next, mut grounded) = collide::move_capsule(room, feet, asked);
    if !props.is_empty() {
        let (out, on_prop) = collide::move_capsule(props, next, Vec3::ZERO);
        let push = Vec3::new(out.x - next.x, (out.y - next.y).max(0.0), out.z - next.z);
        if push != Vec3::ZERO {
            let (moved, g) = collide::move_capsule(room, next, push);
            (next, grounded) = (moved, grounded || g);
        }
        grounded |= on_prop;
    }
    (next, grounded)
}

/// V switches first and third person. The camera eases to its place: at the eyes (bobbing with the Head Bob
/// setting), over the shoulder (pulled in where a wall is in the way), or, while the avatar editor is open, in front of
/// the player looking back at it. The field of view follows the setting, times the preset's multiplier (wider while
/// sprinting).
#[allow(clippy::too_many_arguments)]
fn camera_view(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut third: ResMut<ThirdPerson>,
    (panel, solids, controls, tuning): (Res<LookPanel>, Res<Solids>, Res<Controls>, Res<Tuning>),
    mut player: Single<(&mut Player, &Transform), Without<View>>,
    mut view: Single<(&mut Transform, &mut Projection), With<View>>,
    rig: Query<&Rig, With<LocalAvatar>>,
) {
    let fov_setting = (controls.settings.get("fov") as f32).clamp(30.0, 120.0);
    if keys.just_pressed(KeyCode::KeyV) && !controls.typing {
        third.0 = !third.0;
    }
    let dt = time.delta_secs();
    let (p, eye) = &mut *player;
    let (tr, projection) = &mut *view;
    let feet = p.feet(eye);
    let moving_fast = p.motor.velocity.length() > tuning.walk_speed + 0.1;
    let goal = if p.motor.sprinting && moving_fast { 1.0 } else { 0.0 };
    p.sprint_view += (goal - p.sprint_view).clamp(-dt / tuning.sprint_camera_blend.max(1e-3), dt / tuning.sprint_camera_blend.max(1e-3));
    let to_local = |at: Transform| Transform::from_matrix(eye.to_matrix().inverse() * at.to_matrix());
    let (want, fov) = if panel.open.is_some() {
        // world place first: 2.2 m in front, a little above the chest, looking back at the whole body
        let body = Quat::from_rotation_y(p.yaw);
        let at = Transform::from_translation(feet + body * Vec3::new(0.0, 1.3, -2.2)).looking_at(feet + Vec3::Y * 0.95, Vec3::Y);
        (to_local(at), fov_setting * 0.85)
    } else if third.0 {
        let (pivot, far) = camera::place(&tuning.third_person, feet, p.yaw, eye.rotation, p.motor.height);
        // INFERRED: a 0.15 m ball keeps the camera off the walls
        let at = camera::pull_in(&solids.0, pivot, far, 0.15);
        (to_local(Transform::from_translation(at).with_rotation(eye.rotation)), fov_setting * tuning.third_person.fov)
    } else {
        // the Head Bob slider, 0..100 (INFERRED scale; unset means none)
        let amount = controls.settings.get("head_bob") as f32;
        let amount = if amount > 1.0 { amount / 100.0 } else { amount };
        let bob = rig.single().ok().filter(|_| p.motor.grounded).map_or(Vec3::ZERO, |r| camera::bob(amount, r.anim.phase, r.anim.speed));
        let lift = (tuning.sprint_camera.pivot.y - tuning.first_person.pivot.y) * p.sprint_view;
        let mult = 1.0 + (tuning.sprint_camera.fov - 1.0) * p.sprint_view;
        (Transform::from_translation(bob + Vec3::Y * lift), fov_setting * mult)
    };
    // CameraBlendTime 0.1 s
    let k = 1.0 - (-dt / (tuning.camera_blend * 0.5).max(1e-3)).exp();
    tr.translation = tr.translation.lerp(want.translation, k);
    tr.rotation = tr.rotation.slerp(want.rotation, k);
    if let Projection::Perspective(pp) = &mut **projection {
        pp.fov += (fov.to_radians() - pp.fov) * k;
    }
}

/// The local player's own avatar: spawned once the kit is made, kept at the feet, driven by the movement, with the
/// screen-mode hands low in front of the eyes (swaying as the view turns and bobbing with the stride), or where the
/// interaction puts them.
#[allow(clippy::too_many_arguments)]
fn local_avatar(
    mut commands: Commands,
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    (look, third, panel, hands): (Res<LocalLook>, Res<ThirdPerson>, Res<LookPanel>, Res<LocalHands>),
    motion: Res<AccumulatedMouseMotion>,
    player: Single<(&Player, &Transform), Without<LocalAvatar>>,
    mut mine: Query<(&mut Transform, &mut Drive, &mut Rig), With<LocalAvatar>>,
    mut last_feet: Local<Option<Vec3>>,
    mut sway: Local<Vec2>,
) {
    let (p, eye) = *player;
    let feet = p.feet(eye);
    let at = Transform::from_translation(feet).with_rotation(Quat::from_rotation_y(p.yaw));
    let Ok((mut tr, mut drive, mut rig)) = mine.single_mut() else {
        if let Some(kit) = kit {
            let e = avatar::spawn(&mut commands, &kit, &mut materials, look.0, at, None, 0);
            commands.entity(e).insert((LocalAvatar, Name::new("local avatar")));
        }
        return;
    };
    let dt = time.delta_secs().max(1e-3);
    // the real velocity, from where the feet went
    drive.velocity = last_feet.map_or(Vec3::ZERO, |l| (feet - l) / dt);
    *last_feet = Some(feet);
    (drive.grounded, drive.flying, drive.head, drive.head_height) = (Some(p.motor.grounded), p.motor.flying, Some(eye.rotation), Some(p.eye));
    // the hands lag a little behind the turning view
    let target = (-motion.delta * 0.0004).clamp(Vec2::splat(-0.04), Vec2::splat(0.04)) * Vec2::new(1.0, -1.0);
    *sway = sway.lerp(target, 1.0 - (-dt * 8.0).exp());
    let bob = -0.015 * rig.anim.phase.sin().abs() * (rig.anim.speed / 3.0).min(1.0);
    let screen = avatar::screen_hands(eye, *sway, bob);
    drive.hands = [0, 1].map(|i| Some(hands.place[i].unwrap_or(screen[i])));
    drive.grip = hands.grip;
    *tr = at;
    let first_person = !third.0 && panel.open.is_none();
    if rig.first_person != first_person {
        rig.first_person = first_person;
    }
    if rig.look != look.0 {
        rig.look = look.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collide::Obb;

    fn cube(center: Vec3, half: Vec3) -> Collider {
        Collider::Box(Obb { center, axes: [Vec3::X, Vec3::Y, Vec3::Z], half })
    }

    #[test]
    fn a_prop_falling_on_the_player_does_not_press_it_through_the_floor() {
        let floor = [cube(Vec3::new(0.0, -0.5, 0.0), Vec3::new(10.0, 0.5, 10.0))];
        let mut feet = Vec3::ZERO;
        // a 0.3 m crate coming down through where the player's chest is
        for i in 0..30 {
            let crate_ = [cube(Vec3::new(0.0, 1.4 - i as f32 * 0.05, 0.1), Vec3::splat(0.15))];
            (feet, _) = walk(&floor, &crate_, feet, Vec3::new(0.0, -0.05, 0.0));
            assert!(feet.y > -0.01, "step {i}: {feet}");
        }
    }

    #[test]
    fn the_player_stands_on_a_prop_and_is_pushed_aside_by_one() {
        let floor = [cube(Vec3::new(0.0, -0.5, 0.0), Vec3::new(10.0, 0.5, 10.0))];
        // a low box (one step high) under the feet: stood on
        let low = [cube(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.3, 0.1, 0.3))];
        let (feet, grounded) = walk(&floor, &low, Vec3::new(0.0, 0.18, 0.0), Vec3::new(0.0, -0.05, 0.0));
        assert!(grounded && (feet.y - 0.2).abs() < 0.02, "{feet}");
        // a tall box beside the player: pushed out sideways, still on the floor
        let tall = [cube(Vec3::new(0.4, 0.5, 0.0), Vec3::new(0.2, 0.5, 0.2))];
        let (feet, grounded) = walk(&floor, &tall, Vec3::ZERO, Vec3::new(0.0, -0.05, 0.0));
        assert!(grounded && feet.x < -0.04 && feet.y.abs() < 0.01, "{feet}");
    }
}
