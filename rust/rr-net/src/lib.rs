//! Room networking shared by the Metallic-RC client and server, matching the 2026 client's stack from the bottom up
//! (docs/agents/rc-spec/realtime.md in the private notes):
//!
//! - [`layer`]: the RR packet layer, a marker byte after every datagram (0x11 plain, 0x2A encrypted);
//! - [`lnl`]: a LiteNetLib 1.x (protocol 13) compatible transport: connect handshake, reliable and sequenced
//!   channels, acks, fragments, ping and timeouts;
//! - [`ngo`]: Netcode batches and messages on top, and the session that gives each client its id and the server clock;
//! - [`events`]: the Rec Room server operations and relayed events carried in NamedMessages;
//! - [`sync`]: the movement records of event 28 (avatars and objects) and the smoothing of what others move;
//! - [`objects`]: networked objects: view ids, instantiate and destroy, ownership, RPC targets;
//! - [`voice`]: voice packets (Dissonance VoiceData) relayed through the room;
//! - [`games`]: built-in game modes (Paintball): match messages between the room server and the players;
//! - [`room`]: a player's side of a room (join, spawn, movement), used by the game client and the tests.
//!
//! No sockets here: callers feed datagrams in and take datagrams out, so the same code runs in tests, the game
//! client and the server.
pub mod events;
pub mod games;
pub mod layer;
pub mod lnl;
pub mod ngo;
pub mod objects;
pub mod room;
pub mod sync;
pub mod voice;
