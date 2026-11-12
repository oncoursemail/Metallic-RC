//! The general game-mode seam. Every built-in Rec Room Original runs as a [`GameMode`] on the room authority (the
//! server, or a host client): clients send what the player DID (inputs), the authority decides what HAPPENS (hit,
//! catch, out, score) and broadcasts it. No outcome is client-trusted. The next RRO game is a drop-in `impl GameMode`
//! reusing the same inputs, events and net plumbing.
//!
//! The authority owns the game's dynamic objects (e.g. thrown balls): it simulates them and broadcasts their state, so
//! every client renders the same flight and the same hit. Player poses are fed in each tick from the networked
//! avatars, so the mode can do its own hit detection against the authoritative positions.

pub type PlayerId = u32;
pub type Team = u8;
/// A dynamic object the authority owns (a thrown ball, a disc, …). Unique within the room.
pub type ObjectId = u64;

/// A player's authoritative pose, handed to the mode each tick so it can detect hits/catches against real positions.
#[derive(Clone, Copy, Debug)]
pub struct PlayerPose {
    pub id: PlayerId,
    pub team: Team,
    /// room-space position of the player's root (feet); the mode adds its own torso/catch offsets
    pub pos: [f32; 3],
}

/// What a client tells the authority the player did. The authority never trusts an *outcome* from a client — only the
/// raw action (threw a ball from here with this velocity; grabbed/dropped this object).
#[derive(Clone, Debug)]
pub enum GameInput {
    /// `player` released a ball from `origin` travelling at `velocity` (m/s, room space). The authority spawns the
    /// authoritative ball and simulates it from here.
    Throw { player: PlayerId, origin: [f32; 3], velocity: [f32; 3] },
    /// `player` grabbed authority ball `object` (removes it from play until thrown again).
    Grab { player: PlayerId, object: ObjectId },
    /// `player` dropped authority ball `object` at `pos`.
    Drop { player: PlayerId, object: ObjectId, pos: [f32; 3] },
}

/// The round/match lifecycle, broadcast so clients can show countdowns/scoreboards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundPhase {
    PreRound,
    Running,
    PostRound,
    GameOver,
}

/// What the authority broadcasts after deciding outcomes. Generic across games; a game uses the subset it needs.
#[derive(Clone, Debug)]
pub enum GameEvent {
    /// an authority-owned object moved — clients render it here (ballistic ball, disc, …)
    ObjectState { object: ObjectId, pos: [f32; 3], vel: [f32; 3] },
    /// an authority-owned object left play (caught, out of bounds, round reset)
    ObjectGone { object: ObjectId },
    /// `player` is now held by `holder` (grabbed), or free again (`holder` = None)
    ObjectHeld { object: ObjectId, holder: Option<PlayerId> },
    PlayerOut(PlayerId),
    PlayerRespawned(PlayerId),
    Score { team: Team, score: u32 },
    Round { round: u32, phase: RoundPhase, winner: Option<Team> },
    /// a named sound/vfx cue the mode emitted (clients map it to the real SFX/VFX)
    Cue(String),
}

/// A game mode running on the room authority. Reusable for every RRO game; the room-authority loop drives it.
pub trait GameMode: Send {
    /// a human-readable name (for logs / the room's mode field)
    fn name(&self) -> &'static str;
    /// a player joined; the mode assigns a team (returned so the authority can tell everyone)
    fn add_player(&mut self, id: PlayerId) -> Team;
    /// a player left
    fn remove_player(&mut self, id: PlayerId);
    /// a client input arrived; `players` are the current authoritative poses
    fn input(&mut self, input: GameInput, players: &[PlayerPose], out: &mut Vec<GameEvent>);
    /// advance the simulation by `dt` seconds against the current authoritative `players`
    fn tick(&mut self, dt: f32, players: &[PlayerPose], out: &mut Vec<GameEvent>);
}
