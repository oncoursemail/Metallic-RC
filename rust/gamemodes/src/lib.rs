//! **Rec Room built-in game modes** (RR Originals), ported from the decompiled C#.
//!
//! The built-in games (Dodgeball, Paintball, Golf, Quest, Laser Tag, …) are not loadable circuits — each is a
//! hardcoded `GameRuleManager` subclass plus its gadgets (e.g. `Dodgeball : Ball`). So this crate reimplements each
//! game's rules manager as engine-agnostic Rust: the client/authoritative server drives it (ticks it, feeds it the
//! gadget/network events) and implements the per-game effects seam. The CV2 `circuits` crate is the parallel layer
//! for user-made games + gadget config/events + loading real UGC rooms; it is not needed to run a built-in game.
//!
//! The `*GameRuleWrapper` family (@dump.cs:695966-696240) enumerates every built-in game; each is a bounded port.
//! Dodgeball is the first (simplest) slice.

pub mod arena;
pub mod bowling;
pub mod charades;
pub mod discgolf;
pub mod dodgeball;
pub mod mode;
pub mod paddleball;
pub mod paintball;
pub mod recrally;
pub mod soccer;
pub mod stuntrunner;

pub use arena::{ArenaConfig, ArenaMode};
pub use bowling::{BowlingConfig, BowlingMode};
pub use charades::{CharadesConfig, CharadesMode};
pub use discgolf::{DiscGolfConfig, DiscGolfMode};
pub use stuntrunner::{StuntRunnerConfig, StuntRunnerMode};
pub use dodgeball::{DodgeballConfig, DodgeballEffects, DodgeballManager, DodgeballMode, Phase, PlayerId, Team};
pub use mode::{FlagEvent, GameEvent, GameInput, GameMode, KillKind, RoundPhase};
pub use paddleball::{PaddleballConfig, PaddleballMode};
pub use paintball::{PaintballConfig, PaintballMode, PaintballVariant};
pub use recrally::{RecRallyConfig, RecRallyMode};
pub use soccer::{SoccerConfig, SoccerMode};
