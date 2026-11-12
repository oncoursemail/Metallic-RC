//! DiscGolf, ported from the decompiled `DiscGolfManager : GameRuleManager` (@dump.cs:2570), VERIFIED against the
//! real binary (decompile lane, sweet-galileo_discgolf-recon.md). A third distinct shape that stress-tests the seam:
//! per-player STROKE play over a multi-hole course, not team elimination (Dodgeball) or team goals (Paddleball). Each
//! player throws a disc at each basket; a throw is +1 stroke; completing a hole records (par − strokes) toward the
//! player's running total; the player with the best total (most under par, i.e. lowest strokes relative to par) wins
//! when the course is done. There is NO fixed score-to-win — it ends when every player has finished all holes.
//!
//! Net flow (same master-validator shape): the client raises `RpcMasterFinishHole` on a basket collision and a throw
//! input per release; the master validates, updates the authoritative scoreboard, and broadcasts. Out-of-bounds is a
//! game-specific HAZARD (penalty stroke + return to tee), not the base killzone. The disc's FLIGHT MODEL (airLift,
//! throw-assist curve, spin) is a client-side prop behaviour — not in these rules.

use std::collections::HashMap;

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, Team};

/// A DiscGolf course: par per hole, read from the scene's `DiscGolfGoal` objects (each has hole# + par). The total par
/// is their sum; the course length is the number of baskets.
#[derive(Clone, Debug)]
pub struct DiscGolfConfig {
    pub course: Vec<u32>,
}

impl Default for DiscGolfConfig {
    fn default() -> Self {
        // The REAL level9 course, read from the scene's 9 `DiscGolfGoal` objects via `unity_assets::level::MonoFields`
        // (hole:i32, par:i32), ordered by hole 0..=8 — pars [3,2,3,4,2,4,3,2,5], total par 28. Validated by the maps
        // lane (18/18 DiscGolfGoal across level9+level10 read with sane hole/par). No longer a stand-in.
        // (level10 is the alternate 9-hole course variant; load it the same way for that room.)
        DiscGolfConfig { course: vec![3, 2, 3, 4, 2, 4, 3, 2, 5] }
    }
}

struct PlayerCard {
    hole: usize,  // the hole this player is on
    strokes: u32, // strokes taken on the current hole
    total: i32,   // sum of (par − strokes) over completed holes (higher = more under par = better)
    done: bool,   // finished the whole course
}

/// DiscGolf as a [`GameMode`] on the room authority: pure validator + per-player scorer. Throw = +stroke, Hazard =
/// +penalty stroke, Basket = finish the current hole (score par−strokes, advance). The match ends when all players
/// finish; the highest total (most under par) wins.
pub struct DiscGolfMode {
    course: Vec<u32>,
    players: HashMap<PlayerId, PlayerCard>,
    order: Vec<PlayerId>,
    over: bool,
}

impl DiscGolfMode {
    pub fn new(config: DiscGolfConfig) -> Self {
        DiscGolfMode { course: config.course, players: HashMap::new(), order: Vec::new(), over: false }
    }

    /// If every player has finished the course, emit the winner once (highest total; ties broken by join order).
    fn check_course_complete(&mut self, out: &mut Vec<GameEvent>) {
        if self.over || self.players.is_empty() || !self.players.values().all(|p| p.done) {
            return;
        }
        self.over = true;
        // 1:1: the exact tie-break (fewest strokes? first to finish?) isn't modelled — highest total, then join order.
        let winner = self.order.iter().copied().max_by_key(|id| self.players.get(id).map(|p| p.total).unwrap_or(i32::MIN));
        if let Some(w) = winner {
            out.push(GameEvent::MatchOver { winner: w });
        }
    }
}

impl GameMode for DiscGolfMode {
    fn name(&self) -> &'static str {
        "DiscGolf"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        self.players.insert(id, PlayerCard { hole: 0, strokes: 0, total: 0, done: false });
        self.order.push(id);
        0 // free-for-all; teams unused (per-player scoring)
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.remove(&id);
        self.order.retain(|&p| p != id);
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        match input {
            GameInput::Throw { player } | GameInput::Hazard { player } => {
                // a throw is a stroke; a hazard is a penalty stroke — both add one to the current hole
                if let Some(p) = self.players.get_mut(&player) {
                    if !p.done {
                        p.strokes += 1;
                    }
                }
            }
            GameInput::Basket { player } => {
                let finished = if let Some(p) = self.players.get_mut(&player) {
                    if p.done || p.hole >= self.course.len() {
                        None
                    } else {
                        // the throw that landed was already counted by its Throw input; the basket just finalises the
                        // hole at (par − strokes taken)
                        let par = self.course[p.hole] as i32;
                        p.total += par - p.strokes as i32;
                        p.hole += 1;
                        p.strokes = 0;
                        if p.hole >= self.course.len() {
                            p.done = true;
                        }
                        Some(p.total)
                    }
                } else {
                    None
                };
                if let Some(total) = finished {
                    out.push(GameEvent::PlayerScore { player, score: total });
                    self.check_course_complete(out);
                }
            }
            _ => {} // Hit/Catch/Goal are other games' inputs
        }
    }

    fn tick(&mut self, _dt: f32, _out: &mut Vec<GameEvent>) {
        // untimed: holes complete on baskets, the match on course completion (no round clock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_player_stroke_play_lowest_relative_to_par_wins() {
        // a 2-hole course, par 3 each
        let mut m = DiscGolfMode::new(DiscGolfConfig { course: vec![3, 3] });
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();

        // player 1: hole 0 in 2 throws (birdie, +1), hole 1 in 3 (par, 0) -> total +1
        m.input(GameInput::Throw { player: 1 }, &mut out);
        m.input(GameInput::Throw { player: 1 }, &mut out);
        m.input(GameInput::Basket { player: 1 }, &mut out); // 2 strokes on hole 0
        m.input(GameInput::Throw { player: 1 }, &mut out);
        m.input(GameInput::Throw { player: 1 }, &mut out);
        m.input(GameInput::Throw { player: 1 }, &mut out);
        m.input(GameInput::Basket { player: 1 }, &mut out); // 3 strokes on hole 1

        // player 2: hole 0 in 4 (bogey, -1, incl. a hazard penalty), hole 1 in 3 (par, 0) -> total -1
        m.input(GameInput::Throw { player: 2 }, &mut out);
        m.input(GameInput::Hazard { player: 2 }, &mut out); // penalty stroke
        m.input(GameInput::Throw { player: 2 }, &mut out);
        m.input(GameInput::Throw { player: 2 }, &mut out);
        m.input(GameInput::Basket { player: 2 }, &mut out); // 4 strokes on hole 0
        m.input(GameInput::Throw { player: 2 }, &mut out);
        m.input(GameInput::Throw { player: 2 }, &mut out);
        m.input(GameInput::Throw { player: 2 }, &mut out);
        m.input(GameInput::Basket { player: 2 }, &mut out); // 3 strokes on hole 1

        let winner = out.iter().rev().find_map(|e| if let GameEvent::MatchOver { winner } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(1), "player 1 (+1 under par) beats player 2 (-1)");
        // the last score for player 1 should be +1, player 2 -1
        let p1 = out.iter().rev().find_map(|e| if let GameEvent::PlayerScore { player: 1, score } = e { Some(*score) } else { None });
        let p2 = out.iter().rev().find_map(|e| if let GameEvent::PlayerScore { player: 2, score } = e { Some(*score) } else { None });
        assert_eq!((p1, p2), (Some(1), Some(-1)));
    }
}
