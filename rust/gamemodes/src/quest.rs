//! Quest, ported from the decompiled `QuestManager : GameRuleManager` (many room scenes loaded in sequence),
//! VERIFIED against the real binary (decompile lane, sweet-galileo_quest-recon.md). A CO-OP PvE CAMPAIGN: a sequence
//! of gameplay rooms with StoryState progression, WON by completing the campaign and LOST on a team WIPE, with a
//! co-op LOOT-points RATING over the top (not the win condition).
//!
//! - **Progression / WIN** (the game): each room's objectives complete → advance the StoryState → load the next room
//!   → … → the final room/boss. Completing the last objective wins the campaign (co-op — everyone wins together).
//! - **LOSE**: a team WIPE. Players have health and go DOWN (enemy / trap / friendly fire); a teammate can REVIVE them
//!   while the continue/life budget lasts. The whole team down with no continues left = wipe = campaign lost.
//! - **LOOT rating** (not the win): `AwardLoot` adds co-op loot points (enemy kills / collectibles); a death subtracts
//!   `death_penalty`, friendly fire subtracts `ff_penalty`. REAL per-campaign values (campaign 1: 1200/1200/50,
//!   campaign 2: 2000/2000/100). The loot is a score/rating, not the win.
//!
//! The campaign STRUCTURE (room sequence, per-room objectives, the life/continue budget) is `QuestCampaign` data —
//! gameplay-depth — so `story_states` + `continue_budget` are `// 1:1:` parameters; the per-campaign loot penalties
//! are the real verified numbers. Enemies/AI/gear/the many room scenes are gameplay-depth; this mode is the co-op
//! progression + wipe + loot scorer.

use std::collections::HashSet;

use crate::mode::{GameEvent, GameInput, GameMode, PlayerId, RoundPhase, Team};

/// Quest config. The loot penalties are REAL (per-campaign QuestManager prefabs). `story_states` (campaign length)
/// and `continue_budget` are `// 1:1:` — they live in the per-campaign `QuestCampaign` data (gameplay-depth), not the
/// manager, so they are parameters here until that data is read.
#[derive(Clone, Copy, Debug)]
pub struct QuestConfig {
    /// objectives / StoryStates to complete the campaign. 1:1: from the campaign's room sequence (gameplay-depth).
    pub story_states: u32,
    /// team continues before a full-team-down becomes a wipe. 1:1: from the campaign data (gameplay-depth).
    pub continue_budget: u32,
    /// loot subtracted on a death (`deathPointsPenalty`) — VERIFIED per campaign (1200 / 2000)
    pub death_penalty: i32,
    /// loot subtracted on friendly fire (`friendlyFirePointsPenalty`) — VERIFIED per campaign (1200 / 2000)
    pub ff_penalty: i32,
    /// loot per collectible (`collectible`) — VERIFIED per campaign (50 / 100); the client uses it for `AwardLoot`
    pub collectible_value: i32,
}

impl QuestConfig {
    /// Campaign 1 scoring (death/FF 1200, collectible 50). `story_states`/`continue_budget` are 1:1: placeholders.
    pub fn campaign1() -> Self {
        QuestConfig { story_states: 10, continue_budget: 3, death_penalty: 1200, ff_penalty: 1200, collectible_value: 50 }
    }
    /// Campaign 2 scoring (death/FF 2000, collectible 100). `story_states`/`continue_budget` are 1:1: placeholders.
    pub fn campaign2() -> Self {
        QuestConfig { story_states: 10, continue_budget: 3, death_penalty: 2000, ff_penalty: 2000, collectible_value: 100 }
    }
}

impl Default for QuestConfig {
    fn default() -> Self {
        Self::campaign1()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Running,
    Won,
    Lost,
}

/// Quest as a [`GameMode`] on the room authority: the co-op campaign progression + wipe + loot scorer.
pub struct QuestMode {
    config: QuestConfig,
    players: Vec<PlayerId>,
    down: HashSet<PlayerId>,
    story_state: u32,
    loot: i32,
    continues_left: u32,
    phase: Phase,
}

impl QuestMode {
    pub fn new(config: QuestConfig) -> Self {
        QuestMode {
            config,
            players: Vec::new(),
            down: HashSet::new(),
            story_state: 0,
            loot: 0,
            continues_left: config.continue_budget,
            phase: Phase::Running,
        }
    }

    /// The co-op loot rating so far (can be negative from penalties).
    pub fn loot(&self) -> i32 {
        self.loot
    }

    /// The current StoryState (objectives completed).
    pub fn story_state(&self) -> u32 {
        self.story_state
    }

    fn running(&self) -> bool {
        self.phase == Phase::Running
    }

