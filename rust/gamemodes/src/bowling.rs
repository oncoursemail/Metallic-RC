//! Bowling, ported from the decompiled `BowlingManager : GameRuleManager` + `BowlingScoreboard` (scene level91),
//! VERIFIED against the real binary (decompile lane, sweet-galileo_bowling-recon.md). A per-player, turn-based game
//! like DiscGolf, but with the REAL 10-pin frame-scoring algorithm (not a simple stroke sum): a strike (10 on the
//! first bowl) scores 10 + the next TWO bowls; a spare (two bowls summing to 10) scores 10 + the next ONE bowl; an
//! open frame scores its pins; the 10th frame grants up to 3 bowls (strike/spare earns the bonus bowls in-frame). A
//! frame whose bonus isn't bowled yet is pending (shown blank/-1 on the real scoreboard). Highest cumulative total
//! after 10 frames wins — there is no score-to-win threshold (10 frames is hardcoded; the wrapper carries no frame
//! config).
//!
//! Flow (NOT the Soccer/Dodgeball request-vote pattern): a bowl settles, the lane counts standing pins by PHYSICS
//! (pins knocked = standing before − standing after), and the pinfall is recorded as a [`GameInput::Bowl`]; the master
//! owns the authoritative scoreboard, recomputing the whole card with this deterministic algorithm (the real
//! `BowlingScoreboard.Refresh`). Pin knockdown and the ball's roll are client-side physics, not in these rules.

use std::collections::HashMap;

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, Team};

/// Bowling config. Frames are hardcoded to 10 in the real game (the `*GameRuleWrapper` has no frame/score config).
#[derive(Clone, Copy, Debug)]
pub struct BowlingConfig {
    pub frames: usize,
}

impl Default for BowlingConfig {
    fn default() -> Self {
        BowlingConfig { frames: 10 } // VERIFIED: 10 frames, no ScoreToWin (standard bowling)
    }
}

/// The classic 10-pin scoreboard over a player's ordered bowl array (a strike is stored as a single `10`). Returns
/// the running cumulative total of the RESOLVED frames (a pending bonus doesn't count yet) and whether all `frames`
/// frames are complete. This is the decompiled `BowlingScoreboard.Refresh` logic.
fn score_card(bowls: &[u8], frames: usize) -> (i32, bool) {
    let mut total = 0i32;
    let mut i = 0usize; // index into bowls
    let mut resolved = 0usize; // frames with a final score
    for f in 0..frames {
        if i >= bowls.len() {
            break;
        }
        let last = f == frames - 1;
        if !last {
            if bowls[i] == 10 {
                // strike: 10 + next two bowls (if both bowled yet)
                if i + 2 < bowls.len() {
                    total += 10 + bowls[i + 1] as i32 + bowls[i + 2] as i32;
                    resolved += 1;
                }
                i += 1;
            } else {
                // needs two bowls to resolve the frame
                if i + 1 >= bowls.len() {
                    break; // open frame half-bowled — pending
                }
                let frame = bowls[i] + bowls[i + 1];
                if frame == 10 {
                    // spare: 10 + next one bowl
                    if i + 2 < bowls.len() {
                        total += 10 + bowls[i + 2] as i32;
                        resolved += 1;
                    }
                } else {
                    total += frame as i32; // open
                    resolved += 1;
                }
                i += 2;
            }
        } else {
            // 10th frame: 2 bowls, or 3 if the player strikes/spares (the bonus bowls are taken in-frame)
            let needs = if bowls.get(i) == Some(&10) {
                3
            } else if i + 1 < bowls.len() && bowls[i] + bowls[i + 1] >= 10 {
                3
            } else {
                2
            };
            if bowls.len() - i >= needs {
                total += bowls[i..i + needs].iter().map(|&b| b as i32).sum::<i32>();
                resolved += 1;
            }
            // nothing after the last frame
        }
    }
    (total, resolved == frames)
}

struct Card {
    bowls: Vec<u8>,
    done: bool,
}

/// Bowling as a [`GameMode`] on the room authority: a per-player scorer running the real 10-pin algorithm. Each
/// [`GameInput::Bowl`] records a bowl's pinfall for a player; the mode recomputes that player's card and broadcasts
/// the running total. When every player has finished all 10 frames, the highest total wins.
pub struct BowlingMode {
    config: BowlingConfig,
    players: HashMap<PlayerId, Card>,
    order: Vec<PlayerId>,
    over: bool,
}

