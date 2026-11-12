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
    /// `guesser` pressed the guess button signalling a correct guess of the current performer's drawing (→ Charades's
    /// `RpcMasterRequestGuess`). The master validates the guesser is NOT the performer, scores +1 (co-op pool or the
    /// guessing team), and advances to the next word/performer. Correctness is the in-world button signal, not text.
    Guess { guesser: PlayerId },
    /// `player` crossed the current course's finish gate with a run time of `time_ms` milliseconds (→ StuntRunner's
    /// `RpcMasterRequestCourseFinishTime`). The master validates + records the per-course time, scores it (faster =
    /// higher), and advances the 6-course sequence. The clock is master-authoritative; the time is the wire value (ms).
    FinishCourse { player: PlayerId, time_ms: u32 },
    /// a Paintball CTF flag event on the flag that belongs to `team` (→ the manager's `RpcMasterRequestFlag*`):
    /// `player` took / captured / dropped / returned it. The master runs the flag state machine; a `Captured` scores
    /// +1 for the capturing player's team. (A paintball TAG reuses [`GameInput::Hit`] — thrower = the shooter.)
    Flag { event: FlagEvent, player: PlayerId, team: Team },
    /// `team` completed the final lap and crossed the finish line (→ RecRally's `RpcMasterRallyRequestRaceFinish`).
    /// The master validates it (not already finished) and assigns the team's PLACE in finish ORDER (1st, 2nd, …);
    /// place 1 wins. A single race, finish-order placement — no points, no time score.
    FinishRace { team: Team },
    /// an Arena (Team Warzone) kill/pickup that scores for `team` (the KILLER's team). The master awards the real
    /// per-`kind` kill-points to that team's pool (clamped ≥ 0). PvE and PvP both count. (→ Arena's master-validated
    /// kill handler.)
    Kill { team: Team, kind: KillKind },
}

/// What an Arena kill/pickup was, which fixes its kill-point value (the real `ArenaManager` scoring table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillKind {
    /// killed an enemy-team player (PvP): +10
    EnemyPlayer,
    /// killed a teammate (friendly fire): −10
    Teammate,
    /// killed a normal enemy (PvE): +2
    NormalEnemy,
    /// killed an elite enemy (PvE): +10
    EliteEnemy,
    /// an enemy friendly-fire event: −2
    EnemyFriendlyFire,
    /// opened a loot chest: +1
    LootChest,
}

/// A Paintball CTF flag event (the flag gadget's master-validated signals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagEvent {
    /// an enemy picked up this team's flag from its base
    Taken,
    /// the carrier brought this (enemy) flag to their own base — the carrier's team scores
    Captured,
    /// the carrier was tagged — this flag drops where they stood
    Dropped,
    /// this team's flag was returned to its base
    Returned,
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
