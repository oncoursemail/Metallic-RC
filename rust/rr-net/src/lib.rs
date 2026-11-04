//! Room networking shared by the Metallic-RC client and server, matching the 2026 client's stack from the bottom up
//! (docs/agents/rc-spec/realtime.md in the private notes):
//!
//! - [`layer`]: the RR packet layer, a marker byte after every datagram (0x11 plain, 0x2A encrypted);
//! - [`lnl`]: a LiteNetLib 1.x (protocol 13) compatible transport: connect handshake, reliable and sequenced
//!   channels, acks, fragments, ping and timeouts;
//! - [`ngo`]: Netcode batches and messages on top, and the session that gives each client its id and the server clock.
//!
//! No sockets here: callers feed datagrams in and take datagrams out, so the same code runs in tests, the game
//! client and the server.
pub mod layer;
pub mod lnl;
pub mod ngo;
