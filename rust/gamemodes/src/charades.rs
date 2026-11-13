//! Charades, ported from the decompiled `CharadesManager : GameRuleManager` (scene level8), VERIFIED against the real
//! binary (decompile lane, sweet-galileo_charades-recon.md). It is DRAW-and-guess (Pictionary-style, not acting): the
//! PERFORMER draws the current word with a pen, and GUESSERS press a physical guess button to signal a correct guess.
//! Correctness is the in-world button signal (master-validated: the guesser is NOT the performer), not text matching —
//! so a correct guess is a [`GameInput::Guess`], and the word bank itself is server/content-driven (not in these
//! rules). 1:1: the words are EXTERNAL, dump-verified NOT in the client (sweet-galileo_bowling-stunt-charades):
//! `CharadesManager` has no word array; words load from a `CardBox` → `CardData` (namespace RecNet, field `EN_US` =
//! the word) via `GET api/activities/charades/v1/words/{source}`, source enum {Charades=0, CharadesAprilFoolsDay=1,
//! Icebreakers=2}. RR's servers are down (2026-06-01), so the live list isn't fetchable — model the word source as
//! external, never invent words. Two modes, both from the real wrappers (no invented numbers):
//!
//! - **CoOpClassic**: all players vs the clock. A 480 s game, 60 s per performer; a correct guess is +1 to the SHARED
//!   score; performers rotate; most words before the game timer expires is the co-op result.
//! - **TeamLightning**: two teams, 4 team rounds. Each team round lasts 90 s with 45 s per performer; a correct guess
//!   is +1 to the GUESSING team; the performing team alternates each round; highest team score after 4 rounds wins.
//!
//! Scoring is master-authoritative (`RpcMasterRequestGuess` → `RpcOnGuess`); the performer rotation
//! (PerformerPlayerIndex / CurrentPerformingTeam / per-team last-performer cursor) and the per-turn timer
//! (`CharadesRoundTimer`) live here. The pen drawing + the guess-button gadget are client-side.

use std::collections::HashMap;

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, RoundPhase, Team};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    CoOp,
    Team,
}

/// Charades config — all values VERIFIED from the `Charades*GameRuleWrapper`s (the same authoritative mechanism as
/// Soccer's `defaultGameDuration` / Paddleball's `defaultGoalsToWin`).
#[derive(Clone, Copy, Debug)]
pub struct CharadesConfig {
    pub variant: Variant,
    /// CoOp game clock in seconds (`CoOpGameDuration`, drives the base round timer). Unused for Team.
    pub game_duration: f32,
    /// per-performer countdown in seconds (`defaultRoundPerformerDuration` CoOp 60, `TeamLightningMaxPerformerDuration` Team 45)
    pub performer_turn: f32,
    /// Team: number of team rounds (`TeamLightningGameRounds` = 4). Unused for CoOp.
    pub team_rounds: u32,
    /// Team: seconds per team round (`TeamLightningTeamRoundDuration` = 90). Unused for CoOp.
    pub team_round_duration: f32,
}

impl CharadesConfig {
    /// CoOpClassic: 480 s game, 60 s per performer (VERIFIED `CharadesCoOpClassicGameRuleWrapper`).
    pub fn coop() -> Self {
        CharadesConfig { variant: Variant::CoOp, game_duration: 480.0, performer_turn: 60.0, team_rounds: 0, team_round_duration: 0.0 }
    }
    /// TeamLightning: 4 rounds, 90 s per team round, 45 s per performer (VERIFIED `CharadesTeamLightningGameRuleWrapper`).
    pub fn team() -> Self {
        CharadesConfig { variant: Variant::Team, game_duration: 0.0, performer_turn: 45.0, team_rounds: 4, team_round_duration: 90.0 }
    }
}

