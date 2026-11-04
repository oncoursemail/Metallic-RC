//! The avatar's animation state, without Bevy: which way it moves (idle, walk, run, jump, fall, fly) from its velocity,
//! a stride phase, blinking and talking, and the body pose that follows from them in the avatar's own space (+y up,
//! facing -z, feet at the origin).
use bevy::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Move {
    #[default]
    Idle,
    Walk,
    Run,
    Jump,
    Fall,
    Fly,
}

/// What drives the animation this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// world velocity, m/s
    pub velocity: Vec3,
    /// known for the local player; for others it is guessed from the vertical speed
    pub grounded: Option<bool>,
    pub flying: bool,
    pub talking: bool,
}

/// below this the avatar stands
const STAND: f32 = 0.3;
/// above this it runs (walk is 3 m/s, sprint 4.5)
const RUN: f32 = 3.75;
/// how long it must be off the ground before it jumps or falls, so steps and stairs do not flicker
const AIR: f32 = 0.12;
/// a remote avatar moving up or down faster than this is taken to be in the air
const AIR_SPEED: f32 = 0.6;
const BLINK: f32 = 0.14;

#[derive(Clone, Debug, Default)]
pub struct Anim {
    pub state: Move,
    /// stride phase, radians; one turn is two steps
    pub phase: f32,
    /// smoothed ground speed, m/s
    pub speed: f32,
    time: f32,
    air: f32,
    /// seconds to the next blink, and how far into the current one
    next_blink: f32,
    blink: Option<f32>,
    seed: u32,
    mouth: f32,
}

/// The body pose in the avatar's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// up and down of everything above the hips, metres
    pub bob: f32,
    /// forward lean of the torso, radians
    pub lean: f32,
    /// leg swing about the hip, radians (positive forward), left then right
    pub legs: [f32; 2],
    /// where the hands are when nobody tells us (no synced hand poses)
    pub hands: [Vec3; 2],
    /// 1 open, 0 shut
    pub eyes: f32,
    /// 0 shut, 1 wide open
    pub mouth: f32,
}

impl Anim {
    pub fn new(seed: u32) -> Self {
        Anim { seed: seed | 1, next_blink: 1.0 + (seed % 7) as f32 * 0.4, ..Default::default() }
    }

