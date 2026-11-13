//! The screen-mode movement maths, without Bevy's ECS: the velocity the player wants from its input and the game's
//! tuning (walk, sprint, crouch, jump with coyote time and early-release cut-off, air control and drag, gravity to a
//! terminal speed, flying with its speed curve), and the crouch height. The caller moves the body through the room
//! (`collide::move_capsule`) and reports back with `after_move`.
//!
//! The acceleration model is INFERRED (the state machine's body was not read): velocity moves toward the target in a
//! straight line at max speed / acceleration time (3 / 0.15 = 20 m/s^2 walking), as `PlayerMovement`'s config builder
//! sets `walkAccel = walkDecel = walkMax / walkAccelDuration`.
use bevy::math::{Vec2, Vec3};

use super::tuning::Tuning;

/// What the player asks for this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// wanted direction in the world, horizontal unless flying (then y is up/down); length at most 1
    pub wish: Vec3,
    /// where the view faces, horizontal, unit length
    pub forward: Vec3,
    pub sprint: bool,
    /// jump pressed this frame, and held now
    pub jump: bool,
    pub jump_held: bool,
    pub crouch: bool,
    /// whether there is room to stand up (the caller checks the room above the head)
    pub head_room: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Motor {
    pub velocity: Vec3,
    pub grounded: bool,
    pub flying: bool,
    /// normalized height: 1 standing, `crouch_height` crouched
    pub height: f32,
    /// whether the body is sprinting this frame (moving, sprint asked, not crouched, within the cancel angle)
    pub sprinting: bool,
    /// seconds since last on the ground
    pub air_time: f32,
    /// rising from a jump (so letting go of jump can cut it short)
    pub jumping: bool,
    /// seconds of flying with input, for the speed curve
    pub fly_time: f32,
}

/// Moves `v` toward `to` by at most `step`.
pub fn toward(v: Vec3, to: Vec3, step: f32) -> Vec3 {
    let d = to - v;
    let len = d.length();
    if len <= step || len < 1e-6 { to } else { v + d / len * step }
}

/// The speed to jump with to reach `height` under `gravity`.
pub fn jump_speed(gravity: f32, height: f32) -> f32 {
    (2.0 * gravity * height).sqrt()
}

/// Whether to sprint, given the sprint settings: auto sprint inverts the key (hold to walk); toggle mode flips
/// `toggled` on each press. INFERRED: the game's sprint-mode choice (`ShowSprintModeSelectionButtonImpl`) is hold or
/// toggle.
pub fn wants_sprint(auto: bool, toggle: bool, held: bool, pressed: bool, toggled: &mut bool) -> bool {
    if toggle {
        if pressed {
            *toggled = !*toggled;
        }
        *toggled != auto
    } else {
        held != auto
    }
}

impl Motor {
    pub fn new() -> Self {
        Motor { height: 1.0, ..Default::default() }
    }

    /// The horizontal velocity the input asks for.
    pub fn target(&mut self, t: &Tuning, i: &Input) -> Vec3 {
        let wish = Vec3::new(i.wish.x, 0.0, i.wish.z);
        let crouched = self.height < 0.999 || i.crouch;
        let along = wish.dot(i.forward);
        let cancel = t.sprint_cancel_deg.to_radians().cos();
        self.sprinting = i.sprint && !crouched && wish.length() > 0.1 && along >= cancel * wish.length();
        if self.sprinting {
            // forward at sprint speed, sideways held to a part of it
            let max = t.walk_speed * t.sprint_mult;
            let side = wish - i.forward * along;
            return i.forward * along * max + side * max * t.sprint_lateral;
        }
        wish * t.walk_speed * if crouched { t.crouch_speed } else { 1.0 }
    }