impl Default for CharadesConfig {
    fn default() -> Self {
        Self::coop()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    PreRound,
    Running,
    GameOver,
}

/// Charades as a [`GameMode`] on the room authority: the performer/timer state machine + the guess scorer. A
/// [`GameInput::Guess`] from a non-performer scores +1 (shared in CoOp, the guesser's team in Team) and advances to
/// the next performer; a performer's timer expiry advances with no point; CoOp ends when the 480 s clock expires,
/// Team after 4 rounds — highest score wins.
pub struct CharadesMode {
    config: CharadesConfig,
    /// players in join order
    players: Vec<PlayerId>,
    teams: HashMap<PlayerId, Team>,
    /// per-team rotation cursor (index into that team's players), so each team cycles through its performers
    perf_cursor: [usize; 2],
    performing_team: Team,
    performer: Option<PlayerId>,
    /// [team0, team1]; CoOp uses [0] as the shared pool
    score: [u32; 2],
    /// per-performer countdown
    turn_timer: f32,
    /// CoOp overall game clock
    game_timer: f32,
    /// Team per-team-round clock
    round_timer: f32,
    /// Team rounds completed
    rounds_done: u32,
    phase: Phase,
}

impl CharadesMode {
    pub fn new(config: CharadesConfig) -> Self {
        CharadesMode {
            config,
            players: Vec::new(),
            teams: HashMap::new(),
            perf_cursor: [0, 0],
            performing_team: 0,
            performer: None,
            score: [0, 0],
            turn_timer: config.performer_turn,
            game_timer: config.game_duration,
            round_timer: config.team_round_duration,
            rounds_done: 0,
            phase: Phase::PreRound,
        }
    }

    /// The player currently drawing (for the client to hand them the pen/word), if any.
    pub fn performer(&self) -> Option<PlayerId> {
        self.performer
    }

    fn team_players(&self, team: Team) -> Vec<PlayerId> {
        self.players.iter().copied().filter(|p| self.teams.get(p).copied() == Some(team)).collect()
    }

    /// Advance to the next performer on the performing team (cycling), reset the turn timer. Returns the new performer.
    fn next_performer(&mut self) -> Option<PlayerId> {
        let team = self.performing_team;
        let roster = self.team_players(team);
        if roster.is_empty() {
            self.performer = None;
            return None;
        }
        let cursor = &mut self.perf_cursor[team as usize];
        let pick = roster[*cursor % roster.len()];
        *cursor = (*cursor + 1) % roster.len();
        self.performer = Some(pick);
        self.turn_timer = self.config.performer_turn;
        Some(pick)
    }

