//! The general game-mode seam, matching Rec Room's real networked flow (verified against the decompiled bodies).
//! Every built-in RR Original runs as a [`GameMode`] on the room authority (the master client): a client that detects
//! an interaction sends a *request* (a thrown ball's owner raises `RpcMasterRequestPlayerOut`; a catcher raises
//! `RpcMasterRequestPlayerCatch`), the authority *validates* it (Dodgeball's `IsValidHit`: round active, both players
//! in, neither already out, not self, not same team) and *decides* the outcome, then broadcasts it (`RpcOnPlayerOut`)
//! for every client to apply. No outcome is client-trusted.
//!
//! The authority does NOT simulate the ball or do geometric hit tests — the thrown ball is the thrower's own
//! networked prop (it keeps the hand's release velocity and syncs through normal object ownership), and that owner's
//! client detects the Unity collision. So this seam carries only the logical requests and outcomes; the next RRO game
//! is a drop-in `impl GameMode` over the same requests/events and net plumbing.

pub type PlayerId = u32;
pub type Team = u8;

/// What a client asks the authority to adjudicate. The authority never trusts the *outcome* — only that the client
/// observed the interaction (its ball hit someone / it caught a ball); the authority decides whether it counts.
#[derive(Clone, Copy, Debug)]
pub enum GameInput {
    /// a live ball thrown by `thrower` collided with `victim` on the owner's client (→ `RpcMasterRequestPlayerOut`)
    Hit { thrower: PlayerId, victim: PlayerId },
    /// `catcher` grabbed a live ball thrown by `thrower` (→ `RpcMasterRequestPlayerCatch`)
    Catch { catcher: PlayerId, thrower: PlayerId },
    /// a ball entered a goal; `team` is the team that scores (→ Paddleball's `RpcMasterRequestScore`). The goal's
    /// owning team is resolved to the scoring team on the client that saw the trigger; the master validates + counts.
    Goal { team: Team },
    /// `player` released a disc/projectile — one stroke (→ DiscGolf's throw; the authority counts it)
    Throw { player: PlayerId },
    /// `player`'s disc entered a basket — hole complete (→ DiscGolf's `RpcMasterFinishHole`)
    Basket { player: PlayerId },
    /// `player`'s disc went out of bounds — a penalty stroke + return to tee (→ DiscGolf's `RpcOnDiscHazard`)
    Hazard { player: PlayerId },
    /// `player` completed a bowl that knocked down `pins` pins (0..=10); recorded by Bowling's authority, which runs
    /// the real 10-pin scoreboard algorithm over the per-player bowl array. Pin count is physics (standing before −
    /// standing after the pins settle), not scripted.
    Bowl { player: PlayerId, pins: u8 },
}

/// The round/match lifecycle, broadcast so clients can show countdowns/scoreboards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundPhase {
    PreRound,
    Running,
    PostRound,
    GameOver,
}

/// What the authority broadcasts after deciding outcomes (→ `RpcOnPlayerOut` etc.). Generic across games.
#[derive(Clone, Debug)]
pub enum GameEvent {
    PlayerOut(PlayerId),
    PlayerRespawned(PlayerId),
    Score { team: Team, score: u32 },
    Round { round: u32, phase: RoundPhase, winner: Option<Team> },
    /// a per-player score changed (e.g. DiscGolf's running total, strokes relative to par — can be negative)
    PlayerScore { player: PlayerId, score: i32 },
    /// a player-scored match ended; `winner` won (e.g. DiscGolf's lowest-strokes/most-under-par player)
    MatchOver { winner: PlayerId },
    /// a named sound/vfx cue (clients map it to the real SFX/VFX, e.g. the impact at the contact point)
    Cue(String),
}

/// A game mode running on the room authority (the master client). Reusable for every RRO game; the room-authority
/// loop validates requests through it, ticks it, and broadcasts the events it emits.
pub trait GameMode: Send {
    fn name(&self) -> &'static str;
    /// a player joined; the mode assigns a team (returned so the authority can tell everyone)
    fn add_player(&mut self, id: PlayerId) -> Team;
    fn remove_player(&mut self, id: PlayerId);
    /// validate + apply a client request; any resulting outcomes are pushed to `out`
    fn input(&mut self, input: GameInput, out: &mut Vec<GameEvent>);
    /// advance the round/match clock by `dt` seconds
    fn tick(&mut self, dt: f32, out: &mut Vec<GameEvent>);
}
