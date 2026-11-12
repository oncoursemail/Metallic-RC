//! Soccer, ported from the decompiled `SoccerManager : GameRuleManager` (scene level72), VERIFIED against the real
//! binary (decompile lane, sweet-galileo_soccer-recon.md). It is a competitive TEAM GOAL game like Paddleball — two
//! teams, each defends a goal; a ball driven into a goal scores for the OPPOSING team — but, crucially, it is
//! **TIMED, not goals-to-win**: the match runs a fixed duration (`SoccerGameRuleWrapper.defaultGameDuration` = 180 s)
//! and whoever has the MOST GOALS when the clock expires wins (a draw is possible). In the real game the 180 s rides
//! the base `GameRuleManager` round timer; here the [`GameMode`] owns the clock directly (engine-agnostic).
//!
//! Scoring is master-authoritative (same shape as Paddleball): the client that sees the ball enter a goal raises
//! `RpcMasterRequestScore`; the master checks the round is active, credits the goal to the goal's OPPONENT (so
//! own-goals count naturally — the team is decided by which goal the ball entered, not by the last toucher), resets
//! the ball to centre for kickoff, and broadcasts `RpcOnScore`. There is no fixed post-goal pause constant — the
//! real restart is the `WaitAndPlayRoundStarting` kickoff coroutine, modelled here as a short kickoff pause
//! (marked `// 1:1:`). Powerups are player ability buffs, not score modifiers (stubbed here). The ball is the same
//! `Ball` base as Dodgeball (physics-authentic), additionally kickable by running into it.

use crate::mode::{GameEvent, GameInput, GameMode, RoundPhase, Team};

/// Soccer config.
#[derive(Clone, Copy, Debug)]
pub struct SoccerConfig {
    /// match length in seconds; most goals at expiry wins (`SoccerGameRuleWrapper.defaultGameDuration`). VERIFIED 180.
    pub match_duration: f32,
}

impl Default for SoccerConfig {
    fn default() -> Self {
        SoccerConfig { match_duration: 180.0 } // VERIFIED
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// ball-scored / kickoff state (pre-round and post-goal): the ball is reset to centre and goals don't count,
    /// until the round-start sequence completes. This is EVENT-DRIVEN, not timed: the decompiled
    /// `WaitAndPlayRoundStarting` coroutine awaits an async round-start op and has NO numeric duration (verified — no
    /// WaitForSeconds / no const / no inspector field). We resume on the authority's next tick (the round-start op is
    /// instant in this rebuild — no async loading to wait for), so no number is invented.
    Kickoff,
    Playing,
    /// match over; the winner, or `None` for a draw
    Over(Option<Team>),
}

/// Soccer as a [`GameMode`] on the room authority: a pure validator + scorer with a match clock. A goal-trigger
/// client raises a [`GameInput::Goal`] carrying the scoring team (the goal's opponent, resolved at detection — so
/// own-goals count), the master counts it, runs a short kickoff pause, and when the clock expires the team with the
/// most goals wins.
pub struct SoccerMode {
    phase: Phase,
    /// match time remaining (counts down from `match_duration`)
    clock: f32,
    /// goals per team
    team_score: Vec<u32>,
    joined: u32,
}

impl SoccerMode {
    pub fn new(config: SoccerConfig) -> Self {
        SoccerMode {
            phase: Phase::Kickoff,
            clock: config.match_duration,
            team_score: vec![0, 0],
            joined: 0,
        }
    }

    fn is_playing(&self) -> bool {
        self.phase == Phase::Playing
    }

    /// The sole team with the most goals, or `None` for a draw (tie at the top, including 0–0).
    fn leader(&self) -> Option<Team> {
        let max = self.team_score.iter().copied().max().unwrap_or(0);
        let leaders: Vec<usize> = self.team_score.iter().enumerate().filter(|(_, &s)| s == max).map(|(i, _)| i).collect();
        (leaders.len() == 1).then(|| leaders[0] as Team)
    }
}

impl GameMode for SoccerMode {
    fn name(&self) -> &'static str {
        "Soccer"
    }

