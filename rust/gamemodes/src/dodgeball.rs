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

/// The ball's physics, VERIFIED against the real binary (decompile lane). The thrown ball is the thrower's networked
/// prop, simulated client-side by the normal prop physics; these are its 1:1 values. Throw speed is NOT scripted — the
/// ball keeps the hand's release velocity (`OnPostRelease` just marks it live + sets `thrownBy`).
///   - DodgeballBall_phys: dynamicFriction 0.6, staticFriction 0.6, bounciness 0.6; frictionCombine = Maximum,
///     bounceCombine = Minimum (two materials take the max friction, the min bounciness).
///   - Project gravity = (0, -9.81, 0) (Unity default, from globalgamemanagers DynamicsManager); BounceThreshold 2.
///   - Ball Rigidbody: mass 1.0, drag 0, angularDrag 0.05, useGravity true, Discrete collision.
/// 1:1: the ball Rigidbody mass/drag are Unity defaults (the exact spawn-prefab values weren't in the decoded bundles;
/// low-stakes since throw speed is the release velocity, so mass only affects post-bounce inertia) — exact TBD.
pub struct BallPhysics {
    pub dynamic_friction: f32,
    pub static_friction: f32,
    pub bounciness: f32,
    pub mass: f32,
    pub gravity: f32,
}
pub const BALL_PHYSICS: BallPhysics =
    BallPhysics { dynamic_friction: 0.6, static_friction: 0.6, bounciness: 0.6, mass: 1.0, gravity: 9.81 };

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

    /// A ball thrown by `thrower` hit `victim` (the ball's owner raised the hit on its `OnCollisionEnter`). This is
    /// `DodgeballManager.IsValidHit` (@0xD3DFC0, VERIFIED): the hit counts only if the round is active, both players
    /// exist and are still in (not out), it is not a self-hit, and they are on different teams (no friendly fire). A
    /// valid hit puts the victim out.
    pub fn on_player_hit(&mut self, victim: PlayerId, thrower: PlayerId, fx: &mut dyn DodgeballEffects) {
        if !self.is_round_running() || victim == thrower {
            return; // round must be running; a self-hit never counts
        }
        // both players present, and NEITHER already out (IsValidHit checks the thrower is in, too)
        let (Some(&PlayerState { team: vt, alive: true }), Some(&PlayerState { team: tt, alive: true })) =
            (self.players.get(&victim), self.players.get(&thrower))
        else {
            return;
        };
        if vt == tt {
            return; // same team: no friendly fire
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

// ---- the authority GameMode: rules only (the master validates requests; it does NOT simulate the ball) ----

use crate::mode::{GameEvent, GameInput, GameMode, RoundPhase};

/// Dodgeball as a [`GameMode`] on the room authority (the master client). It wraps [`DodgeballManager`] and is a pure
/// validator + scorer: a client whose thrown ball hit someone sends a [`GameInput::Hit`] (Rec Room's
/// `RpcMasterRequestPlayerOut`), a catcher sends [`GameInput::Catch`] (`RpcMasterRequestPlayerCatch`), and this runs
/// the verified `IsValidHit` rule and the round/score logic, emitting [`GameEvent`]s to broadcast (`RpcOnPlayerOut`,
/// score, round). The ball itself is the thrower's own networked prop (it keeps the hand release velocity and syncs
/// through object ownership) — the authority never simulates it. Config is the decompile lane's 1:1 numbers.
pub struct DodgeballMode {
    mgr: DodgeballManager,
    joined: u32,
}

impl DodgeballMode {
    pub fn new(config: DodgeballConfig) -> Self {
        DodgeballMode { mgr: DodgeballManager::new(config), joined: 0 }
    }
}

/// Collects the manager's effects into broadcastable [`GameEvent`]s.
struct Collect<'a> {
    out: &'a mut Vec<GameEvent>,
    round: u32,
}
impl DodgeballEffects for Collect<'_> {
    fn round_started(&mut self, r: u32) {
        self.round = r;
        self.out.push(GameEvent::Round { round: r, phase: RoundPhase::Running, winner: None });
    }
    fn round_over(&mut self, w: Option<Team>) {
        self.out.push(GameEvent::Round { round: self.round, phase: RoundPhase::PostRound, winner: w });
    }
    fn match_over(&mut self, w: Team) {
        self.out.push(GameEvent::Round { round: self.round, phase: RoundPhase::GameOver, winner: Some(w) });
    }
    fn player_out(&mut self, p: PlayerId) {
        self.out.push(GameEvent::PlayerOut(p));
    }
    fn player_respawned(&mut self, p: PlayerId) {
        self.out.push(GameEvent::PlayerRespawned(p));
    }
    fn score_changed(&mut self, t: Team, s: u32) {
        self.out.push(GameEvent::Score { team: t, score: s });
    }
    fn sfx(&mut self, name: &str) {
        self.out.push(GameEvent::Cue(name.to_string()));
    }
}

