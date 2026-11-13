//! Test bots: server-side players that walk and shoot, so one person can try a match. The server has no map
//! geometry, so a bot walks in straight lines on the ground between the spawns and flags (its height follows the
//! points it walks between) and shoots the nearest enemy in sight range through walls. All numbers INFERRED (ours):
//! walk 3 m/s (the player's walk speed), sight 30 m, a shot every 0.7-1.2 s, aim spread about 2 degrees, shots fly
//! 1.5 s at most.
use rr_net::games::Mode;

use super::paintball::{add, flat, len, scale, sub, Match, GRAVITY, GUN_SPEED, V};

pub const WALK: f32 = 3.0;
pub const SIGHT: f32 = 30.0;
pub const SHOT_LIFE: f32 = 1.5;
const FIRE_EVERY: f64 = 0.7;
const SPREAD: f32 = 0.035;
/// close enough to an enemy to stop walking and side-step
const FIGHT: f32 = 14.0;

/// What a bot knows this step.
pub struct View {
    pub team: u8,
    pub feet: V,
    pub mode: Mode,
    pub enemies: Vec<V>,
    /// where it wants to go
    pub goal: Option<V>,
    /// bounds of the field, if known
    pub bounds: Option<(V, V)>,
    pub last_fire: f64,
}

impl View {
    pub fn of(m: &Match, id: i32) -> Option<View> {
        let team = m.team_of(id)?;
        let feet = m.position(id)?;
        let enemies = m.standing().into_iter().filter(|e| e.1 != team).map(|e| e.2).collect();
        let map = m.map();
        let enemy = 1 - team;
        let goal = map.and_then(|map| match m.mode() {
            Mode::CaptureTheFlag => {
                let theirs = m.flag(enemy);
                let ours = m.flag(team);
                Some(if theirs.carrier == id || (theirs.carrier != 0 && m.team_of(theirs.carrier) == Some(team)) {
                    map.goals[team as usize]
                } else if ours.carrier == 0 && !ours.home {
                    ours.at
                } else {
                    theirs.at
                })
            }
            Mode::TeamBattle => {
                let s = &map.spawns[enemy as usize];
                (!s.is_empty()).then(|| scale(s.iter().fold([0.0; 3], |a, s| add(a, s.pos)), 1.0 / s.len() as f32))
            }
        });
        Some(View { team, feet, mode: m.mode(), enemies, goal, bounds: map.and_then(|m| m.bounds), last_fire: m.last_fire(id) })
    }
}

/// What a bot does this step.
pub struct Act {
    pub feet: V,
    pub velocity: V,
    pub yaw: f32,
    /// a shot: origin and velocity
    pub fire: Option<(V, V)>,
}

pub struct Brain {
    next_fire: f64,
    strafe: f32,
    switch_at: f64,
    wander: V,
}

/// Yaw (radians about +Y, 0 looks along -Z) facing `d`.
fn yaw_of(d: V) -> f32 {
    (-d[0]).atan2(-d[2])
}

/// The launch direction that drops onto `to` from `from` at `speed` (the flat arc), or straight at it.
pub fn aim(from: V, to: V, speed: f32) -> V {
    let d = sub(to, from);
    let h = flat(to, from);
    let flat_dir = if h > 1e-4 { [d[0] / h, 0.0, d[2] / h] } else { [0.0, 0.0, -1.0] };
    let (v2, g) = (speed * speed, GRAVITY);
    let disc = v2 * v2 - g * (g * h * h + 2.0 * d[1] * v2);
    if h < 1e-4 || disc < 0.0 {
        let l = len(d).max(1e-4);
        return scale(d, 1.0 / l);
    }
    let angle = ((v2 - disc.sqrt()) / (g * h)).atan();
    add(scale(flat_dir, angle.cos()), [0.0, angle.sin(), 0.0])
}

impl Brain {
    pub fn new(now: f64, id: i32) -> Brain {
        Brain { next_fire: now + 1.0, strafe: if id % 2 == 0 { 1.0 } else { -1.0 }, switch_at: now, wander: [0.0; 3] }
    }

    /// `r`: two random numbers in 0..1.
    pub fn think(&mut self, now: f64, dt: f32, v: &View, r: [f32; 2]) -> Act {
        let target = v.enemies.iter().copied().filter(|e| len(sub(*e, v.feet)) <= SIGHT).min_by(|a, b| len(sub(*a, v.feet)).total_cmp(&len(sub(*b, v.feet))));
        if now >= self.switch_at {
            self.switch_at = now + 1.5;
            self.strafe = -self.strafe;
        }
        let mut velocity = [0.0; 3];
        let mut feet = v.feet;
        let fighting = target.is_some_and(|t| flat(t, v.feet) < FIGHT) && v.mode == Mode::TeamBattle;
        if let Some(goal) = v.goal.map(|g| add(g, self.wander)) {
            let d = sub(goal, v.feet);
            let dist = flat(goal, v.feet);
            if fighting {
                let t = target.unwrap();
                let to = sub(t, v.feet);
                let l = flat(t, v.feet).max(1e-3);
                velocity = [to[2] / l * self.strafe * WALK * 0.6, 0.0, -to[0] / l * self.strafe * WALK * 0.6];
            } else if dist > 1.0 {
                velocity = [d[0] / dist * WALK, 0.0, d[2] / dist * WALK];
                // the height follows the line to the goal
                feet[1] += d[1] * (WALK * dt / dist).min(1.0);
            } else if v.mode == Mode::TeamBattle {
                self.wander = [(r[0] - 0.5) * 20.0, 0.0, (r[1] - 0.5) * 20.0];
            }
        }
        feet = add(feet, scale(velocity, dt));
        if let Some((lo, hi)) = v.bounds {
            feet[0] = feet[0].clamp(lo[0], hi[0]);
            feet[2] = feet[2].clamp(lo[2], hi[2]);
        }
        let yaw = match target {
            Some(t) => yaw_of(sub(t, v.feet)),
            None if len(velocity) > 0.1 => yaw_of(velocity),
            None => yaw_of([0.0, 0.0, -1.0]),
        };
        let mut fire = None;
        if let Some(t) = target.filter(|_| now >= self.next_fire && now - v.last_fire >= FIRE_EVERY) {
            self.next_fire = now + FIRE_EVERY + r[0] as f64 * 0.5;
            let origin = add(feet, [0.0, 1.4, 0.0]);
            let dir = aim(origin, add(t, [0.0, 1.0, 0.0]), GUN_SPEED);
            let jitter = [(r[0] - 0.5) * 2.0 * SPREAD, (r[1] - 0.5) * 2.0 * SPREAD, (r[0] * r[1] - 0.25) * 2.0 * SPREAD];
            let dir = add(dir, jitter);
            fire = Some((add(origin, scale(dir, 0.4)), scale(dir, GUN_SPEED / len(dir))));
        }
        Act { feet, velocity, yaw, fire }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_drops_onto_the_target() {
        let (from, to) = ([0.0, 1.4, 0.0], [0.0, 1.0, -25.0]);
        let d = aim(from, to, GUN_SPEED);
        let (mut p, mut vel) = (from, scale(d, GUN_SPEED));
        for _ in 0..600 {
            vel[1] -= GRAVITY / 600.0;
            p = add(p, scale(vel, 1.0 / 600.0));
            if p[2] <= -25.0 {
                break;
            }
        }
        assert!((p[1] - 1.0).abs() < 0.05, "arrived at height {}", p[1]);
        assert!((yaw_of([0.0, 0.0, -1.0])).abs() < 1e-6);
    }
}