    /// All joined players are currently down.
    fn team_wiped(&self) -> bool {
        !self.players.is_empty() && self.players.iter().all(|p| self.down.contains(p))
    }
}

impl GameMode for QuestMode {
    fn name(&self) -> &'static str {
        "Quest"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        self.players.push(id);
        0 // co-op: everyone on one side
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.retain(|&p| p != id);
        self.down.remove(&id);
    }

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        if !self.running() {
            return;
        }
        match input {
            GameInput::ObjectiveComplete => {
                self.story_state += 1;
                out.push(GameEvent::Cue(format!("storyState:{}", self.story_state)));
                if self.story_state >= self.config.story_states {
                    self.phase = Phase::Won;
                    out.push(GameEvent::Cue("questWon".into()));
                    out.push(GameEvent::Round { round: self.story_state, phase: RoundPhase::GameOver, winner: None });
                }
            }
            GameInput::PlayerDown { player, friendly_fire } => {
                if !self.players.contains(&player) {
                    return;
                }
                self.down.insert(player);
                // loot penalty: friendly fire vs a normal death
                self.loot -= if friendly_fire { self.config.ff_penalty } else { self.config.death_penalty };
                out.push(GameEvent::Cue(format!("playerDown:{player}")));
                if self.team_wiped() {
                    if self.continues_left > 0 {
                        // spend a continue: the team recovers (everyone revived)
                        self.continues_left -= 1;
                        self.down.clear();
                        out.push(GameEvent::Cue(format!("continue:{}", self.continues_left)));
                    } else {
                        self.phase = Phase::Lost;
                        out.push(GameEvent::Cue("questLost".into()));
                        out.push(GameEvent::Round { round: self.story_state, phase: RoundPhase::GameOver, winner: None });
                    }
                }
            }
            GameInput::PlayerRevived { player } => {
                self.down.remove(&player);
                out.push(GameEvent::Cue(format!("playerRevived:{player}")));
            }
            GameInput::AwardLoot { amount } => {
                self.loot += amount;
                out.push(GameEvent::PlayerScore { player: 0, score: self.loot }); // co-op pool (player 0 = the team)
            }
            _ => {} // other games' inputs
        }
    }

    fn tick(&mut self, _dt: f32, _out: &mut Vec<GameEvent>) {
        // no clock — Quest ends on campaign completion (win) or team wipe (lose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(states: u32, continues: u32) -> QuestConfig {
        QuestConfig { story_states: states, continue_budget: continues, death_penalty: 1200, ff_penalty: 1200, collectible_value: 50 }
    }

    #[test]
    fn completing_the_campaign_wins_co_op() {
        let mut m = QuestMode::new(cfg(3, 3));
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        for _ in 0..3 {
            m.input(GameInput::ObjectiveComplete, &mut out);
        }
        assert!(out.iter().any(|e| matches!(e, GameEvent::Cue(c) if c == "questWon")));
        assert!(out.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::GameOver, .. })));
        assert_eq!(m.story_state(), 3);
    }

    #[test]
    fn team_wipe_with_no_continues_loses_but_a_continue_recovers() {
        // 1 continue: the first full-team-down spends it and recovers; the second wipes and loses
        let mut m = QuestMode::new(cfg(10, 1));
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        // both down → wipe → spend the 1 continue, team revived
        m.input(GameInput::PlayerDown { player: 1, friendly_fire: false }, &mut out);
        m.input(GameInput::PlayerDown { player: 2, friendly_fire: false }, &mut out);
        assert!(out.iter().any(|e| matches!(e, GameEvent::Cue(c) if c.starts_with("continue"))), "a continue recovers the team");
        assert!(!out.iter().any(|e| matches!(e, GameEvent::Cue(c) if c == "questLost")));
        // both down again → no continues left → wipe → lost
        let mut out2 = Vec::new();
        m.input(GameInput::PlayerDown { player: 1, friendly_fire: false }, &mut out2);
        m.input(GameInput::PlayerDown { player: 2, friendly_fire: false }, &mut out2);
        assert!(out2.iter().any(|e| matches!(e, GameEvent::Cue(c) if c == "questLost")), "no continues left → wipe loses");
    }

    #[test]
    fn revive_clears_a_down_so_the_team_is_not_wiped() {
        let mut m = QuestMode::new(cfg(10, 0)); // no continues
        m.add_player(1);
        m.add_player(2);
        let mut out = Vec::new();
        m.input(GameInput::PlayerDown { player: 1, friendly_fire: false }, &mut out);
        m.input(GameInput::PlayerRevived { player: 1 }, &mut out); // revived before the team wipes
        m.input(GameInput::PlayerDown { player: 2, friendly_fire: false }, &mut out);
        assert!(!out.iter().any(|e| matches!(e, GameEvent::Cue(c) if c == "questLost")), "no wipe — player 1 was revived");
    }

    #[test]
    fn loot_accumulates_and_penalties_use_the_real_per_campaign_values() {
        let mut m = QuestMode::new(QuestConfig::campaign2()); // death/FF 2000, collectible 100
        m.add_player(1);
        let mut out = Vec::new();
        m.input(GameInput::AwardLoot { amount: 500 }, &mut out);
        m.input(GameInput::PlayerDown { player: 1, friendly_fire: true }, &mut out); // -2000 (FF)
        assert_eq!(m.loot(), 500 - 2000, "campaign-2 friendly-fire penalty is 2000");
    }
}