    /// Updates the velocity and height for one frame of `dt` seconds.
    pub fn step(&mut self, t: &Tuning, i: &Input, dt: f32) {
        if self.flying {
            return self.fly(t, i, dt);
        }
        // crouch: the height eases toward its goal; standing up waits for room
        let goal = if i.crouch || (!i.head_room && self.height < 1.0) { t.crouch_height } else { 1.0 };
        self.height += (goal - self.height).clamp(-t.height_change_speed * dt, t.height_change_speed * dt);

        let target = self.target(t, i);
        let flat = Vec3::new(self.velocity.x, 0.0, self.velocity.z);
        let accel_time = if self.sprinting { t.sprint_accel_time } else { t.walk_accel_time };
        let max = target.length().max(t.walk_speed);
        let accel = if accel_time > 0.0 { max / accel_time } else { f32::MAX };
        let flat = if self.grounded {
            toward(flat, target, accel * dt)
        } else if target.length() > 1e-3 {
            toward(flat, target, accel * t.air_control * dt)
        } else {
            toward(flat, Vec3::ZERO, t.air_drag * dt)
        };

        if self.grounded {
            self.air_time = 0.0;
        } else {
            self.air_time += dt;
        }
        let mut vy = self.velocity.y;
        if i.jump && !self.jumping && (self.grounded || self.air_time <= t.coyote) {
            vy = jump_speed(t.gravity, t.jump_height);
            (self.jumping, self.grounded, self.air_time) = (true, false, t.coyote + dt);
        }
        // letting go early cuts the rise to what half the height needs
        let cut = jump_speed(t.gravity, t.jump_height * t.jump_cutoff_mult);
        if self.jumping && !i.jump_held && vy > cut {
            vy = cut;
        }
        // always pulled down, so the ground stays touched (`after_move` stops it there)
        vy = (vy - t.gravity * dt).max(-t.fall_terminal);
        if vy <= 0.0 {
            self.jumping = false;
        }
        self.velocity = Vec3::new(flat.x, vy, flat.z);
    }

    fn fly(&mut self, t: &Tuning, i: &Input, dt: f32) {
        self.height += (1.0 - self.height).min(t.height_change_speed * dt);
        self.sprinting = false;
        let wish = i.wish.clamp_length_max(1.0);
        if wish.length() > 1e-3 {
            self.fly_time += dt;
            let speed = t.fly_speed * t.fly_curve.eval((self.fly_time / t.fly_curve_time).min(1.0));
            self.velocity = toward(self.velocity, wish * speed, speed.max(t.walk_speed) / t.fly_accel_time * dt);
        } else {
            self.fly_time = 0.0;
            self.velocity = toward(self.velocity, Vec3::ZERO, t.fly_speed / t.fly_brake_time * dt);
        }
    }

    /// After the body moved by `moved` (asked: `velocity * dt`): stands on the ground, stops at ceilings, and does not
    /// keep speed into walls.
    pub fn after_move(&mut self, grounded: bool, moved: Vec3, dt: f32) {
        let asked = self.velocity * dt;
        if self.flying {
            self.grounded = false;
            return;
        }
        self.grounded = grounded;
        if grounded && self.velocity.y < 0.0 {
            self.velocity.y = 0.0;
        }
        // a head bump ends the rise
        if self.velocity.y > 0.0 && moved.y < asked.y - 1e-4 {
            (self.velocity.y, self.jumping) = (0.0, false);
        }
        let (want, got) = (Vec2::new(asked.x, asked.z), Vec2::new(moved.x, moved.z));
        if dt > 0.0 && got.length() < want.length() - 1e-5 {
            // keep only the part of the speed the walls let through
            let kept = if want.length() > 1e-6 { got.dot(want) / want.length_squared() } else { 0.0 };
            let f = kept.clamp(0.0, 1.0);
            let slide = got / dt;
            let v = Vec2::new(self.velocity.x, self.velocity.z) * f;
            let v = if slide.length() > v.length() { slide.clamp_length_max(Vec2::new(self.velocity.x, self.velocity.z).length()) } else { v };
            (self.velocity.x, self.velocity.z) = (v.x, v.y);
        }
    }

