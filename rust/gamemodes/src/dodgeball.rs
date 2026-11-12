//! Dodgeball, ported from the decompiled C# `DodgeballManager : GameRuleManager` (@dump.cs:5118) and
//! `Dodgeball : Ball : RRNetworkBehavior` (@5010). Built-in RR Original games are hardcoded C# rule managers, not
//! loadable circuits — so this is a faithful reimplementation of the manager's rules + the ball's event hooks, kept
//! engine-agnostic: the client/authoritative server feeds it ball events (thrown / hit / caught) and ticks it, and
//! implements [`DodgeballEffects`] (eliminate, respawn, score, SFX/VFX) over `rr-net`.
//!
//! RULES SOURCE (per the integrator's steer, since the IL2CPP dump is signatures-only and body recovery is noisy):
//! ported from the descriptive field/method NAMES + types, the authoritative CONFIG numbers from the prefab, and the
//! well-understood Dodgeball ruleset — a ROUND is last-team-standing: players are eliminated when a thrown ball hits
//! them *unless it's caught* (a catch puts the THROWER out); when only one team has live players the round ends and
//! that team scores a round-win; first team to `ScoreToWin` round-wins takes the match; players revive between
//! rounds. Decisions that a cleanly-recovered method body would pin down further are marked `// 1:1:`. Config values
//! are placeholders until the real `DodgeballGameRuleWrapper`/`DodgeballManager` prefab numbers are decoded.

use std::collections::HashMap;

pub type PlayerId = u32;
pub type Team = u8;

/// Game-rule config (`DodgeballGameRuleData` @29720 + `defaultScoreToWin`). Placeholder numbers until the real prefab
/// values are decoded.
#[derive(Clone, Debug)]
pub struct DodgeballConfig {
    /// round-wins needed to take the match (`ScoreToWin` / `defaultScoreToWin`)
    pub score_to_win: u32,
    pub pre_round_duration: f32,
    pub post_round_duration: f32,
}

impl Default for DodgeballConfig {
    fn default() -> Self {
        // 1:1 decoded from the prefabs + dump.cs (decompile lane, sweet-galileo_dodgeball-recon.md §EXACT CONFIG):
        //   ScoreToWin = DodgeballGameRuleWrapper.defaultScoreToWin = 3
        //   PreRoundDuration / PostRoundDuration = DodgeballManager consts 3f / 3f (the third const, 0.35f, is a
        //   separate threshold whose exact use needs the method body — see on_player_hit).
        DodgeballConfig { score_to_win: 3, pre_round_duration: 3.0, post_round_duration: 3.0 }
    }
}

/// The real `DodgeballBall_phys` PhysicsMaterial (decoded 1:1): the ball bounces and grips the floor the same on every
/// client. Used by the client/server when it simulates the ball (friction/bounce). The ball's Rigidbody mass/drag and
/// the release throw velocity live in `Dodgeball.OnPostRelease` (a method body not yet recovered), so they're not here.
pub struct BallPhysics {
    pub dynamic_friction: f32,
    pub static_friction: f32,
    pub bounciness: f32,
}
pub const BALL_PHYSICS: BallPhysics = BallPhysics { dynamic_friction: 0.6, static_friction: 0.6, bounciness: 0.6 };

/// The round/match lifecycle (`IsRoundRunning` + Pre/PostRoundDuration). The `f32` is seconds left in a timed phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    PreRound(f32),
    Running,
    PostRound(f32),
    GameOver(Team),
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
    fn match_over(&mut self, _winner: Team) {}
    fn player_out(&mut self, _player: PlayerId) {}
    fn player_respawned(&mut self, _player: PlayerId) {}
    fn score_changed(&mut self, _team: Team, _round_wins: u32) {}
    fn sfx(&mut self, _name: &str) {}
}

/// Port of `DodgeballManager`: rounds, score-to-win, last-team-standing elimination, timing. Consumes ball events
/// (the ball gadget detects hits/catches and fires `DodgeballPlayerHitEvent` / `DodgeballCatchEvent`).
pub struct DodgeballManager {
    pub config: DodgeballConfig,
    pub phase: Phase,
    pub round: u32,
    pub players: HashMap<PlayerId, PlayerState>,
    /// round-wins per team
    pub team_score: Vec<u32>,
}

