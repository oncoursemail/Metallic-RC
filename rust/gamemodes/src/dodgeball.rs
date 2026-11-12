//! Dodgeball, ported from the decompiled C# `DodgeballManager : GameRuleManager` (@dump.cs:5118) and
//! `Dodgeball : Ball : RRNetworkBehavior` (@5010). Built-in RR Original games are hardcoded C# rule managers, not
//! loadable circuits — so this is a faithful reimplementation of the manager's rules + the ball's event hooks, kept
//! engine-agnostic: the client/authoritative server feeds it ball events (thrown / hit / caught) and ticks it, and
//! implements [`DodgeballEffects`] (eliminate, respawn, score, SFX/VFX) over `rr-net`.
//!
//! GROUNDED vs PENDING: the manager's fields and lifecycle (ScoreToWin, Pre/PostRoundDuration, IsRoundRunning, round
//! begin/end, player-out, Update tick) are from the dump; the round-phase timer here is unambiguous. The exact
//! rule DECISIONS — who scores on a hit, catch-vs-out, respawn-vs-eliminate, teams-vs-FFA, the precise win/round-end
//! condition — are marked `// 1:1:` and will be reconciled against the decompiled method BODIES (the signatures dump
//! lacks them); this is a runnable first pass, not the final 1:1 ruleset. Config values are placeholders until the
//! real `DodgeballGameRuleWrapper`/`DodgeballManager` prefab values are pulled.

use std::collections::HashMap;

pub type PlayerId = u32;
pub type Team = u8;

/// Serialized game-rule config (`DodgeballGameRuleData` @29720 + `defaultScoreToWin`). Placeholder numbers until the
/// real prefab values are decoded.
#[derive(Clone, Debug)]
pub struct DodgeballConfig {
    pub score_to_win: u32,
    pub pre_round_duration: f32,
    pub post_round_duration: f32,
}

impl Default for DodgeballConfig {
    fn default() -> Self {
        // 1:1: replace with the real DodgeballGameRuleWrapper.defaultScoreToWin + DodgeballManager.Pre/PostRoundDuration
        DodgeballConfig { score_to_win: 7, pre_round_duration: 3.0, post_round_duration: 5.0 }
    }
}

/// The round lifecycle (`IsRoundRunning` + Pre/PostRoundDuration). `f32` is the seconds remaining in the timed phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    PreRound(f32),
    Running,
    PostRound(f32),
}

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub team: Team,
    pub alive: bool,
}

/// Side effects the manager emits; the client/authoritative server implements them (networked elimination/respawn,
/// scoreboard, the manager's SFX/VFX — roundStartSFX, playerOutSFX/VO, playerImpactVFX, …).
pub trait DodgeballEffects {
    fn round_started(&mut self, _round: u32) {}
    fn round_over(&mut self, _winner: Option<Team>) {}
    fn player_out(&mut self, _player: PlayerId) {}
    fn player_respawned(&mut self, _player: PlayerId) {}
    fn score_changed(&mut self, _team: Team, _score: u32) {}
    fn sfx(&mut self, _name: &str) {}
}

/// Port of `DodgeballManager`: rounds, score-to-win, elimination, timing. Consumes ball events (the ball gadget
/// detects hits/catches and fires `DodgeballPlayerHitEvent` / `DodgeballCatchEvent`; the manager scores).
pub struct DodgeballManager {
    pub config: DodgeballConfig,
    pub phase: Phase,
    pub round: u32,
    pub players: HashMap<PlayerId, PlayerState>,
    /// per-team score (index by `Team`); two teams by default
    pub team_score: Vec<u32>,
}

impl DodgeballManager {
    /// `DodgeballManager.Start`: begins in the pre-round countdown.
    pub fn new(config: DodgeballConfig) -> Self {
        let pre = config.pre_round_duration;
        DodgeballManager {
            config,
            phase: Phase::PreRound(pre),
            round: 1,
            players: HashMap::new(),
            team_score: vec![0, 0],
        }
    }

    pub fn add_player(&mut self, id: PlayerId, team: Team) {
        self.players.insert(id, PlayerState { team, alive: true });
        if team as usize >= self.team_score.len() {
            self.team_score.resize(team as usize + 1, 0);
        }
    }

    pub fn is_round_running(&self) -> bool {
        self.phase == Phase::Running
    }

    /// `DodgeballManager.Update`: advances the round timer; the timed phases (`PreRound`/`PostRound`) are unambiguous.
    pub fn tick(&mut self, dt: f32, fx: &mut dyn DodgeballEffects) {
        match self.phase {
            Phase::PreRound(t) => {
                let t = t - dt;
                if t <= 0.0 {
                    self.begin_round(fx);
                } else {
                    self.phase = Phase::PreRound(t);
                }
            }
            Phase::Running => {}
            Phase::PostRound(t) => {
                let t = t - dt;
                if t <= 0.0 {
                    self.next_round(fx);
                } else {
                    self.phase = Phase::PostRound(t);
                }
            }
        }
    }

    /// `KTLYQVYAOGB` round-begin override: everyone revives, round goes live.
    fn begin_round(&mut self, fx: &mut dyn DodgeballEffects) {
        for (id, p) in self.players.iter_mut() {
            if !p.alive {
                p.alive = true;
                fx.player_respawned(*id);
            }
        }
        self.phase = Phase::Running;
        fx.round_started(self.round);
        fx.sfx("roundStart");
    }