impl BowlingMode {
    pub fn new(config: BowlingConfig) -> Self {
        BowlingMode { config, players: HashMap::new(), order: Vec::new(), over: false }
    }

    fn check_match_complete(&mut self, out: &mut Vec<GameEvent>) {
        if self.over || self.players.is_empty() || !self.players.values().all(|c| c.done) {
            return;
        }
        self.over = true;
        // highest cumulative total wins (standard bowling); ties broken by join order
        let winner = self
            .order
            .iter()
            .copied()
            .max_by_key(|id| self.players.get(id).map(|c| score_card(&c.bowls, self.config.frames).0).unwrap_or(i32::MIN));
        if let Some(w) = winner {
            out.push(GameEvent::MatchOver { winner: w });
        }
    }
}

impl GameMode for BowlingMode {
    fn name(&self) -> &'static str {
        "Bowling"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        self.players.insert(id, Card { bowls: Vec::new(), done: false });
        self.order.push(id);
        0 // free-for-all; teams unused (per-player scoring)
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.remove(&id);
        self.order.retain(|&p| p != id);
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let GameInput::Bowl { player, pins } = input else {
            return; // other games' inputs
        };
        let frames = self.config.frames;
        let (total, done) = if let Some(c) = self.players.get_mut(&player) {
            if c.done {
                return; // already finished the card
            }
            c.bowls.push(pins.min(10));
            let (total, done) = score_card(&c.bowls, frames);
            c.done = done;
            (total, done)
        } else {
            return;
        };
        out.push(GameEvent::PlayerScore { player, score: total });
        if done {
            out.push(GameEvent::Cue("cardComplete".into()));
            self.check_match_complete(out);
        }
    }

    fn tick(&mut self, _dt: f32, _out: &mut Vec<GameEvent>) {
        // untimed: frames complete on bowls, the match when every player finishes 10 frames (no clock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(bowls: &[u8]) -> i32 {
        score_card(bowls, 10).0
    }
    fn done(bowls: &[u8]) -> bool {
        score_card(bowls, 10).1
    }

    #[test]
    fn perfect_game_is_300() {
        let bowls = vec![10; 12]; // 12 strikes
        assert_eq!(total(&bowls), 300);
        assert!(done(&bowls));
    }

    #[test]
    fn all_fives_is_150() {
        // 9 frames of 5/5 spare, then 10th frame 5/5/5 -> 150
        let mut bowls = vec![5u8; 21];
        bowls.truncate(21);
        assert_eq!(total(&bowls), 150);
        assert!(done(&bowls));
    }

    #[test]
    fn gutter_game_is_zero() {
        let bowls = vec![0u8; 20];
        assert_eq!(total(&bowls), 0);
        assert!(done(&bowls));
    }

    #[test]
    fn mixed_card_matches_hand_score() {
        // a worked example: 1/4, 4/5, 6/spare(4), 5/strike... use a known textbook card.
        // Frames: X 7/ 9- X -8 8/ -6 X X X8(10th: strike then 8-...). Use the canonical 167 example:
        // 10, 7,3, 9,0, 10, 0,8, 8,2, 0,6, 10, 10, 10,8,1
        let bowls = vec![10, 7, 3, 9, 0, 10, 0, 8, 8, 2, 0, 6, 10, 10, 10, 8, 1];
        assert_eq!(total(&bowls), 167);
        assert!(done(&bowls));
    }

    #[test]
    fn pending_bonus_does_not_count_until_bowled() {
        // a strike in frame 1 with no following bowls yet: frame pending, running total 0
        assert_eq!(total(&[10]), 0);
        assert!(!done(&[10]));
        // once the next two land, it resolves
        assert_eq!(total(&[10, 4, 3]), 17 + 7); // f0 = 10+4+3=17, f1 = 7 -> 24
    }

    #[test]
    fn highest_total_wins_the_match() {
        let mut m = BowlingMode::new(BowlingConfig::default());
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        // player 1 bowls a gutter game (0), player 2 bowls all nines-open-ish (20 bowls of 4 -> open frames = 80)
        for _ in 0..20 {
            m.input(GameInput::Bowl { player: 1, pins: 0 }, &mut out);
        }
        for _ in 0..20 {
            m.input(GameInput::Bowl { player: 2, pins: 4 }, &mut out);
        }
        let winner = out.iter().rev().find_map(|e| if let GameEvent::MatchOver { winner } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(2), "player 2 (80) beats player 1 (0)");
    }
}
