//! Paintball, ported from the decompiled `PaintballManager : GameRuleManager` (scene level14 + 15–18 arena map pool),
//! VERIFIED against the real binary (decompile lane, sweet-galileo_paintball-recon.md). A competitive TEAM shooter in
//! two config-driven modes (the room runs a CTF or a TeamBattle `GameRuleWrapper` circuit — any map hosts either):
//!
//! - **CaptureTheFlag**: carry the enemy flag to your base to CAPTURE it (+1 your team); first team to
//!   `defaultFlagCapturesToWin` = 3, or the higher score when the 600 s clock expires.
//! - **TeamBattle**: a paintball TAG scores +1 for the shooter's team; first team to `defaultScoreToWin` = 50, or the
//!   higher score at 600 s. Respawn-based (a tag respawns the victim, it does NOT eliminate for the round).
//!
//! Both are TIMED (`defaultGameDuration` = 600 s) with a score cap, master-authoritative. A tag is master-validated
//! exactly like Dodgeball's `IsValidHit` (round active, both players in, different teams, not self) and reuses
//! [`GameInput::Hit`] (thrower = the shooter). The CTF flag events are [`GameInput::Flag`]. All numbers are the real
//! wrapper defaults (room-overridable at runtime); no invented values.

use std::collections::HashMap;

use crate::mode::{FlagEvent, GameEvent, GameInput, GameMode, PlayerId, RoundPhase, Team};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintballVariant {
    Ctf,
    TeamBattle,
}

/// Paintball config — VERIFIED wrapper defaults (applied at runtime from the room's GameRuleData; room-overridable).
#[derive(Clone, Copy, Debug)]
pub struct PaintballConfig {
    pub variant: PaintballVariant,
    /// CTF `defaultFlagCapturesToWin` = 3 / TeamBattle `defaultScoreToWin` = 50
    pub score_to_win: u32,
    /// `defaultGameDuration` = 600 s (both modes)
    pub game_duration: f32,
}

impl PaintballConfig {
    pub fn ctf() -> Self {
        PaintballConfig { variant: PaintballVariant::Ctf, score_to_win: 3, game_duration: 600.0 }
    }
    pub fn team_battle() -> Self {
        PaintballConfig { variant: PaintballVariant::TeamBattle, score_to_win: 50, game_duration: 600.0 }
    }
}