    /// Highest-scoring team, or `None` for a draw (Team mode). CoOp has no winning team (the shared pool is the result).
    fn team_leader(&self) -> Option<Team> {
        match self.score[0].cmp(&self.score[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    }

    fn start(&mut self, out: &mut Vec<GameEvent>) {
        self.performing_team = 0;
        self.next_performer();
        self.phase = Phase::Running;
        out.push(GameEvent::Round { round: self.rounds_done + 1, phase: RoundPhase::Running, winner: None });
    }

    fn end(&mut self, out: &mut Vec<GameEvent>) {
        self.phase = Phase::GameOver;
        self.performer = None;
        let winner = match self.config.variant {
            Variant::CoOp => None, // co-op: the shared score is the result, no winning team
            Variant::Team => self.team_leader(),
        };
        // final score broadcast
        match self.config.variant {
            Variant::CoOp => out.push(GameEvent::Score { team: 0, score: self.score[0] }),
            Variant::Team => {
                out.push(GameEvent::Score { team: 0, score: self.score[0] });
                out.push(GameEvent::Score { team: 1, score: self.score[1] });
            }
        }
        out.push(GameEvent::Round { round: self.config.team_rounds.max(1), phase: RoundPhase::GameOver, winner });
    }
}

impl GameMode for CharadesMode {
    fn name(&self) -> &'static str {
        "Charades"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        let team: Team = match self.config.variant {
            Variant::CoOp => 0,
            Variant::Team => (self.players.len() % 2) as Team, // alternate teams
        };
        self.players.push(id);
        self.teams.insert(id, team);
        team
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.retain(|&p| p != id);
        self.teams.remove(&id);
        if self.performer == Some(id) {
            self.next_performer();
        }
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let GameInput::Guess { guesser } = input else { return };
        if self.phase != Phase::Running {
            return;
        }
        // the guesser must be a player and must NOT be the performer (the real RpcMasterRequestGuess guard)
        if !self.teams.contains_key(&guesser) || self.performer == Some(guesser) {
            return;
        }
        // score: CoOp → shared pool (team 0); Team → the guessing player's team
        let scoring_team = match self.config.variant {
            Variant::CoOp => 0,
            Variant::Team => self.teams.get(&guesser).copied().unwrap_or(0),
        };
        self.score[scoring_team as usize] += 1;
        out.push(GameEvent::Score { team: scoring_team, score: self.score[scoring_team as usize] });
        out.push(GameEvent::Cue("wordGuessed".into()));
        // advance to the next word/performer on the performing team
        self.next_performer();
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        match self.phase {
            Phase::PreRound => {
                if !self.players.is_empty() {
                    self.start(out);
                }
                return;
            }
            Phase::GameOver => return,
            Phase::Running => {}
        }

        // per-performer countdown: expiry advances the performer with NO point
        self.turn_timer -= dt;
        if self.turn_timer <= 0.0 {
            out.push(GameEvent::Cue("performerTimeUp".into()));
            self.next_performer();
        }

        match self.config.variant {
            Variant::CoOp => {
                self.game_timer -= dt;
                if self.game_timer <= 0.0 {
                    self.end(out);
                }
            }
            Variant::Team => {
                self.round_timer -= dt;
                if self.round_timer <= 0.0 {
                    self.rounds_done += 1;
                    if self.rounds_done >= self.config.team_rounds {
                        self.end(out);
                    } else {
                        // next team round: alternate the performing team, reset the clocks + performer
                        self.performing_team = 1 - self.performing_team;
                        self.round_timer = self.config.team_round_duration;
                        self.next_performer();
                        out.push(GameEvent::Round { round: self.rounds_done + 1, phase: RoundPhase::Running, winner: None });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coop_shared_score_most_words_before_the_clock() {
        // short game so the test is quick; the real default is 480 s / 60 s
        let cfg = CharadesConfig { variant: Variant::CoOp, game_duration: 10.0, performer_turn: 3.0, team_rounds: 0, team_round_duration: 0.0 };
        let mut m = CharadesMode::new(cfg);
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        m.tick(0.01, &mut out); // PreRound -> Running, first performer picked
        let performer = m.performer().unwrap();
        let guesser = if performer == 1 { 2 } else { 1 };

        // a guess by the performer themselves is rejected
        let mut bad = Vec::new();
        m.input(GameInput::Guess { guesser: performer }, &mut bad);
        assert!(!bad.iter().any(|e| matches!(e, GameEvent::Score { .. })), "the performer can't guess their own word");

        // three correct guesses by the other player → shared score 3
        for _ in 0..3 {
            let p = m.performer().unwrap();
            let g = if p == 1 { 2 } else { 1 };
            let mut ev = Vec::new();
            m.input(GameInput::Guess { guesser: g }, &mut ev);
            assert!(ev.iter().any(|e| matches!(e, GameEvent::Score { team: 0, .. })), "a correct guess scores the shared pool");
        }
        let _ = guesser;

        // run the game clock out
        let mut end = Vec::new();
        m.tick(cfg.game_duration, &mut end);
        let score = end.iter().rev().find_map(|e| if let GameEvent::Score { team: 0, score } = e { Some(*score) } else { None });
        assert_eq!(score, Some(3), "co-op shared score = words guessed before the clock");
        assert!(end.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::GameOver, winner: None, .. })), "co-op ends with no winning team");
    }

    #[test]
    fn team_highest_score_after_rounds_wins() {
        // 2 rounds, short clocks, 4 players (2 per team)
        let cfg = CharadesConfig { variant: Variant::Team, game_duration: 0.0, performer_turn: 5.0, team_rounds: 2, team_round_duration: 6.0 };
        let mut m = CharadesMode::new(cfg);
        for id in 1..=4 {
            m.add_player(id);
        }
        let mut out = Vec::new();
        m.tick(0.01, &mut out); // -> Running, team 0 performs first

        // round 1 (team 0 performs): a team-0 teammate (not the performer) guesses twice → team 0 +2
        for _ in 0..2 {
            let perf = m.performer().unwrap();
            // a non-performer on the performing team
            let g = m.team_players(0).into_iter().find(|&p| p != perf).unwrap();
            let mut ev = Vec::new();
            m.input(GameInput::Guess { guesser: g }, &mut ev);
            assert!(ev.iter().any(|e| matches!(e, GameEvent::Score { team: 0, .. })));
        }
        // end round 1 → round 2 (team 1 performs); team 1 guesses once → team 1 +1
        m.tick(cfg.team_round_duration, &mut Vec::new());
        let perf = m.performer().unwrap();
        let g = m.team_players(1).into_iter().find(|&p| p != perf).unwrap();
        m.input(GameInput::Guess { guesser: g }, &mut Vec::new());
        // end round 2 → game over
        let mut end = Vec::new();
        m.tick(cfg.team_round_duration, &mut end);
        let winner = end.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(0)), "team 0 (2 words) beats team 1 (1 word) after 2 rounds");
    }
}