    pub fn toggle_fly(&mut self) {
        self.flying = !self.flying;
        (self.velocity, self.fly_time, self.jumping) = (Vec3::ZERO, 0.0, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 120.0;

    fn forward() -> Input {
        Input { wish: Vec3::NEG_Z, forward: Vec3::NEG_Z, head_room: true, ..Default::default() }
    }

    /// Runs on flat ground at y = 0 for `secs`; returns the motor and the feet.
    fn run(m: &mut Motor, feet: &mut Vec3, secs: f32, i: Input) {
        let t = Tuning::default();
        let mut i = i;
        for _ in 0..(secs / DT).round() as usize {
            m.step(&t, &i, DT);
            let mut next = *feet + m.velocity * DT;
            let grounded = next.y <= 0.0;
            next.y = next.y.max(0.0);
            m.after_move(grounded, next - *feet, DT);
            *feet = next;
            i.jump = false;
        }
    }

    fn grounded() -> Motor {
        Motor { grounded: true, ..Motor::new() }
    }

    #[test]
    fn walks_at_three_metres_a_second_after_the_acceleration_time() {
        let (mut m, mut p) = (grounded(), Vec3::ZERO);
        run(&mut m, &mut p, 0.075, forward());
        // half way through 0.15 s: half the speed
        assert!((m.velocity.length() - 1.5).abs() < 0.05, "{}", m.velocity);
        run(&mut m, &mut p, 0.1, forward());
        assert!((m.velocity.length() - 3.0).abs() < 1e-4, "{}", m.velocity);
        // and stops in the same time
        run(&mut m, &mut p, 0.16, Input { wish: Vec3::ZERO, ..forward() });
        assert_eq!(m.velocity, Vec3::ZERO);
    }

    #[test]
    fn sprints_forward_only() {
        let (mut m, mut p) = (grounded(), Vec3::ZERO);
        run(&mut m, &mut p, 0.5, Input { sprint: true, ..forward() });
        assert!(m.sprinting && (m.velocity.length() - 4.5).abs() < 1e-4, "{}", m.velocity);
        // backwards cancels the sprint: walking speed
        run(&mut m, &mut p, 0.6, Input { sprint: true, wish: Vec3::Z, ..forward() });
        assert!(!m.sprinting && (m.velocity.length() - 3.0).abs() < 1e-4, "{}", m.velocity);
        // diagonal: sideways part held to 0.4 of the sprint speed
        let d = Vec3::new(1.0, 0.0, -1.0).normalize();
        run(&mut m, &mut p, 0.6, Input { sprint: true, wish: d, ..forward() });
        assert!((m.velocity.z + d.x * 4.5).abs() < 1e-3 && (m.velocity.x - d.x * 4.5 * 0.4).abs() < 1e-3, "{}", m.velocity);
    }

    #[test]
    fn jump_reaches_its_height_and_a_tap_reaches_half() {
        let t = Tuning::default();
        assert!((jump_speed(t.gravity, t.jump_height) - 3.836).abs() < 1e-3);
        let (mut m, mut p) = (grounded(), Vec3::ZERO);
        let mut top = 0f32;
        run(&mut m, &mut p, DT, Input { jump: true, jump_held: true, wish: Vec3::ZERO, ..forward() });
        for _ in 0..120 {
            run(&mut m, &mut p, DT, Input { jump_held: true, wish: Vec3::ZERO, ..forward() });
            top = top.max(p.y);
        }
        assert!((top - 0.75).abs() < 0.03, "{top}");
        assert!(m.grounded && p.y == 0.0);
        // let go at once: the rise is cut to half the height
        let mut top = 0f32;
        run(&mut m, &mut p, DT, Input { jump: true, jump_held: true, wish: Vec3::ZERO, ..forward() });
        for _ in 0..120 {
            run(&mut m, &mut p, DT, Input { wish: Vec3::ZERO, ..forward() });
            top = top.max(p.y);
        }
        assert!((top - 0.375).abs() < 0.03, "{top}");
    }

    #[test]
    fn coyote_time_allows_a_late_jump_once() {
        let t = Tuning::default();
        let mut m = Motor { grounded: false, air_time: t.coyote - 0.02, ..Motor::new() };
        m.step(&t, &Input { jump: true, jump_held: true, ..forward() }, DT);
        assert!(m.velocity.y > 3.0);
        // no second jump in the air
        let vy = m.velocity.y;
        m.step(&t, &Input { jump: true, jump_held: true, ..forward() }, DT);
        assert!(m.velocity.y < vy);
        let mut late = Motor { grounded: false, air_time: t.coyote + 0.02, ..Motor::new() };
        late.step(&t, &Input { jump: true, jump_held: true, ..forward() }, DT);
        assert!(late.velocity.y <= 0.0);
    }

    #[test]
    fn falls_to_terminal_speed_and_keeps_half_control_in_the_air() {
        let t = Tuning::default();
        let mut m = Motor::new();
        for _ in 0..600 {
            m.step(&t, &Input { wish: Vec3::ZERO, ..forward() }, DT);
        }
        assert_eq!(m.velocity.y, -20.0);
        // air control: half the ground acceleration
        let mut m = Motor::new();
        m.step(&t, &forward(), 0.05);
        assert!((m.velocity.z + 0.5).abs() < 1e-4, "{}", m.velocity);
        // no input in the air: drag of 2 m/s^2
        let mut m = Motor { velocity: Vec3::new(3.0, 0.0, 0.0), ..Motor::new() };
        m.step(&t, &Input { wish: Vec3::ZERO, ..forward() }, 0.5);
        assert!((m.velocity.x - 2.0).abs() < 1e-4, "{}", m.velocity);
    }

    #[test]
    fn crouch_lowers_and_slows_and_waits_for_room_to_stand() {
        let (mut m, mut p) = (grounded(), Vec3::ZERO);
        run(&mut m, &mut p, 1.0, Input { crouch: true, ..forward() });
        assert!((m.height - 0.725).abs() < 1e-5 && (m.velocity.length() - 2.25).abs() < 1e-4, "{} {}", m.height, m.velocity);
        // no head room: stays down
        run(&mut m, &mut p, 1.0, Input { head_room: false, ..forward() });
        assert!((m.height - 0.725).abs() < 1e-5);
        // room again: back up at the height change speed (0.275 takes 0.2 s)
        run(&mut m, &mut p, 0.1, forward());
        assert!(m.height > 0.8 && m.height < 1.0, "{}", m.height);
        run(&mut m, &mut p, 0.15, forward());
        assert_eq!(m.height, 1.0);
    }

    #[test]
    fn walls_take_the_speed_and_ceilings_end_the_jump() {
        let mut m = Motor { velocity: Vec3::new(3.0, 2.0, 0.0), ..Motor::new() };
        m.after_move(false, Vec3::new(0.0, 0.0, 0.0), 0.01);
        assert_eq!(m.velocity, Vec3::ZERO);
        // sliding along a wall keeps the part along it
        let mut m = Motor { grounded: true, velocity: Vec3::new(3.0, 0.0, -3.0), ..Motor::new() };
        m.after_move(true, Vec3::new(0.0, 0.0, -0.03), 0.01);
        assert!((m.velocity.z + 3.0).abs() < 1e-4 && m.velocity.x.abs() < 1e-4, "{}", m.velocity);
    }

    #[test]
    fn flying_ramps_up_along_the_curve() {
        let t = Tuning::default();
        let mut m = Motor::new();
        m.toggle_fly();
        for _ in 0..12 {
            m.step(&t, &forward(), 0.01);
        }
        // the curve starts at 0.15 of 35
        assert!(m.velocity.length() > 4.0 && m.velocity.length() < 8.0, "{}", m.velocity);
        for _ in 0..300 {
            m.step(&t, &forward(), 0.01);
        }
        assert!((m.velocity.length() - 35.0).abs() < 1e-3);
        for _ in 0..20 {
            m.step(&t, &Input { wish: Vec3::ZERO, ..forward() }, 0.01);
        }
        assert_eq!(m.velocity, Vec3::ZERO);
    }

    #[test]
    fn sprint_settings() {
        let mut toggled = false;
        assert!(wants_sprint(false, false, true, false, &mut toggled));
        assert!(!wants_sprint(true, false, true, false, &mut toggled));
        assert!(wants_sprint(true, false, false, false, &mut toggled));
        assert!(wants_sprint(false, true, false, true, &mut toggled));
        assert!(wants_sprint(false, true, false, false, &mut toggled));
        assert!(!wants_sprint(false, true, true, true, &mut toggled));
    }
}