impl GameMode for DodgeballMode {
    fn name(&self) -> &'static str {
        "Dodgeball"
    }

    fn add_player(&mut self, id: PlayerId) -> Team {
        let team = (self.joined % 2) as Team; // alternate so the teams stay balanced
        self.joined += 1;
        self.mgr.add_player(id, team);
        team
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.mgr.players.remove(&id);
    }

    /// Validate a client's request. A `Hit` runs the verified `IsValidHit` + eliminates the victim; a `Catch` puts the
    /// thrower out (the catcher's team keeps the catcher). Both emit the resulting out/score/round events.
    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>) {
        let mut c = Collect { out, round: self.mgr.round };
        match input {
            GameInput::Hit { thrower, victim } => self.mgr.on_player_hit(victim, thrower, &mut c),
            GameInput::Catch { catcher, thrower } => self.mgr.on_catch(catcher, thrower, &mut c),
            GameInput::Goal { .. } => {} // not a Dodgeball input
        }
    }

    /// Advance the round/match clock (pre-round → running → post-round → next/game-over). This is the GameRule base's
    /// timing; the real `DodgeballManager.Update` is only killzone enforcement (teleport out-of-bounds balls back),
    /// which belongs to the ball/arena layer, not the authority rules.
    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>) {
        let mut c = Collect { out, round: self.mgr.round };
        self.mgr.tick(dt, &mut c);
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

    #[test]
    fn authority_validates_a_hit_request_and_scores() {
        use crate::mode::{GameEvent, GameInput, GameMode, RoundPhase};
        // two players on opposite teams; score_to_win 1 so one wiped team ends the match
        let mut m = DodgeballMode::new(DodgeballConfig { score_to_win: 1, pre_round_duration: 0.0, post_round_duration: 1.0 });
        let t0 = m.add_player(1);
        let t1 = m.add_player(2);
        assert_ne!(t0, t1, "teams alternate");

        let mut out = Vec::new();
        m.tick(0.1, &mut out); // pre-round(0) elapses -> Running
        assert!(out.iter().any(|e| matches!(e, GameEvent::Round { phase: RoundPhase::Running, .. })));

        // a friendly-fire request is rejected (IsValidHit: same team never counts)
        let mut ff = Vec::new();
        m.input(GameInput::Hit { thrower: 2, victim: 2 }, &mut ff); // self/own-team
        assert!(ff.is_empty(), "a self/friendly hit is not valid");

        // player 1's thrown ball hit player 2 (the owner's client raised RpcMasterRequestPlayerOut)
        let mut ev = Vec::new();
        m.input(GameInput::Hit { thrower: 1, victim: 2 }, &mut ev);
        let out_player = ev.iter().find_map(|e| if let GameEvent::PlayerOut(p) = e { Some(*p) } else { None });
        let scoring = ev.iter().find_map(|e| if let GameEvent::Score { team, .. } = e { Some(*team) } else { None });
        assert_eq!(out_player, Some(2), "the authority validates the hit and puts player 2 out");
        assert_eq!(scoring, Some(t0), "team 0 (last standing) scores the round");
    }
}
