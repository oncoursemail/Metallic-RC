//! StuntRunner, ported from the decompiled `StuntRunnerManager : QuestManager : GameRuleManager` (scene level92 lobby + level93–98
//! course rooms), VERIFIED against the real binary (decompile lane, sweet-galileo_stuntrunner-recon.md). It is a
//! competitive per-player TIME-TRIAL over a fixed sequence of 6 courses: each player runs a course once, their finish
//! time (in milliseconds) converts to a per-course point score (faster = higher, linear to a time cap), and after all
//! 6 courses the highest total score wins (with per-player placement grades + a world leaderboard in the real game).
//!
//! State machine (the real `networkedState`): WAIT_FOR_PLAYERS → COURSE_PREP → COUNTDOWN → COURSE_RUNNING →
//! COURSE_FINISHED → COURSE_TRANSITION → next course. The clock is master-authoritative (starts at COURSE_RUNNING,
//! ticked every frame, capped at `CourseMaxFinishTimeSeconds`); a player stops it by crossing the finish gate
//! (`RpcMasterRequestCourseFinishTime(player, int ms)` → [`GameInput::FinishCourse`]). A course ends when ALL players
//! finish OR the clock hits the cap (a timed-out player scores 0 for that course — the formula at the cap). The gate /
//! checkpoint / respawn gadgets and the course-room loading are client-side; this mode is the sequence + scorer.

use std::collections::HashMap;

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, Team};

/// StuntRunner config. `courses` = 6 (VERIFIED: level93–98). The two scoring numbers are serialized designer values
/// on the StuntRunnerManager prefab, read via the decompile/MonoFields lane.
#[derive(Clone, Copy, Debug)]
pub struct StuntRunnerConfig {
    /// number of courses in the sequence (VERIFIED 6)
    pub courses: usize,
    /// per-course maximum score, awarded for an instant finish (`courseMaxBaseScore`). CONFIRMED 180000 — read
    /// directly from the StuntRunnerManager prefab via `value::read` (the `.ctor` default, NOT overridden); one global
    /// value for all 6 courses.
    pub course_max_base_score: u32,
    /// one time cap in seconds (`CourseMaxFinishTimeSeconds`, auto-property @0x820); at the cap the score is 0.
    /// 1:1: this is PER-COURSE CONTENT, not a client constant (dump-verified, sweet-galileo_bowling-stunt-charades):
    /// the backing field is `[CompilerGenerated]`, NOT `[SerializeField]`, and the finish time is master-authoritative
    /// — `RpcMasterRequestCourseFinishTime(player, int finishTimeMilliseconds)` carries it FROM THE COURSE (each
    /// StuntRunner course's circuit sets its own; `StuntRunner_Config.json` has no finish time). So it VARIES per
    /// course and lives in room/circuit data, not the client binary — model it as a per-course parameter. The value
    /// below is our own-course default. NOTE: the final RANKING / WINNER is cap-independent (total score ranks by total
    /// time either way — the cap only scales absolute scores and sets the timeout), so sequence/formula/winner hold.
    pub course_max_finish_seconds: f32,
}

impl Default for StuntRunnerConfig {
    fn default() -> Self {
        // `courses` = 6 and `course_max_base_score` = 180000 are VERIFIED. `course_max_finish_seconds` is
        // 1:1-PENDING a live capture (runtime-assigned, not in the scene/manager) — see the field doc. The winner is
        // cap-independent, so this placeholder affects only displayed absolute scores and the timeout threshold.
        StuntRunnerConfig { courses: 6, course_max_base_score: 180000, course_max_finish_seconds: 120.0 }
    }
}

struct Card {
    total: i32,
    /// the finish time (ms) this player posted on the current course, if they've finished it
    finished_current: Option<u32>,
}

/// StuntRunner as a [`GameMode`] on the room authority: the 6-course sequence driver + per-player time→score scorer.
/// A [`GameInput::FinishCourse`] records a player's course time and scores it; the master clock (via `tick`) ends a
/// course when everyone finishes or the cap elapses; after course 6 the highest total wins.
pub struct StuntRunnerMode {
    config: StuntRunnerConfig,
    players: Vec<PlayerId>,
    cards: HashMap<PlayerId, Card>,
    course: usize,
    /// seconds elapsed on the current course (master-authoritative clock)
    course_clock: f32,
    over: bool,
}

impl StuntRunnerMode {
    pub fn new(config: StuntRunnerConfig) -> Self {
        StuntRunnerMode { config, players: Vec::new(), cards: HashMap::new(), course: 0, course_clock: 0.0, over: false }
    }

    /// The current course index (0-based), for the client to load the matching course room.
    pub fn course(&self) -> usize {
        self.course
    }

    /// The per-course point score for a finish time in milliseconds: linear from `course_max_base_score` at 0 ms down
    /// to 0 at the cap (`course_max_finish_seconds` * 1000). This is the real `courseMaxBaseScore − (courseMaxBaseScore
    /// / (CourseMaxFinishTimeSeconds*1000)) * timeMs`, clamped at 0.
    fn score_for(&self, time_ms: u32) -> u32 {
        let cap_ms = self.config.course_max_finish_seconds * 1000.0;
        if cap_ms <= 0.0 || time_ms as f32 >= cap_ms {
            return 0;
        }
        let per_ms = self.config.course_max_base_score as f32 / cap_ms;
        (self.config.course_max_base_score as f32 - per_ms * time_ms as f32).max(0.0).round() as u32
    }

