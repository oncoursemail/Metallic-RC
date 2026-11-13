//! LiteNetLib 1.x packets. Byte 0 holds the property (bits 0-4), the connection number (bits 5-6) and the fragmented
//! flag (bit 7); integers are little-endian.
use std::net::SocketAddr;

pub const PROTOCOL_ID: i32 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Property {
    Unreliable = 0,
    Channeled = 1,
    Ack = 2,
    Ping = 3,
    Pong = 4,
    ConnectRequest = 5,
    ConnectAccept = 6,
    Disconnect = 7,
    UnconnectedMessage = 8,
    MtuCheck = 9,
    MtuOk = 10,
    Broadcast = 11,
    Merged = 12,
    ShutdownOk = 13,
    PeerNotFound = 14,
    InvalidProtocol = 15,
    NatMessage = 16,
    Empty = 17,
}

impl Property {
    fn from_u8(v: u8) -> Option<Self> {
        use Property::*;
        Some(match v {
            0 => Unreliable,
            1 => Channeled,
            2 => Ack,
            3 => Ping,
            4 => Pong,
            5 => ConnectRequest,
            6 => ConnectAccept,
            7 => Disconnect,
            8 => UnconnectedMessage,
            9 => MtuCheck,
            10 => MtuOk,
            11 => Broadcast,
            12 => Merged,
            13 => ShutdownOk,
            14 => PeerNotFound,
            15 => InvalidProtocol,
            16 => NatMessage,
            17 => Empty,
            _ => return None,
        })
    }
}

/// How a message travels. The values are LiteNetLib's `DeliveryMethod`; a channeled packet's channel id is
/// `channel * 4 + delivery` (unreliable messages have no channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Delivery {
    ReliableUnordered = 0,
    /// unreliable, but never older than the last one delivered
    Sequenced = 1,
    ReliableOrdered = 2,
    Unreliable = 4,
}

impl Delivery {
    pub fn from_channel_id(id: u8) -> Option<Self> {
        match id % 4 {
            0 => Some(Delivery::ReliableUnordered),
            1 => Some(Delivery::Sequenced),
            2 => Some(Delivery::ReliableOrdered),
            // ReliableSequenced: the 2026 client never sends it
            _ => None,
        }
    }

    pub fn is_reliable(self) -> bool {
        matches!(self, Delivery::ReliableUnordered | Delivery::ReliableOrdered)
    }
}

pub const MAX_SEQUENCE: u16 = 32768;
const HALF_SEQUENCE: i32 = 16384;

/// `a - b` on the 15-bit sequence circle, in -16384..16384.
pub fn seq_diff(a: u16, b: u16) -> i32 {
    let max = MAX_SEQUENCE as i32;
    (a as i32 - b as i32 + max + HALF_SEQUENCE) % max - HALF_SEQUENCE
}

pub fn header_size(p: Property) -> usize {
    match p {
        Property::Channeled | Property::Ack => 4,
        Property::Ping => 3,
        Property::Pong => 11,
        Property::ConnectRequest => 18,
        Property::ConnectAccept => 15,
        Property::Disconnect => 9,
        _ => 1,
    }
}

pub const FRAGMENT_HEADER: usize = 6;

/// One decoded packet header; `body` is what follows it.
#[derive(Debug)]
pub struct Packet<'a> {
    pub property: Property,
    pub connection: u8,
    pub fragmented: bool,
    pub bytes: &'a [u8],
}

