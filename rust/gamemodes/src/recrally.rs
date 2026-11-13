//! RecRally, ported from the decompiled `RecRallyManager : GameRuleManager` (scene level107, RecRally_Chaparral),
//! VERIFIED against the real binary (decompile lane, sweet-galileo_recrally-recon.md). A kart race scored purely by
//! TEAM placement in FINISH ORDER — no points, no time score. Karts are assigned to `GameTeam`s; a team finishes when
//! it completes all the laps (`totalLaps` = 3, real, read from level107's LapManager), and the master assigns its
//! PLACE in the order teams complete. First team home = place 1 = winner; the rest are ranked by finish order. It is a
//! single race; `GameDuration` is only a FALLBACK time limit (if the race drags, the standings stand).
//!
//! The mode is the finish-order scorer: a [`GameInput::FinishRace`] (a team crossed the line on the final lap,
//! master-validated) assigns the next place. The lap/checkpoint triggers (`LapCollider`, ordered) and the live
//! `TeamPlaceChanged` standings are client-side (the kart `GroundVehicle` physics is a separate bevy_rapier item);
//! this mode only needs the finish-order placement.

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, RoundPhase, Team};

/// RecRally config. `total_laps` = 3 (VERIFIED, level107 LapManager).
#[derive(Clone, Copy, Debug)]
pub struct RecRallyConfig {
    /// laps to complete the race (VERIFIED 3)
    pub total_laps: u32,
    /// fallback time limit in seconds (`RecRallyGameRuleWrapper.defaultGameDuration`). Only a fallback — the win is
    /// finish-order, not time. 1:1-PENDING: level107 has no type trees, so this float (@il2cpp 0x88 on the wrapper
    /// circuit) needs maps' MonoFields offset read; it is NOT win-critical.
    pub game_duration: f32,
}

impl Default for RecRallyConfig {
    fn default() -> Self {
        // total_laps = 3 VERIFIED. game_duration is a //1:1-pending fallback (maps read), not win-critical — the
        // placeholder below only bounds a race where nobody finishes.
        RecRallyConfig { total_laps: 3, game_duration: 300.0 }
    }
}

/// RecRally as a [`GameMode`] on the room authority: the finish-order placement scorer. Teams finishing the race are
/// assigned places 1..N in order; place 1 is the winner.
pub struct RecRallyMode {
    /// participating teams, in join order (each racer/kart is a team)
    teams: Vec<Team>,
    joined: u32,
    /// teams in the order they finished the race
    finish_order: Vec<Team>,
    /// fallback race clock (counts down from `game_duration`; `total_laps` is tracked client-side)
    clock: f32,
    winner: Option<Team>,
    over: bool,
}

impl RecRallyMode {
    pub fn new(config: RecRallyConfig) -> Self {
        RecRallyMode { teams: Vec::new(), joined: 0, finish_order: Vec::new(), clock: config.game_duration, winner: None, over: false }
    }

    /// The team's finishing place (1-based), if it has finished.
    pub fn place_of(&self, team: Team) -> Option<usize> {
        self.finish_order.iter().position(|&t| t == team).map(|i| i + 1)
    }
}

impl GameMode for RecRallyMode {
    fn name(&self) -> &'static str {
        "RecRally"
    }

    fn add_player(&mut self, _id: PlayerId) -> Team {
        // each racer is its own team/kart; the race ranks teams by finish order
        let team = self.joined as Team;
        self.joined += 1;
        self.teams.push(team);
        team
    }

    fn remove_player(&mut self, _id: PlayerId) {}

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let GameInput::FinishRace { team } = input else { return };
        if self.over || self.finish_order.contains(&team) {
            return; // single race; a team places once
        }
        self.finish_order.push(team);
        let place = self.finish_order.len();
        out.push(GameEvent::Cue(format!("placed:{team}:{place}")));
        if place == 1 {
            // first team home wins the race
            self.winner = Some(team);
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner: Some(team) });
        }
        // once every team has finished, the race is fully over (ranking complete)
        if self.finish_order.len() >= self.teams.len() {
            self.over = true;
        }
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        if self.over {
            return;
        }
        self.clock -= dt;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            self.over = true;
            if self.winner.is_none() {
                // fallback: time expired with nobody across the line. 1:1-pending: the real game ranks by current
                // standings (live lap/checkpoint progress, client-side) — not tracked in this scorer — so there is no
                // finish-order winner to declare here.
                out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner: None });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_team_across_the_line_wins_and_places_follow_finish_order() {
        let mut m = RecRallyMode::new(RecRallyConfig::default());
        let a = m.add_player(1);
        let b = m.add_player(2);
        let c = m.add_player(3);

        let mut out = Vec::new();
        // they finish in the order c, a, b
        m.input(GameInput::FinishRace { team: c }, &mut out);
        m.input(GameInput::FinishRace { team: a }, &mut out);
        m.input(GameInput::FinishRace { team: b }, &mut out);

        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(c)), "the first team across the line wins");
        assert_eq!(m.place_of(c), Some(1));
        assert_eq!(m.place_of(a), Some(2));
        assert_eq!(m.place_of(b), Some(3));
    }

    #[test]
    fn a_team_can_only_place_once() {
        let mut m = RecRallyMode::new(RecRallyConfig::default());
        let a = m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        m.input(GameInput::FinishRace { team: a }, &mut out);
        m.input(GameInput::FinishRace { team: a }, &mut out); // duplicate ignored
        assert_eq!(m.finish_order.len(), 1, "a team places exactly once");
    }

    #[test]
    fn fallback_timeout_with_no_finisher_ends_with_no_winner() {
        let cfg = RecRallyConfig { total_laps: 3, game_duration: 5.0 };
        let mut m = RecRallyMode::new(cfg);
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        m.tick(5.0, &mut out); // nobody finished
        let ended = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(ended, Some(None), "the fallback timeout ends the race with no finish-order winner");
    }
}