    fn add_player(&mut self, _id: crate::mode::PlayerId) -> Team {
        let team = (self.joined % 2) as Team; // alternate teams
        self.joined += 1;
        if team as usize >= self.team_score.len() {
            self.team_score.resize(team as usize + 1, 0);
        }
        team
    }

    fn remove_player(&mut self, _id: crate::mode::PlayerId) {}

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        // Soccer only scores on a goal; the Dodgeball/DiscGolf inputs are ignored.
        let GameInput::Goal { team } = input else { return };
        if !self.is_playing() {
            return; // a goal request is only honoured while the round is active (not during the kickoff pause)
        }
        let t = team as usize;
        if t >= self.team_score.len() {
            return;
        }
        self.team_score[t] += 1;
        let score = self.team_score[t];
        out.push(GameEvent::Score { team, score });
        out.push(GameEvent::Cue("goalScored".into())); // RpcOnScore: VFX/SFX/VO + ball reset to centre
        // no goals-to-win check — the match is decided by the clock; a goal just enters the ball-scored/kickoff state
        self.phase = Phase::Kickoff;
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        if matches!(self.phase, Phase::Over(_)) {
            return;
        }
        // the match clock runs down over the whole match.
        // 1:1: assumed to keep running during the kickoff state (the base round timer is not paused on a goal in the
        // decompiled Update; revisit if the base timer turns out to stop during the ball-scored state).
        self.clock -= dt;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            let winner = self.leader();
            self.phase = Phase::Over(winner);
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner });
            return;
        }
        // event-driven kickoff: the round-start op completes (instantly in this rebuild — no async loading), so play
        // resumes on this next authority tick. No invented pause duration.
        if self.phase == Phase::Kickoff {
            self.phase = Phase::Playing;
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::Running, winner: None });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_most_goals_wins() {
        // a short match so the test is quick; the real default is 180 s
        let cfg = SoccerConfig { match_duration: 10.0 };
        let mut m = SoccerMode::new(cfg);
        let t0 = m.add_player(1);
        let t1 = m.add_player(2);
        assert_ne!(t0, t1);

        // the opening kickoff resumes on the next tick (event-driven, no pause duration)
        let mut out = Vec::new();
        m.tick(0.01, &mut out);
        assert!(out.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::Running, .. })));

        // a goal during the kickoff state doesn't count
        m.phase = Phase::Kickoff;
        let mut ignored = Vec::new();
        m.input(GameInput::Goal { team: t0 }, &mut ignored);
        assert!(!ignored.iter().any(|e| matches!(e, GameEvent::Score { .. })), "a goal during kickoff doesn't count");
        m.phase = Phase::Playing;

        // team 0 scores twice, team 1 once; each goal is followed by the next tick clearing the kickoff state
        for team in [t0, t0, t1] {
            let mut ev = Vec::new();
            m.input(GameInput::Goal { team }, &mut ev);
            assert!(ev.iter().any(|e| matches!(e, GameEvent::Score { .. })), "goal counts while playing");
            m.tick(0.01, &mut Vec::new()); // next tick clears kickoff back to Playing
        }

        // run the clock out
        let mut end = Vec::new();
        m.tick(cfg.match_duration, &mut end);
        let winner = end.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(t0)), "team 0 (2 goals) beats team 1 (1 goal) when the clock expires");
    }

    #[test]
    fn equal_goals_is_a_draw() {
        let cfg = SoccerConfig { match_duration: 6.0 };
        let mut m = SoccerMode::new(cfg);
        let t0 = m.add_player(1);
        let t1 = m.add_player(2);
        m.tick(0.01, &mut Vec::new()); // kickoff -> Playing
        for team in [t0, t1] {
            m.input(GameInput::Goal { team }, &mut Vec::new());
            m.tick(0.01, &mut Vec::new());
        }
        let mut end = Vec::new();
        m.tick(cfg.match_duration, &mut end);
        let winner = end.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(None), "1–1 at the whistle is a draw (no winner)");
    }
}