    fn random(&mut self) -> f32 {
        // xorshift: blink intervals that differ per avatar and repeat in tests
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 1000) as f32 / 1000.0
    }

    pub fn step(&mut self, dt: f32, i: Input) {
        self.time += dt;
        let ground = Vec3::new(i.velocity.x, 0.0, i.velocity.z).length();
        self.speed += (ground - self.speed) * (1.0 - (-dt * 10.0).exp());
        let airborne = i.grounded.map_or(i.velocity.y.abs() > AIR_SPEED, |g| !g);
        self.air = if airborne { self.air + dt } else { 0.0 };
        self.state = if i.flying {
            Move::Fly
        } else if self.air > AIR {
            if i.velocity.y > 0.0 { Move::Jump } else { Move::Fall }
        } else if self.speed < STAND {
            Move::Idle
        } else if self.speed < RUN {
            Move::Walk
        } else {
            Move::Run
        };
        // a stride (two steps) is longer the faster it goes
        if matches!(self.state, Move::Walk | Move::Run) {
            self.phase = (self.phase + dt * self.speed / (1.0 + 0.15 * self.speed) * std::f32::consts::TAU) % std::f32::consts::TAU;
        } else {
            // the legs come back together by the shorter way
            let back = if self.phase > std::f32::consts::PI { self.phase - std::f32::consts::TAU } else { self.phase };
            self.phase = (back * (-dt * 6.0).exp()).rem_euclid(std::f32::consts::TAU);
        }
        match self.blink {
            Some(t) if t + dt >= BLINK => {
                self.blink = None;
                self.next_blink = 2.0 + 3.5 * self.random();
            }
            Some(t) => self.blink = Some(t + dt),
            None => {
                self.next_blink -= dt;
                if self.next_blink <= 0.0 {
                    self.blink = Some(0.0);
                }
            }
        }
        // talking opens and closes the mouth at syllable pace, with some unevenness
        let target = if i.talking { 0.25 + 0.75 * ((self.time * 11.0).sin() * (self.time * 4.3).sin()).abs() } else { 0.0 };
        self.mouth += (target - self.mouth) * (1.0 - (-dt * 25.0).exp());
    }

    pub fn pose(&self) -> Pose {
        let (s, idle) = (self.phase.sin(), (self.time * 2.1).sin());
        let rest = [Vec3::new(-0.28, 0.82, -0.12), Vec3::new(0.28, 0.82, -0.12)];
        let walking = matches!(self.state, Move::Walk | Move::Run) || self.phase.sin().abs() > 1e-3;
        let (bob, lean, legs, hands) = match self.state {
            Move::Jump => (0.0, -0.05, [0.55, -0.25], rest.map(|h| h + Vec3::new(0.0, 0.32, 0.02))),
            Move::Fall => (0.0, 0.0, [0.25, 0.1], [Vec3::new(-0.42, 1.25, 0.0), Vec3::new(0.42, 1.25, 0.0)]),
            Move::Fly => (0.03 * idle, 0.3, [-0.35 + 0.08 * idle, -0.3 - 0.08 * idle], rest.map(|h| h + Vec3::new(0.0, 0.05, 0.12))),
            _ if walking => {
                let run = self.state == Move::Run;
                let amp = if run { 0.7 } else { 0.45 } * (self.speed / 2.0).clamp(0.3, 1.0);
                let legs = [-s * amp, s * amp];
                // the hips dip as the legs spread (legs 0.6 m long), and the hands swing against the legs
                let bob = -(1.0 - legs[0].cos()) * 0.6;
                let arm = if run { 0.22 } else { 0.14 };
                let hands = [rest[0] + Vec3::new(0.0, bob, -s * arm), rest[1] + Vec3::new(0.0, bob, s * arm)];
                (bob, if run { 0.16 } else { 0.05 }, legs, hands)
            }
            _ => (0.012 * idle, 0.0, [0.0, 0.0], rest.map(|h| h + Vec3::Y * 0.01 * idle)),
        };
        let eyes = match self.blink {
            // shut by the middle of the blink, open again by its end
            Some(b) => (2.0 * b / BLINK - 1.0).abs(),
            None => 1.0,
        };
        Pose { bob, lean, legs, hands, eyes, mouth: self.mouth }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(a: &mut Anim, secs: f32, i: Input) {
        for _ in 0..(secs * 60.0) as usize {
            a.step(1.0 / 60.0, i);
        }
    }

    fn going(x: f32, y: f32) -> Input {
        Input { velocity: Vec3::new(x, y, 0.0), ..Default::default() }
    }

    #[test]
    fn velocity_picks_the_move() {
        let mut a = Anim::new(7);
        run(&mut a, 0.5, going(0.0, 0.0));
        assert_eq!(a.state, Move::Idle);
        run(&mut a, 0.5, going(3.0, 0.0));
        assert_eq!(a.state, Move::Walk);
        run(&mut a, 0.5, going(4.5, 0.0));
        assert_eq!(a.state, Move::Run);
        // a remote avatar going up fast is jumping, coming down fast is falling
        run(&mut a, 0.3, going(4.5, 3.8));
        assert_eq!(a.state, Move::Jump);
        run(&mut a, 0.3, going(4.5, -3.0));
        assert_eq!(a.state, Move::Fall);
        run(&mut a, 0.5, going(0.0, 0.0));
        assert_eq!(a.state, Move::Idle);
        run(&mut a, 0.1, Input { flying: true, ..going(0.0, 5.0) });
        assert_eq!(a.state, Move::Fly);
    }

    #[test]
    fn short_air_time_and_known_ground_do_not_jump() {
        let mut a = Anim::new(1);
        run(&mut a, 0.5, going(3.0, 0.0));
        // a step up: a brief vertical burst stays a walk
        run(&mut a, 0.08, going(3.0, 2.0));
        assert_eq!(a.state, Move::Walk);
        // the local player says it is on the ground: the vertical speed does not matter
        run(&mut a, 0.5, Input { grounded: Some(true), ..going(3.0, 2.0) });
        assert_eq!(a.state, Move::Walk);
        run(&mut a, 0.2, Input { grounded: Some(false), ..going(3.0, 0.0) });
        assert_eq!(a.state, Move::Fall);
    }

    #[test]
    fn walking_swings_legs_and_hands_against_each_other() {
        let mut a = Anim::new(3);
        let still = a.pose();
        assert_eq!(still.legs, [0.0, 0.0]);
        run(&mut a, 0.6, going(3.0, 0.0));
        let mut widest = 0f32;
        for _ in 0..60 {
            a.step(1.0 / 60.0, going(3.0, 0.0));
            let p = a.pose();
            assert!((p.legs[0] + p.legs[1]).abs() < 1e-6);
            // the left hand goes forward (-z) as the left leg goes back
            assert!((p.hands[0].z - still.hands[0].z) * p.legs[0] >= -1e-6);
            assert!(p.bob <= 1e-6);
            widest = widest.max(p.legs[0].abs());
        }
        assert!(widest > 0.3, "{widest}");
        // stopping brings the feet together
        run(&mut a, 1.5, going(0.0, 0.0));
        assert!(a.pose().legs[0].abs() < 0.01, "{:?}", a.pose().legs);
    }

    #[test]
    fn blinks_now_and_then_and_talks_on_cue() {
        let mut a = Anim::new(11);
        let (mut blinks, mut was_open) = (0, true);
        for _ in 0..(20 * 60) {
            a.step(1.0 / 60.0, Input::default());
            let open = a.pose().eyes > 0.5;
            if was_open && !open {
                blinks += 1;
            }
            was_open = open;
        }
        // one blink every 2 to 5.5 s
        assert!((3..=10).contains(&blinks), "{blinks} blinks in 20 s");
        assert_eq!(a.pose().mouth, 0.0);
        let mut widest = 0f32;
        for _ in 0..60 {
            a.step(1.0 / 60.0, Input { talking: true, ..Default::default() });
            widest = widest.max(a.pose().mouth);
        }
        assert!(widest > 0.6, "{widest}");
        run(&mut a, 0.5, Input::default());
        assert!(a.pose().mouth < 0.01);
    }
}