impl<'a> Packet<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let b0 = *bytes.first()?;
        let property = Property::from_u8(b0 & 0x1F)?;
        let fragmented = b0 & 0x80 != 0;
        let mut need = header_size(property);
        if fragmented {
            need += FRAGMENT_HEADER;
        }
        if bytes.len() < need {
            return None;
        }
        Some(Packet { property, connection: (b0 >> 5) & 3, fragmented, bytes })
    }

    pub fn sequence(&self) -> u16 {
        u16::from_le_bytes([self.bytes[1], self.bytes[2]])
    }

    pub fn channel_id(&self) -> u8 {
        self.bytes[3]
    }

    /// (fragment id, part, total) of a fragmented channeled packet.
    pub fn fragment(&self) -> (u16, u16, u16) {
        let r = |i: usize| u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]]);
        (r(4), r(6), r(8))
    }

    pub fn body(&self) -> &'a [u8] {
        let mut start = header_size(self.property);
        if self.fragmented {
            start += FRAGMENT_HEADER;
        }
        &self.bytes[start..]
    }

    pub fn i64_at(&self, at: usize) -> i64 {
        i64::from_le_bytes(self.bytes[at..at + 8].try_into().unwrap())
    }

    pub fn i32_at(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
    }
}

pub fn first_byte(p: Property, connection: u8, fragmented: bool) -> u8 {
    p as u8 | (connection & 3) << 5 | if fragmented { 0x80 } else { 0 }
}

/// A channeled packet: `[hdr][u16 seq][u8 channel]` plus `[u16 id][u16 part][u16 total]` when fragmented.
pub fn channeled(connection: u8, seq: u16, channel_id: u8, fragment: Option<(u16, u16, u16)>, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + FRAGMENT_HEADER + body.len());
    v.push(first_byte(Property::Channeled, connection, fragment.is_some()));
    v.extend_from_slice(&seq.to_le_bytes());
    v.push(channel_id);
    if let Some((id, part, total)) = fragment {
        for x in [id, part, total] {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }
    v.extend_from_slice(body);
    v
}

pub fn unreliable(connection: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + body.len());
    v.push(first_byte(Property::Unreliable, connection, false));
    v.extend_from_slice(body);
    v
}

/// Bytes of an ack bitfield: a window of 64 sequences, bit `seq % 64`.
pub const ACK_BYTES: usize = 9;

pub fn ack(connection: u8, window_start: u16, channel_id: u8, bits: &[u8; ACK_BYTES]) -> Vec<u8> {
    let mut v = vec![first_byte(Property::Ack, connection, false)];
    v.extend_from_slice(&window_start.to_le_bytes());
    v.push(channel_id);
    v.extend_from_slice(bits);
    v
}

pub fn ping(connection: u8, seq: u16) -> Vec<u8> {
    let mut v = vec![first_byte(Property::Ping, connection, false)];
    v.extend_from_slice(&seq.to_le_bytes());
    v
}

pub fn pong(connection: u8, seq: u16, time: i64) -> Vec<u8> {
    let mut v = vec![first_byte(Property::Pong, connection, false)];
    v.extend_from_slice(&seq.to_le_bytes());
    v.extend_from_slice(&time.to_le_bytes());
    v
}

/// The address the client says it connects to, as .NET serializes a SocketAddress: family (2 = IPv4, 23 = IPv6,
/// little-endian), port (big-endian), then the address.
pub fn socket_address(a: SocketAddr) -> Vec<u8> {
    let mut v = Vec::new();
    match a {
        SocketAddr::V4(a) => {
            v.extend_from_slice(&2u16.to_le_bytes());
            v.extend_from_slice(&a.port().to_be_bytes());
            v.extend_from_slice(&a.ip().octets());
            v.extend_from_slice(&[0; 8]);
        }
        SocketAddr::V6(a) => {
            v.extend_from_slice(&23u16.to_le_bytes());
            v.extend_from_slice(&a.port().to_be_bytes());
            v.extend_from_slice(&a.flowinfo().to_le_bytes());
            v.extend_from_slice(&a.ip().octets());
            v.extend_from_slice(&a.scope_id().to_le_bytes());
        }
    }
    v
}

/// `[hdr][i32 protocol][i64 connection time][i32 peer id][u8 address size][address][connect data]`.
pub fn connect_request(connection_time: i64, peer_id: i32, target: SocketAddr, data: &[u8]) -> Vec<u8> {
    let addr = socket_address(target);
    let mut v = vec![first_byte(Property::ConnectRequest, 0, false)];
    v.extend_from_slice(&PROTOCOL_ID.to_le_bytes());
    v.extend_from_slice(&connection_time.to_le_bytes());
    v.extend_from_slice(&peer_id.to_le_bytes());
    v.push(addr.len() as u8);
    v.extend_from_slice(&addr);
    v.extend_from_slice(data);
    v
}

