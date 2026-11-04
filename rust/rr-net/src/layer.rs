//! The RR packet layer around every UDP datagram, the LiteNetLib handshake included: a marker byte at the end,
//! 0x11 for plain, 0x2A for Rabbit-encrypted (the 8-byte IV just before it). Encryption comes with the 2026
//! handshake; until then an encrypted datagram is refused.
use std::fmt;

pub const PLAIN: u8 = 0x11;
pub const ENCRYPTED: u8 = 0x2A;
/// Bytes the layer may add to a datagram (IV + marker), taken off the transport's MTU.
pub const EXTRA: usize = 9;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    /// encrypted, and this endpoint has no key
    NoKey,
    UnknownMarker(u8),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Empty => f.write_str("empty datagram"),
            Error::NoKey => f.write_str("encrypted datagram but no key for this endpoint"),
            Error::UnknownMarker(m) => write!(f, "unknown marker {m:#04x}"),
        }
    }
}

impl std::error::Error for Error {}

/// A transport packet as it goes on the wire.
pub fn wrap(packet: &[u8]) -> Vec<u8> {
    let mut d = Vec::with_capacity(packet.len() + 1);
    d.extend_from_slice(packet);
    d.push(PLAIN);
    d
}

/// The transport packet inside a datagram.
pub fn unwrap(datagram: &[u8]) -> Result<&[u8], Error> {
    match datagram.split_last() {
        None => Err(Error::Empty),
        Some((&PLAIN, packet)) => Ok(packet),
        Some((&ENCRYPTED, _)) => Err(Error::NoKey),
        Some((&m, _)) => Err(Error::UnknownMarker(m)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_markers() {
        assert_eq!(wrap(&[1, 2, 3]), [1, 2, 3, 0x11]);
        assert_eq!(unwrap(&[1, 2, 3, 0x11]), Ok(&[1u8, 2, 3][..]));
        assert_eq!(unwrap(&[1, 2, 0x2A]), Err(Error::NoKey));
        assert_eq!(unwrap(&[1, 2, 0x12]), Err(Error::UnknownMarker(0x12)));
        assert_eq!(unwrap(&[]), Err(Error::Empty));
    }
}
