//! Paddleball, ported from the decompiled `PaddleballManager : GameRuleManager` (@dump.cs:10788), all VERIFIED against
//! the real binary (decompile lane, sweet-galileo_paddleball-recon.md). Unlike Dodgeball this is a competitive TEAM
//! GOAL game (air-hockey / pong): two teams, each defends a goal; a ball driven into a goal scores for a team, and the
//! first team to `GoalsToWin` wins. It shares almost everything with Dodgeball — the master-validator net flow, the
//! round virtuals, the Update killzone, the scene load, and physics-authentic props (the paddle is a held physics
//! collider with a swing anim; the ball flies by physics, no scripted force) — so this [`GameMode`] is a drop-in; only
//! the scoring differs (goal-trigger team scoring vs elimination).
//!
//! Net flow (same shape as Dodgeball's PlayerOut): the client that sees the ball enter a goal raises
//! `RpcMasterRequestScore`; the master checks the round is active, counts the goal for the scoring team, sets the
//! post-goal pause, and broadcasts `RpcOnScore`. Win at `GoalsToWin` → round end.

use crate::mode::{GameEvent, GameInput, GameMode, RoundPhase, Team};

/// Paddleball config — VERIFIED from the prefab/corpus (decompile lane).
#[derive(Clone, Copy, Debug)]
pub struct PaddleballConfig {
    /// goals to win the match (`PaddleballGameRuleWrapper.defaultGoalsToWin`)
    pub goals_to_win: u32,
    /// the post-goal pause / serve reset (`ballScoredStateDuration`)
    pub ball_scored_pause: f32,
}

impl Default for PaddleballConfig {
    fn default() -> Self {
        PaddleballConfig { goals_to_win: 5, ball_scored_pause: 2.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// the post-goal / serve pause, seconds left (`ballScoredState`)
    Serving(f32),
    Playing,
    GameOver(Team),
}

/// Paddleball as a [`GameMode`] on the room authority. A pure validator + scorer: a goal-trigger client raises a
/// [`GameInput::Goal`], the master counts it for the scoring team, runs the post-goal pause, and ends the match at
/// `goals_to_win`.
pub struct PaddleballMode {
    config: PaddleballConfig,
    phase: Phase,
    /// goals per team
    team_score: Vec<u32>,
    joined: u32,
}

impl PaddleballMode {
    pub fn new(config: PaddleballConfig) -> Self {
        let pause = config.ball_scored_pause;
        PaddleballMode { config, phase: Phase::Serving(pause), team_score: vec![0, 0], joined: 0 }
    }

    fn is_playing(&self) -> bool {
        self.phase == Phase::Playing
    }
}

impl GameMode for PaddleballMode {
    fn name(&self) -> &'static str {
        "Paddleball"
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
        // Paddleball only scores on a goal; Hit/Catch are Dodgeball's and are ignored.
        let GameInput::Goal { team } = input else { return };
        if !self.is_playing() {
            return; // RpcMasterRequestScore is only honoured while the round is active (not during the serve pause)
        }
        let t = team as usize;
        if t >= self.team_score.len() {
            return;
        }
        self.team_score[t] += 1;
        let score = self.team_score[t];
        out.push(GameEvent::Score { team, score });
        out.push(GameEvent::Cue("ballScored".into()));
        if score >= self.config.goals_to_win {
            self.phase = Phase::GameOver(team);
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner: Some(team) });
        } else {
            // the 2s ball-scored state (serve reset) before play resumes
            self.phase = Phase::Serving(self.config.ball_scored_pause);
        }
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        if let Phase::Serving(t) = self.phase {
            let t = t - dt;
            if t <= 0.0 {
                self.phase = Phase::Playing;
                out.push(GameEvent::Round { round: 1, phase: RoundPhase::Running, winner: None });
            } else {
                self.phase = Phase::Serving(t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_team_to_five_goals_wins() {
        let mut m = PaddleballMode::new(PaddleballConfig::default());
        let t0 = m.add_player(1);
        let t1 = m.add_player(2);
        assert_ne!(t0, t1);

        let mut out = Vec::new();
        m.tick(2.0, &mut out); // serve pause elapses -> Playing
        assert!(out.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::Running, .. })));

        // a goal while serving is ignored (only counts while playing)
        m.phase = Phase::Serving(2.0);
        let mut ignored = Vec::new();
        m.input(GameInput::Goal { team: t0 }, &mut ignored);
        assert!(!ignored.iter().any(|e| matches!(e, GameEvent::Score { .. })), "a goal during the serve pause doesn't count");
        m.phase = Phase::Playing;

        // team 0 scores 5 goals; each is honoured only after the serve pause clears
        let mut winner = None;
        for i in 1..=5 {
            let mut ev = Vec::new();
            m.input(GameInput::Goal { team: t0 }, &mut ev);
            let score = ev.iter().find_map(|e| if let GameEvent::Score { team, score } = e { (*team == t0).then_some(*score) } else { None });
            assert_eq!(score, Some(i), "goal {i} counts for team 0");
            if let Some(GameEvent::Round { winner: Some(w), phase: RoundPhase::GameOver, .. }) = ev.iter().find(|e| matches!(e, GameEvent::Round { phase: RoundPhase::GameOver, .. })) {
                winner = Some(*w);
            }
            m.tick(2.0, &mut Vec::new()); // clear the post-goal pause back to Playing
        }
        assert_eq!(winner, Some(t0), "team 0 wins at 5 goals");
    }
}
