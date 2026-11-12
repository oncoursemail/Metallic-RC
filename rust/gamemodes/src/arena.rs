//! Arena (Team Warzone), ported from the decompiled `ArenaManager : GameRuleManager` (scene level75 Arena_Hangar3 /
//! level76 Arena_Cyberjunk_City map pool), VERIFIED against the real binary (decompile lane,
//! sweet-galileo_arena-recon.md). Internally the manager is "Lasertag" (its wrapper is `LaserTagGameRuleWrapper`) — so
//! Arena IS the Team-Warzone / laser-tag game, a proper `GameRuleManager`, NOT the separate gadget/circuit path.
//!
//! Two teams (red/blue) fight the horde AND each other; every kill/pickup awards the KILLER's team from a REAL
//! kill-point table (both PvE and PvP count toward the same per-team pool): an enemy-team player +10, a teammate
//! (friendly fire) −10, a normal enemy +2, an elite enemy +10, an enemy friendly-fire −2, a loot chest +1. A team's
//! score is CLAMPED at 0 (friendly fire can't push it negative). First team to `ScoreToWin` = 400, or the higher team
//! score when `GameDuration` = 480 s expires, wins. The shop (`ArenaVendingMachine`) spends CREDITS, a SEPARATE
//! currency (collectibles = +50 credits) that never touches the win score, so it is not modelled here. The enemy
//! waves / AI / shop are gameplay-depth; this mode is the team kill-point scorer. All numbers are the real wrapper/
//! prefab values — no invented values.

use crate::mode::{GameEvent, GameInput, GameMode, KillKind, PlayerId, RoundPhase, Team};

/// Arena config — VERIFIED (LaserTag wrapper + ArenaManager prefab).
#[derive(Clone, Copy, Debug)]
pub struct ArenaConfig {
    /// kill-points to win (`ScoreToWin`) = 400
    pub score_to_win: u32,
    /// match length in seconds (`GameDuration`) = 480; highest team at expiry wins
    pub game_duration: f32,
}

impl Default for ArenaConfig {
    fn default() -> Self {
        ArenaConfig { score_to_win: 400, game_duration: 480.0 } // VERIFIED
    }
}

/// The real kill-point value for each kind (the `ArenaManager` scoring table).
fn points(kind: KillKind) -> i32 {
    match kind {
        KillKind::EnemyPlayer => 10,
        KillKind::Teammate => -10,
        KillKind::NormalEnemy => 2,
        KillKind::EliteEnemy => 10,
        KillKind::EnemyFriendlyFire => -2,
        KillKind::LootChest => 1,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Running,
    Over(Option<Team>),
}

/// Arena as a [`GameMode`] on the room authority: the two-team kill-point scorer with a match clock + score cap.
pub struct ArenaMode {
    config: ArenaConfig,
    /// per-team kill-point pool (red = 0, blue = 1), never below 0
    score: [u32; 2],
    clock: f32,
    phase: Phase,
}

impl ArenaMode {
    pub fn new(config: ArenaConfig) -> Self {
        ArenaMode { config, score: [0, 0], clock: config.game_duration, phase: Phase::Running }
    }

    fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }

    fn leader(&self) -> Option<Team> {
        match self.score[0].cmp(&self.score[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// Team score as a readable value (for the client).
    pub fn score(&self, team: Team) -> u32 {
        self.score.get(team as usize).copied().unwrap_or(0)
    }
}

impl GameMode for ArenaMode {
    fn name(&self) -> &'static str {
        "Arena"
    }

    fn add_player(&mut self, _id: PlayerId) -> Team {
        // teams are the two warzone sides; the room/lobby assigns red/blue. Default: balance by current head count is
        // the client's job — the mode just scores by the `team` the kill reports. Return red as a placeholder.
        0
    }

    fn remove_player(&mut self, _id: PlayerId) {}

    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let GameInput::Kill { team, kind } = input else { return };
        if !self.is_running() {
            return;
        }
        let t = team as usize;
        if t >= 2 {
            return;
        }
        // award the real kill-points, clamped so a team score never drops below 0
        let delta = points(kind);
        let new = (self.score[t] as i32 + delta).max(0) as u32;
        self.score[t] = new;
        out.push(GameEvent::Score { team, score: new });
        if new >= self.config.score_to_win {
            self.phase = Phase::Over(Some(team));
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner: Some(team) });
        }
    }

    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        if !self.is_running() {
            return;
        }
        self.clock -= dt;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            let winner = self.leader(); // higher team at expiry; None = draw
            self.phase = Phase::Over(winner);
            out.push(GameEvent::Round { round: 1, phase: RoundPhase::GameOver, winner });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score_after(kinds: &[(Team, KillKind)]) -> ArenaMode {
        let mut m = ArenaMode::new(ArenaConfig { score_to_win: 400, game_duration: 480.0 });
        let mut out = Vec::new();
        for &(team, kind) in kinds {
            m.input(GameInput::Kill { team, kind }, &mut out);
        }
        m
    }

    #[test]
    fn kill_point_table_is_the_real_values() {
        assert_eq!(points(KillKind::EnemyPlayer), 10);
        assert_eq!(points(KillKind::Teammate), -10);
        assert_eq!(points(KillKind::NormalEnemy), 2);
        assert_eq!(points(KillKind::EliteEnemy), 10);
        assert_eq!(points(KillKind::EnemyFriendlyFire), -2);
        assert_eq!(points(KillKind::LootChest), 1);
    }

    #[test]
    fn mixed_pve_and_pvp_accumulate_to_one_team_pool() {
        // team 0: a normal (+2), an elite (+10), an enemy player (+10), a loot chest (+1) = 23
        let m = score_after(&[(0, KillKind::NormalEnemy), (0, KillKind::EliteEnemy), (0, KillKind::EnemyPlayer), (0, KillKind::LootChest)]);
        assert_eq!(m.score(0), 23, "PvE + PvP + loot all accumulate to the team pool");
    }

    #[test]
    fn friendly_fire_is_clamped_at_zero() {
        // a single teammate kill (−10) on a fresh team clamps to 0, not negative
        let m = score_after(&[(0, KillKind::Teammate)]);
        assert_eq!(m.score(0), 0, "friendly fire can't push a team below 0");
        // +10 then −10 → back to 0 (not negative, not stuck)
        let m2 = score_after(&[(0, KillKind::EnemyPlayer), (0, KillKind::Teammate)]);
        assert_eq!(m2.score(0), 0);
    }

    #[test]
    fn first_team_to_the_cap_wins() {
        let mut m = ArenaMode::new(ArenaConfig { score_to_win: 20, game_duration: 480.0 });
        let mut out = Vec::new();
        for _ in 0..2 {
            m.input(GameInput::Kill { team: 1, kind: KillKind::EnemyPlayer }, &mut out); // +10 each → 20
        }
        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(1)), "team 1 wins at the kill-point cap");
    }

    #[test]
    fn higher_team_wins_when_the_clock_expires() {
        let mut m = ArenaMode::new(ArenaConfig { score_to_win: 400, game_duration: 10.0 });
        let mut out = Vec::new();
        m.input(GameInput::Kill { team: 0, kind: KillKind::EliteEnemy }, &mut out); // team 0: 10
        m.input(GameInput::Kill { team: 1, kind: KillKind::NormalEnemy }, &mut out); // team 1: 2
        m.tick(10.0, &mut out);
        let winner = out.iter().find_map(|e| if let GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } = e { Some(*winner) } else { None });
        assert_eq!(winner, Some(Some(0)), "the higher team wins at time");
    }
}
