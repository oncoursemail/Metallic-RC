//! BattleRoyale, ported from the decompiled `BattleRoyaleManager : GameRuleManager` (scene level78 RecRoyale_Frontier),
//! VERIFIED against the real binary (decompile lane, sweet-galileo_battleroyale-recon.md). PURELY last-standing — NO
//! score, NO timer. Players are eliminated (permanently — from the shrinking killzone or combat), and the master
//! assigns each a RANK in elimination ORDER: the first out gets the highest rank number, and the LAST standing is
//! rank 1 = the winner. Two modes (the wrapper's only field is `modeType`): SOLOS (per-player) and SQUADS (per-team —
//! a team is out when all its members are out; the last team with a living player wins).
//!
//! The mode is the elimination-order scorer: an [`GameInput::Eliminated`] marks a player out, assigns their rank =
//! the players remaining at that moment, and ends the match when one player (Solos) or one team (Squads) remains. The
//! shrinking killzone + gear/loot + the big map are gameplay-depth (the death TRIGGER is client-side); this mode just
//! needs the elimination events.

use std::collections::HashMap;

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, RoundPhase, Team};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrVariant {
    Solos,
    Squads,
}

/// BattleRoyale config — the wrapper's only field is `modeType` (SOLOS=0 / SQUADS=1). No score, no duration.
#[derive(Clone, Copy, Debug)]
pub struct BattleRoyaleConfig {
    pub variant: BrVariant,
}

impl BattleRoyaleConfig {
    pub fn solos() -> Self {
        BattleRoyaleConfig { variant: BrVariant::Solos }
    }
    pub fn squads() -> Self {
        BattleRoyaleConfig { variant: BrVariant::Squads }
    }
}

impl Default for BattleRoyaleConfig {
    fn default() -> Self {
        Self::solos()
    }
}

struct Entry {
    team: Team,
    alive: bool,
    rank: Option<u32>,
}

/// BattleRoyale as a [`GameMode`] on the room authority: the last-standing / elimination-rank scorer.
pub struct BattleRoyaleMode {
    config: BattleRoyaleConfig,
    players: Vec<PlayerId>,
    entries: HashMap<PlayerId, Entry>,
    joined: u32,
    over: bool,
}

impl BattleRoyaleMode {
    pub fn new(config: BattleRoyaleConfig) -> Self {
        BattleRoyaleMode { config, players: Vec::new(), entries: HashMap::new(), joined: 0, over: false }
    }

    fn alive_count(&self) -> u32 {
        self.entries.values().filter(|e| e.alive).count() as u32
    }

    /// Distinct teams that still have at least one living member (Squads).
    fn living_teams(&self) -> Vec<Team> {
        let mut out: Vec<Team> = Vec::new();
        for e in self.entries.values().filter(|e| e.alive) {
            if !out.contains(&e.team) {
                out.push(e.team);
            }
        }
        out
    }

    /// A player's elimination rank (1 = winner), if assigned.
    pub fn rank_of(&self, player: PlayerId) -> Option<u32> {
        self.entries.get(&player).and_then(|e| e.rank)
    }
}

impl GameMode for BattleRoyaleMode {
    fn name(&self) -> &'static str {
        "BattleRoyale"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        // SOLOS: each player is their own "team"; SQUADS: alternate into two squads (the lobby picks squads in the
        // real game — the mode scores by whatever team an entry carries).
        let team = match self.config.variant {
            BrVariant::Solos => self.joined as Team,
            BrVariant::Squads => (self.joined % 2) as Team,
        };
        self.joined += 1;
        self.players.push(id);
        self.entries.insert(id, Entry { team, alive: true, rank: None });
        team
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.retain(|&p| p != id);
        self.entries.remove(&id);
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let GameInput::Eliminated { player } = input else { return };
        if self.over {
            return;
        }
        // mark the player out, assigning their rank = the players remaining at this moment (first out = highest rank)
        let remaining = self.alive_count();
        match self.entries.get_mut(&player) {
            Some(e) if e.alive => {
                e.alive = false;
                e.rank = Some(remaining);
            }
            _ => return, // unknown or already out
        }
        out.push(GameEvent::Cue(format!("eliminated:{player}:rank{remaining}")));

        match self.config.variant {
            BrVariant::Solos => {
                if self.alive_count() == 1 {
                    // the sole survivor is rank 1 = winner
                    if let Some(&survivor) = self.players.iter().find(|p| self.entries.get(p).is_some_and(|e| e.alive)) {
                        if let Some(e) = self.entries.get_mut(&survivor) {
                            e.rank = Some(1);
                        }
                        self.over = true;
                        out.push(GameEvent::MatchOver { winner: survivor });
                    }
                }
            }
            BrVariant::Squads => {
                let living = self.living_teams();
                if living.len() == 1 {
                    self.over = true;
                    out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner: Some(living[0]) });
                }
            }
        }
    }

    fn tick(&mut self, _dt: f32, _out: &mut Vec<GameEvent>) {
        // no timer — BattleRoyale ends only when one player / team remains (the shrinking zone forces convergence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solos_last_standing_wins_and_ranks_are_elimination_order() {
        let mut m = BattleRoyaleMode::new(BattleRoyaleConfig::solos());
        for id in 1..=3 {
            m.add_player(id);
        }
        let mut out = Vec::new();
        m.input(GameInput::Eliminated { player: 1 }, &mut out); // first out: 3 remaining → rank 3
        m.input(GameInput::Eliminated { player: 2 }, &mut out); // 2 remaining → rank 2; now 1 left → p3 wins

        let winner = out.iter().rev().find_map(|e| if let GameEvent::MatchOver { winner } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(3), "the last player standing wins");
        assert_eq!(m.rank_of(1), Some(3), "first out = highest rank");
        assert_eq!(m.rank_of(2), Some(2));
        assert_eq!(m.rank_of(3), Some(1), "winner = rank 1");
    }

    #[test]
    fn squads_win_when_the_last_team_stands() {
        let mut m = BattleRoyaleMode::new(BattleRoyaleConfig::squads());
        // players 1,3 → team 0 ; players 2,4 → team 1
        let t_a = m.add_player(1);
        let t_b = m.add_player(2);
        m.add_player(3);
        m.add_player(4);
        assert_ne!(t_a, t_b);

        let mut out = Vec::new();
        // eliminate both of team 1 (players 2 and 4); team 0 (1,3) still has living members → team 0 wins
        m.input(GameInput::Eliminated { player: 2 }, &mut out);
        assert!(!out.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::GameOver, .. })), "one down, team still alive");
        m.input(GameInput::Eliminated { player: 4 }, &mut out);
        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(t_a)), "the last team standing wins");
    }

    #[test]
    fn an_already_out_player_cannot_be_eliminated_again() {
        let mut m = BattleRoyaleMode::new(BattleRoyaleConfig::solos());
        for id in 1..=3 {
            m.add_player(id);
        }
        let mut out = Vec::new();
        m.input(GameInput::Eliminated { player: 1 }, &mut out);
        let before = m.rank_of(1);
        m.input(GameInput::Eliminated { player: 1 }, &mut out); // duplicate ignored
        assert_eq!(m.rank_of(1), before, "rank isn't reassigned on a duplicate elimination");
    }
}