impl Default for PaintballConfig {
    fn default() -> Self {
        Self::team_battle()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FlagState {
    AtBase,
    Carried(PlayerId),
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Running,
    Over(Option<Team>),
}

/// Paintball as a [`GameMode`] on the room authority: a team scorer with a match clock + score cap, in either mode.
pub struct PaintballMode {
    config: PaintballConfig,
    teams: HashMap<PlayerId, Team>,
    joined: u32,
    score: [u32; 2],
    /// match time remaining (counts down from `game_duration`)
    clock: f32,
    phase: Phase,
    /// CTF: `flags[t]` is team `t`'s flag (carried by an enemy, dropped, or at its base)
    flags: [FlagState; 2],
}

impl PaintballMode {
    pub fn new(config: PaintballConfig) -> Self {
        PaintballMode {
            config,
            teams: HashMap::new(),
            joined: 0,
            score: [0, 0],
            clock: config.game_duration,
            phase: Phase::Running,
            flags: [FlagState::AtBase, FlagState::AtBase],
        }
    }

    fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }

    fn team_of(&self, p: PlayerId) -> Option<Team> {
        self.teams.get(&p).copied()
    }

    /// Credit `team` one point; if it reaches the cap, end the match with that team as the winner.
    fn award(&mut self, team: Team, out: &mut Vec<GameEvent>) {
        let t = team as usize;
        if t >= 2 {
            return;
        }
        self.score[t] += 1;
        out.push(GameEvent::Score { team, score: self.score[t] });
        if self.score[t] >= self.config.score_to_win {
            self.phase = Phase::Over(Some(team));
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner: Some(team) });
        }
    }

    fn leader(&self) -> Option<Team> {
        match self.score[0].cmp(&self.score[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    }
}

impl GameMode for PaintballMode {
    fn name(&self) -> &'static str {
        "Paintball"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        let team = (self.joined % 2) as Team;
        self.joined += 1;
        self.teams.insert(id, team);
        team
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.teams.remove(&id);
        // a leaving carrier drops whatever flag they held
        for f in self.flags.iter_mut() {
            if *f == FlagState::Carried(id) {
                *f = FlagState::Dropped;
            }
        }
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        if !self.is_running() {
            return;
        }
        match input {
            // a paintball tag (master-validated like Dodgeball's IsValidHit)
            GameInput::Hit { thrower, victim } => {
                let (Some(ta), Some(tv)) = (self.team_of(thrower), self.team_of(victim)) else { return };
                if thrower == victim || ta == tv {
                    return; // no self-tag, no friendly fire
                }
                match self.config.variant {
                    PaintballVariant::TeamBattle => {
                        // +1 the shooter's team; respawn-based (the victim respawns — client-side)
                        out.push(GameEvent::Cue(format!("tag:{victim}")));
                        self.award(ta, out);
                    }
                    PaintballVariant::Ctf => {
                        // a tagged flag-carrier drops the flag; tags don't score in CTF
                        for f in self.flags.iter_mut() {
                            if *f == FlagState::Carried(victim) {
                                *f = FlagState::Dropped;
                                out.push(GameEvent::Cue(format!("flagDropped:{victim}")));
                            }
                        }
                    }
                }
            }
            // a CTF flag event (TeamBattle has no flags)
            GameInput::Flag { event, player, team } => {
                if self.config.variant != PaintballVariant::Ctf {
                    return;
                }
                let ft = team as usize;
                if ft >= 2 {
                    return;
                }
                match event {
                    FlagEvent::Taken => self.flags[ft] = FlagState::Carried(player),
                    FlagEvent::Dropped => self.flags[ft] = FlagState::Dropped,
                    FlagEvent::Returned => self.flags[ft] = FlagState::AtBase,
                    FlagEvent::Captured => {
                        // the carrier must actually be carrying this (enemy) flag; the capturing team is theirs
                        if self.flags[ft] != FlagState::Carried(player) {
                            return;
                        }
                        let Some(scoring) = self.team_of(player) else { return };
                        self.flags[ft] = FlagState::AtBase; // the captured flag returns to its base
                        out.push(GameEvent::Cue(format!("flagCaptured:{player}")));
                        self.award(scoring, out);
                    }
                }
            }
            _ => {} // other games' inputs
        }
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        if !self.is_running() {
            return;
        }
        self.clock -= dt;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            let winner = self.leader(); // higher team at time; None = draw
            self.phase = Phase::Over(winner);
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_battle_tags_score_to_the_cap() {
        let cfg = PaintballConfig { variant: PaintballVariant::TeamBattle, score_to_win: 3, game_duration: 600.0 };
        let mut m = PaintballMode::new(cfg);
        let t0 = m.add_player(1);
        let t1 = m.add_player(2);
        assert_ne!(t0, t1);

        // friendly fire + self-tag are rejected
        m.add_player(3); // player 3 on team 0 (same as player 1)
        let mut bad = Vec::new();
        m.input(GameInput::Hit { thrower: 1, victim: 3 }, &mut bad); // same team
        m.input(GameInput::Hit { thrower: 1, victim: 1 }, &mut bad); // self
        assert!(!bad.iter().any(|e| matches!(e, GameEvent::Score { .. })), "no friendly fire / self-tag");

        // player 1 (team 0) tags player 2 (team 1) three times → team 0 hits the cap and wins
        let mut out = Vec::new();
        for _ in 0..3 {
            m.input(GameInput::Hit { thrower: 1, victim: 2 }, &mut out);
        }
        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(t0)), "team 0 wins at the tag cap");
    }

    #[test]
    fn ctf_capture_requires_carrying_and_scores_the_carrier_team() {
        let cfg = PaintballConfig { variant: PaintballVariant::Ctf, score_to_win: 2, game_duration: 600.0 };
        let mut m = PaintballMode::new(cfg);
        let t0 = m.add_player(1); // team 0
        let _t1 = m.add_player(2); // team 1

        // a capture with nobody carrying is rejected
        let mut bad = Vec::new();
        m.input(GameInput::Flag { event: FlagEvent::Captured, player: 1, team: 1 }, &mut bad);
        assert!(!bad.iter().any(|e| matches!(e, GameEvent::Score { .. })), "can't capture a flag you're not carrying");

        // player 1 (team 0) takes team 1's flag, then captures it → team 0 +1
        let mut out = Vec::new();
        m.input(GameInput::Flag { event: FlagEvent::Taken, player: 1, team: 1 }, &mut out);
        m.input(GameInput::Flag { event: FlagEvent::Captured, player: 1, team: 1 }, &mut out);
        assert!(out.iter().any(|e| matches!(e, GameEvent::Score { team, .. } if *team == t0)), "the carrier's team scores");

        // a second capture wins at the cap (2)
        m.input(GameInput::Flag { event: FlagEvent::Taken, player: 1, team: 1 }, &mut out);
        m.input(GameInput::Flag { event: FlagEvent::Captured, player: 1, team: 1 }, &mut out);
        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(t0)), "team 0 wins at 2 captures");
    }

    #[test]
    fn ctf_tag_drops_the_carried_flag() {
        let cfg = PaintballConfig::ctf();
        let mut m = PaintballMode::new(cfg);
        m.add_player(1); // team 0
        m.add_player(2); // team 1
        let mut out = Vec::new();
        m.input(GameInput::Flag { event: FlagEvent::Taken, player: 1, team: 1 }, &mut out); // p1 carries team 1's flag
        m.input(GameInput::Hit { thrower: 2, victim: 1 }, &mut out); // p2 tags the carrier
        assert!(out.iter().any(|e| matches!(e, GameEvent::Cue(c) if c.starts_with("flagDropped"))), "a tagged carrier drops the flag");
        // now a capture by p1 fails (no longer carrying)
        let mut after = Vec::new();
        m.input(GameInput::Flag { event: FlagEvent::Captured, player: 1, team: 1 }, &mut after);
        assert!(!after.iter().any(|e| matches!(e, GameEvent::Score { .. })), "can't capture after dropping");
    }

    #[test]
    fn higher_score_wins_when_the_clock_expires() {
        let cfg = PaintballConfig { variant: PaintballVariant::TeamBattle, score_to_win: 50, game_duration: 10.0 };
        let mut m = PaintballMode::new(cfg);
        m.add_player(1); // team 0
        m.add_player(2); // team 1
        let mut out = Vec::new();
        m.input(GameInput::Hit { thrower: 1, victim: 2 }, &mut out); // team 0: 1
        m.tick(10.0, &mut out); // clock expires
        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(0)), "the higher team wins at time");
    }
}