    /// `JIKUUNYQMWA` round-end override: declare a winner and enter the post-round cooldown.
    fn end_round(&mut self, winner: Option<Team>, fx: &mut dyn DodgeballEffects) {
        self.phase = Phase::PostRound(self.config.post_round_duration);
        fx.round_over(winner);
        fx.sfx("roundOver");
    }

    fn next_round(&mut self, fx: &mut dyn DodgeballEffects) {
        self.round += 1;
        self.phase = Phase::PreRound(self.config.pre_round_duration);
        // 1:1: confirm whether scores carry across rounds or reset (manager body)
        let _ = fx;
    }

    /// A ball thrown by `thrower` hit `victim` (ball's `DodgeballPlayerHitEvent` -> manager). The ball does the hit
    /// detection (`OnCollisionEnter`); the manager applies the rule.
    pub fn on_player_hit(&mut self, victim: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        // 1:1: confirm friendly-fire / self-hit handling (ball's OWEMYSVBCGK) — for now, a live cross-team hit scores.
        let (Some(&PlayerState { team: vt, alive: true }), Some(tt)) =
            (self.players.get(&victim), self.players.get(&thrower).map(|p| p.team))
        else {
            return;
        };
        if vt == tt {
            return;
        }
        self.eliminate(victim, fx);
        fx.sfx("playerImpact");
        self.award(tt, fx);
    }

    /// `catcher` caught a ball thrown by `thrower` (ball's `DodgeballCatchEvent`): the thrower is out.
    pub fn on_catch(&mut self, _catcher: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        // 1:1: confirm the catch rule (thrower out? catcher's teammate revived? scoring) against the ball/manager body
        self.eliminate(thrower, fx);
    }

    fn eliminate(&mut self, player: PlayerId, fx: &mut dyn DodgeballEffects) {
        if let Some(p) = self.players.get_mut(&player) {
            if p.alive {
                p.alive = false;
                fx.player_out(player);
                fx.sfx("playerOut");
            }
        }
    }

    fn award(&mut self, team: Team, fx: &mut dyn DodgeballEffects) {
        let s = &mut self.team_score[team as usize];
        *s += 1;
        let score = *s;
        fx.score_changed(team, score);
        fx.sfx("playerScore");
        // 1:1: ScoreToWin ends the game — confirm whether it ends the round (then PostRound) or the whole match
        if score >= self.config.score_to_win {
            self.end_round(Some(team), fx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Rec {
        log: Vec<String>,
    }
    impl DodgeballEffects for Rec {
        fn round_started(&mut self, r: u32) {
            self.log.push(format!("start {r}"));
        }
        fn round_over(&mut self, w: Option<Team>) {
            self.log.push(format!("over {w:?}"));
        }
        fn player_out(&mut self, p: PlayerId) {
            self.log.push(format!("out {p}"));
        }
        fn score_changed(&mut self, t: Team, s: u32) {
            self.log.push(format!("score t{t}={s}"));
        }
    }

    #[test]
    fn round_timer_and_scoring() {
        let cfg = DodgeballConfig { score_to_win: 2, pre_round_duration: 2.0, post_round_duration: 1.0 };
        let mut m = DodgeballManager::new(cfg);
        m.add_player(1, 0);
        m.add_player(2, 1);
        let mut fx = Rec::default();

        // pre-round: not running until the countdown elapses
        m.tick(1.0, &mut fx);
        assert!(!m.is_round_running());
        m.tick(1.5, &mut fx); // crosses 2.0 -> begin round
        assert!(m.is_round_running());
        assert!(fx.log.contains(&"start 1".to_string()));

        // team 0 hits team 1's player twice (via two live players) -> reaches score_to_win=2 -> round over
        m.on_player_hit(2, 1, &mut fx); // p2 out, team 0 score 1
        assert_eq!(m.players[&2].alive, false);
        assert_eq!(m.team_score[0], 1);
        m.add_player(3, 1); // another opponent
        m.on_player_hit(3, 1, &mut fx); // team 0 score 2 -> win
        assert_eq!(m.team_score[0], 2);
        assert_eq!(m.phase, Phase::PostRound(1.0));
        assert!(fx.log.contains(&"over Some(0)".to_string()));

        // post-round cooldown -> next round, round goes back to pre-round and players revive
        m.tick(1.0, &mut fx);
        assert_eq!(m.round, 2);
        assert!(matches!(m.phase, Phase::PreRound(_)));
        m.tick(2.0, &mut fx);
        assert!(m.is_round_running());
        assert!(m.players[&2].alive); // revived at round begin
    }

    #[test]
    fn catch_puts_thrower_out() {
        let mut m = DodgeballManager::new(DodgeballConfig { pre_round_duration: 0.0, ..Default::default() });
        m.add_player(1, 0);
        m.add_player(2, 1);
        let mut fx = Rec::default();
        m.tick(0.1, &mut fx); // pre-round 0 -> running
        assert!(m.is_round_running());
        m.on_catch(2, 1, &mut fx); // p2 catches p1's throw -> p1 out
        assert_eq!(m.players[&1].alive, false);
    }
}