/// A connect request as the server reads it.
#[derive(Clone, Debug)]
pub struct ConnectRequest {
    pub protocol: i32,
    pub connection_time: i64,
    pub peer_id: i32,
    pub data: Vec<u8>,
}

impl ConnectRequest {
    pub fn parse(p: &Packet) -> Option<Self> {
        if p.property != Property::ConnectRequest {
            return None;
        }
        let addr_len = *p.bytes.get(17)? as usize;
        Some(ConnectRequest {
            protocol: p.i32_at(1),
            connection_time: p.i64_at(5),
            peer_id: p.i32_at(13),
            data: p.bytes.get(18 + addr_len..)?.to_vec(),
        })
    }
}

/// `[hdr][i64 connection time][u8 connection number][u8 reused peer][i32 peer id]`.
pub fn connect_accept(connection_time: i64, connection: u8, peer_id: i32) -> Vec<u8> {
    let mut v = vec![first_byte(Property::ConnectAccept, 0, false)];
    v.extend_from_slice(&connection_time.to_le_bytes());
    v.push(connection);
    v.push(0);
    v.extend_from_slice(&peer_id.to_le_bytes());
    v
}

pub fn disconnect(connection: u8, connection_time: i64, data: &[u8]) -> Vec<u8> {
    let mut v = vec![first_byte(Property::Disconnect, connection, false)];
    v.extend_from_slice(&connection_time.to_le_bytes());
    v.extend_from_slice(data);
    v
}

pub fn bare(p: Property, connection: u8) -> Vec<u8> {
    vec![first_byte(p, connection, false)]
}

/// The packets inside a merged packet: `[hdr]` then `[u16 size][packet]` repeated.
pub fn split_merged(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 1;
    while at + 2 <= bytes.len() {
        let n = u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
        at += 2;
        if n == 0 || at + n > bytes.len() {
            break;
        }
        out.push(&bytes[at..at + n]);
        at += n;
    }
    out
}

/// .NET `DateTime.UtcNow.Ticks`: 100 ns units since 0001-01-01.
pub fn dotnet_ticks_now() -> i64 {
    let since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    621_355_968_000_000_000 + (since_unix.as_nanos() / 100) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_circle() {
        assert_eq!(seq_diff(5, 3), 2);
        assert_eq!(seq_diff(3, 5), -2);
        assert_eq!(seq_diff(1, MAX_SEQUENCE - 1), 2);
        assert_eq!(seq_diff(MAX_SEQUENCE - 1, 1), -2);
    }

    #[test]
    fn header_byte_and_fields() {
        let v = channeled(2, 300, 6, Some((7, 1, 3)), b"hi");
        let p = Packet::parse(&v).unwrap();
        assert_eq!((p.property, p.connection, p.fragmented), (Property::Channeled, 2, true));
        assert_eq!((p.sequence(), p.channel_id(), p.fragment()), (300, 6, (7, 1, 3)));
        assert_eq!(p.body(), b"hi");
        assert_eq!(Delivery::from_channel_id(6), Some(Delivery::ReliableOrdered));
    }

    #[test]
    fn connect_request_layout() {
        let v = connect_request(42, 9, "127.0.0.1:7777".parse().unwrap(), &[0, 0]);
        // protocol id at bytes 1..5, header 18 bytes, a 16-byte IPv4 address
        assert_eq!(&v[1..5], &13i32.to_le_bytes());
        assert_eq!(v[17], 16);
        assert_eq!(v.len(), 18 + 16 + 2);
        let r = ConnectRequest::parse(&Packet::parse(&v).unwrap()).unwrap();
        assert_eq!((r.protocol, r.connection_time, r.peer_id, r.data.as_slice()), (13, 42, 9, &[0u8, 0][..]));
        assert_eq!(connect_accept(42, 1, 9).len(), header_size(Property::ConnectAccept));
    }
}
