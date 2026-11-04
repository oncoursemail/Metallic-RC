//! The 2026 client's Netcode for GameObjects fork, as far as the room connection uses it: batches, byte packing, the
//! seven message types and the connect/approve session with its clock.
pub mod batch;
pub mod message;
pub mod pack;
pub mod session;

pub use message::{Kind, Message};
pub use session::{Client, ClientEvent, NgoDelivery, Server, ServerEvent};