impl DodgeballManager {
    /// `DodgeballManager.Start`: begins in the pre-round countdown of round 1.
    pub fn new(config: DodgeballConfig) -> Self {
        let pre = config.pre_round_duration;
        DodgeballManager { config, phase: Phase::PreRound(pre), round: 1, players: HashMap::new(), team_score: vec![0, 0] }
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

    /// `DodgeballManager.Update`: advances the round timer; the timed phases are unambiguous.
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
            Phase::PostRound(t) => {
                let t = t - dt;
                if t <= 0.0 {
                    self.next_round(fx);
                } else {
                    self.phase = Phase::PostRound(t);
                }
            }
            Phase::Running | Phase::GameOver(_) => {}
        }
    }

    /// `KTLYQVYAOGB` round-begin: everyone revives, round goes live.
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

    fn next_round(&mut self, fx: &mut dyn DodgeballEffects) {
        self.round += 1;
        self.phase = Phase::PreRound(self.config.pre_round_duration);
        let _ = fx;
    }

    /// A ball thrown by `thrower` hit `victim` (ball's `DodgeballPlayerHitEvent`). The ball does the hit detection
    /// (`OnCollisionEnter`); a hit (not a catch) puts the victim out.
    pub fn on_player_hit(&mut self, victim: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        // 1:1: confirm friendly-fire / self-hit (ball OWEMYSVBCGK). For now a hit between live cross-team players counts.
        let (Some(&PlayerState { team: vt, alive: true }), Some(tt)) =
            (self.players.get(&victim), self.players.get(&thrower).map(|p| p.team))
        else {
            return;
        };
        if vt == tt {
            return;
        }
        fx.sfx("playerImpact");
        self.eliminate(victim, fx);
    }

    /// `catcher` caught a ball thrown by `thrower` (ball's `DodgeballCatchEvent`): the thrower is out.
    pub fn on_catch(&mut self, _catcher: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        // 1:1: confirm the catch rule (thrower out? a teammate also revived?) against the ball/manager body
        self.eliminate(thrower, fx);
    }

    fn eliminate(&mut self, player: PlayerId, fx: &mut dyn DodgeballEffects) {
        if self.players.get(&player).map(|p| p.alive) != Some(true) {
            return;
        }
        self.players.get_mut(&player).unwrap().alive = false;
        fx.player_out(player);
        fx.sfx("playerOut");
        self.check_round_end(fx);
    }

    /// When one team (or none) still has live players, the round is over: the surviving team scores a round-win.
    fn check_round_end(&mut self, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() {
            return;
        }
        let mut live: Vec<Team> = self.players.values().filter(|p| p.alive).map(|p| p.team).collect();
        live.sort_unstable();
        live.dedup();
        if live.len() <= 1 {
            // 1:1: a round timeout (most-players-left wins) may also exist; not modeled until confirmed
            self.round_won(live.first().copied(), fx);
        }
    }

    fn round_won(&mut self, winner: Option<Team>, fx: &mut dyn DodgeballEffects) {
        if let Some(t) = winner {
            self.team_score[t as usize] += 1;
            let score = self.team_score[t as usize];
            fx.score_changed(t, score);
            fx.sfx("playerScore");
            if score >= self.config.score_to_win {
                self.phase = Phase::GameOver(t);
                fx.round_over(Some(t));
                fx.match_over(t);
                return;
            }
        }
        self.phase = Phase::PostRound(self.config.post_round_duration);
        fx.round_over(winner);
        fx.sfx("roundOver");
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
            self.log.push(format!("round_over {w:?}"));
        }
        fn match_over(&mut self, w: Team) {
            self.log.push(format!("match_over {w}"));
        }
        fn player_out(&mut self, p: PlayerId) {
            self.log.push(format!("out {p}"));
        }
        fn score_changed(&mut self, t: Team, s: u32) {
            self.log.push(format!("score t{t}={s}"));
        }
    }

    #[test]
    fn round_is_last_team_standing() {
        let cfg = DodgeballConfig { score_to_win: 2, pre_round_duration: 1.0, post_round_duration: 1.0 };
        let mut m = DodgeballManager::new(cfg);
        m.add_player(1, 0);
        m.add_player(2, 0);
        m.add_player(3, 1);
        let mut fx = Rec::default();

        m.tick(1.0, &mut fx); // pre-round elapses -> running
        assert!(m.is_round_running());

        // team 0 eliminates team 1's only player -> team 0 wins the round (score 1), not a win yet
        m.on_player_hit(3, 1, &mut fx);
        assert_eq!(m.team_score[0], 1);
        assert_eq!(m.phase, Phase::PostRound(1.0));

        // next round: everyone revives
        m.tick(1.0, &mut fx); // post-round -> pre-round(next)
        assert_eq!(m.round, 2);
        m.tick(1.0, &mut fx); // pre-round -> running
        assert!(m.players[&3].alive);

        // team 0 wins round 2 -> reaches score_to_win=2 -> match over
        m.on_player_hit(3, 1, &mut fx);
        assert_eq!(m.team_score[0], 2);
        assert_eq!(m.phase, Phase::GameOver(0));
        assert!(fx.log.contains(&"match_over 0".to_string()));
    }

    #[test]
    fn a_catch_eliminates_the_thrower_and_can_end_the_round() {
        let mut m = DodgeballManager::new(DodgeballConfig { pre_round_duration: 0.0, ..Default::default() });
        m.add_player(1, 0);
        m.add_player(2, 1);
        let mut fx = Rec::default();
        m.tick(0.1, &mut fx); // running
        m.on_catch(2, 1, &mut fx); // p2 catches p1's throw -> p1 (team 0) out -> team 1 last standing -> round win
        assert_eq!(m.players[&1].alive, false);
        assert_eq!(m.team_score[1], 1);
    }
}
