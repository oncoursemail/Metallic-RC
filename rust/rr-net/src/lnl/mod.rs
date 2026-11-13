//! A LiteNetLib 1.x (protocol 13) compatible transport, as the 2026 client's LiteNetLibTransport uses it.
pub mod packet;
pub mod peer;

pub use packet::{ConnectRequest, Delivery, Packet, Property};
pub use peer::{Config, Event, Peer, Reason, SendError};