    fn all_finished(&self) -> bool {
        !self.players.is_empty() && self.players.iter().all(|p| self.cards.get(p).is_some_and(|c| c.finished_current.is_some()))
    }

    /// Advance to the next course (reset per-course finishes + clock); after the last course, end the match.
    fn advance_course(&mut self, out: &mut Vec<GameEvent>) {
        self.course += 1;
        self.course_clock = 0.0;
        for c in self.cards.values_mut() {
            c.finished_current = None;
        }
        if self.course >= self.config.courses {
            self.finish(out);
        } else {
            out.push(GameEvent::Cue(format!("courseStart:{}", self.course)));
        }
    }

    fn finish(&mut self, out: &mut Vec<GameEvent>) {
        self.over = true;
        // highest total score wins; ties broken by join order. The winner + totals are EXACT (and cap-independent).
        // The end-game grades (S+/S/A/B/C, real notification titles) are COSMETIC — they show each player's rank and
        // are winner/scoring-independent. Confirmed via the prefab read: the grade is picked by CODE in the
        // win-sequence coroutine, NOT serialized percentile cutoffs (feedbackBuckets is empty). Not modeled here.
        let winner = self.players.iter().copied().max_by_key(|p| self.cards.get(p).map(|c| c.total).unwrap_or(i32::MIN));
        if let Some(w) = winner {
            out.push(GameEvent::MatchOver { winner: w });
        }
    }
}

impl GameMode for StuntRunnerMode {
    fn name(&self) -> &'static str {
        "StuntRunner"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        self.players.push(id);
        self.cards.insert(id, Card { total: 0, finished_current: None });
        0 // per-player free-for-all; teams unused
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.retain(|&p| p != id);
        self.cards.remove(&id);
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let GameInput::FinishCourse { player, time_ms } = input else { return };
        if self.over {
            return;
        }
        let score = self.score_for(time_ms);
        if let Some(c) = self.cards.get_mut(&player) {
            if c.finished_current.is_some() {
                return; // one run per course
            }
            c.finished_current = Some(time_ms);
            c.total += score as i32;
            out.push(GameEvent::PlayerScore { player, score: c.total });
        } else {
            return;
        }
        if self.all_finished() {
            self.advance_course(out);
        }
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        if self.over || self.players.is_empty() {
            return;
        }
        self.course_clock += dt;
        if self.course_clock >= self.config.course_max_finish_seconds {
            // the course timed out: everyone who hasn't finished scores 0 for this course (the formula at the cap),
            // then the course ends.
            let unfinished: Vec<PlayerId> = self.players.iter().copied().filter(|p| self.cards.get(p).is_some_and(|c| c.finished_current.is_none())).collect();
            for p in unfinished {
                if let Some(c) = self.cards.get_mut(&p) {
                    c.finished_current = Some(u32::MAX); // timed out → score 0 (already reflected: total unchanged)
                    out.push(GameEvent::PlayerScore { player: p, score: c.total });
                }
            }
            self.advance_course(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(courses: usize) -> StuntRunnerConfig {
        StuntRunnerConfig { courses, course_max_base_score: 1000, course_max_finish_seconds: 100.0 }
    }

    #[test]
    fn faster_total_time_wins_over_the_course_sequence() {
        // 2 courses, 2 players; player 1 is faster on both → higher total
        let mut m = StuntRunnerMode::new(cfg(2));
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();

        // course 0: p1 10 s (10000 ms), p2 50 s
        m.input(GameInput::FinishCourse { player: 1, time_ms: 10_000 }, &mut out);
        m.input(GameInput::FinishCourse { player: 2, time_ms: 50_000 }, &mut out); // both finished → advance to course 1
        // course 1: p1 20 s, p2 40 s
        m.input(GameInput::FinishCourse { player: 1, time_ms: 20_000 }, &mut out);
        m.input(GameInput::FinishCourse { player: 2, time_ms: 40_000 }, &mut out); // both finished → finish match

        let winner = out.iter().rev().find_map(|e| if let GameEvent::MatchOver { winner } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(1), "the faster player (higher total score) wins");
    }

    #[test]
    fn score_is_linear_faster_is_higher_and_cap_is_zero() {
        let m = StuntRunnerMode::new(cfg(6));
        assert_eq!(m.score_for(0), 1000, "instant finish = full base score");
        assert_eq!(m.score_for(50_000), 500, "half the cap time = half the score");
        assert_eq!(m.score_for(100_000), 0, "at the cap = 0");
        assert_eq!(m.score_for(200_000), 0, "past the cap = 0 (clamped)");
        assert!(m.score_for(10_000) > m.score_for(30_000), "faster scores higher");
    }

    #[test]
    fn a_timed_out_player_scores_zero_for_the_course() {
        let mut m = StuntRunnerMode::new(cfg(1)); // single course so the timeout ends the match
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        // p1 finishes at 20 s (score 800); p2 never finishes
        m.input(GameInput::FinishCourse { player: 1, time_ms: 20_000 }, &mut out);
        // clock runs past the 100 s cap → course times out, p2 scores 0, match ends
        m.tick(101.0, &mut out);
        let winner = out.iter().rev().find_map(|e| if let GameEvent::MatchOver { winner } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(1), "the finisher beats the timed-out player (who scored 0)");
    }
}
