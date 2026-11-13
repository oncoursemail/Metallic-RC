//! The seven Netcode messages the 2026 client knows. Their type ids are positions in a table: before approval the
//! fixed order below; after it, the order of the hashes the server sent in ConnectionApproved.
use twox_hash::XxHash32;

use super::pack::{Reader, Truncated, Writer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    ConnectionApproved,
    ConnectionRequest,
    DisconnectReason,
    Named,
    ServerLog,
    TimeSync,
    Unnamed,
}

/// The table before approval: approval and request first, the rest by type name.
pub const ORDER: [Kind; 7] = [
    Kind::ConnectionApproved,
    Kind::ConnectionRequest,
    Kind::DisconnectReason,
    Kind::Named,
    Kind::ServerLog,
    Kind::TimeSync,
    Kind::Unnamed,
];

impl Kind {
    pub fn type_name(self) -> &'static str {
        match self {
            Kind::ConnectionApproved => "Unity.Netcode.ConnectionApprovedMessage",
            Kind::ConnectionRequest => "Unity.Netcode.ConnectionRequestMessage",
            Kind::DisconnectReason => "Unity.Netcode.DisconnectReasonMessage",
            Kind::Named => "Unity.Netcode.NamedMessage",
            Kind::ServerLog => "Unity.Netcode.ServerLogMessage",
            Kind::TimeSync => "Unity.Netcode.TimeSyncMessage",
            Kind::Unnamed => "Unity.Netcode.UnnamedMessage",
        }
    }

    /// XXH32 of the type's full name: how both sides name a message type in the version lists.
    pub fn hash(self) -> u32 {
        XxHash32::oneshot(0, self.type_name().as_bytes())
    }

    pub fn from_hash(h: u32) -> Option<Self> {
        ORDER.into_iter().find(|k| k.hash() == h)
    }
}

/// `(type hash, version)` for each message type, in table order; every version is 0.
pub fn versions(order: &[Kind]) -> Vec<(u32, i32)> {
    order.iter().map(|k| (k.hash(), 0)).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    ConnectionRequest { versions: Vec<(u32, i32)>, config_hash: u64, data: Vec<u8> },
    ConnectionApproved { versions: Vec<(u32, i32)>, client_id: u64, tick: i32, payload: String },
    DisconnectReason(String),
    /// `hash` is the RR NamedMessageType (3/4 relayed events, 5/6 server operations, 1/2 voice)
    Named { hash: u16, payload: Vec<u8> },
    TimeSync { tick: i32 },
    Unnamed(Vec<u8>),
}

fn write_versions(w: &mut Writer, v: &[(u32, i32)]) {
    w.u32(v.len() as u32);
    for &(hash, version) in v {
        w.raw(&hash.to_le_bytes());
        w.i32(version);
    }
}

fn read_versions(r: &mut Reader) -> Result<Vec<(u32, i32)>, Truncated> {
    let n = r.u32()? as usize;
    (0..n).map(|_| Ok((r.raw_u32()?, r.i32()?))).collect()
}

impl Message {
    pub fn kind(&self) -> Kind {
        match self {
            Message::ConnectionRequest { .. } => Kind::ConnectionRequest,
            Message::ConnectionApproved { .. } => Kind::ConnectionApproved,
            Message::DisconnectReason(_) => Kind::DisconnectReason,
            Message::Named { .. } => Kind::Named,
            Message::TimeSync { .. } => Kind::TimeSync,
            Message::Unnamed(_) => Kind::Unnamed,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Message::ConnectionRequest { versions, config_hash, data } => {
                write_versions(&mut w, versions);
                w.raw(&config_hash.to_le_bytes());
                w.bytes(data);
            }
            Message::ConnectionApproved { versions, client_id, tick, payload } => {
                write_versions(&mut w, versions);
                w.u64(*client_id);
                w.i32(*tick);
                w.string(payload);
            }
            Message::DisconnectReason(reason) => w.string(reason),
            Message::Named { hash, payload } => {
                w.raw(&hash.to_le_bytes());
                w.raw(payload);
            }
            Message::TimeSync { tick } => w.i32(*tick),
            Message::Unnamed(b) => w.raw(b),
        }
        w.0
    }

    /// A message body of the given kind; ServerLog is not used and comes back as `None`.
    pub fn decode(kind: Kind, body: &[u8]) -> Result<Option<Self>, Truncated> {
        let mut r = Reader::new(body);
        Ok(Some(match kind {
            Kind::ConnectionRequest => {
                let versions = read_versions(&mut r)?;
                let config_hash = r.raw_u64()?;
                // without connection approval there is no data
                let data = if r.at < body.len() { r.bytes()?.to_vec() } else { vec![] };
                Message::ConnectionRequest { versions, config_hash, data }
            }
            Kind::ConnectionApproved => Message::ConnectionApproved {
                versions: read_versions(&mut r)?,
                client_id: r.u64()?,
                tick: r.i32()?,
                payload: if r.at < body.len() { r.string()? } else { String::new() },
            },
            Kind::DisconnectReason => Message::DisconnectReason(r.string()?),
            Kind::Named => Message::Named { hash: r.raw_u16()?, payload: r.rest().to_vec() },
            Kind::TimeSync => Message::TimeSync { tick: r.i32()? },
            Kind::Unnamed => Message::Unnamed(r.rest().to_vec()),
            Kind::ServerLog => return Ok(None),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_distinct_and_messages_round_trip() {
        let hashes: std::collections::HashSet<u32> = ORDER.iter().map(|k| k.hash()).collect();
        assert_eq!(hashes.len(), 7);
        assert_eq!(Kind::from_hash(Kind::TimeSync.hash()), Some(Kind::TimeSync));
        for m in [
            Message::ConnectionRequest { versions: versions(&ORDER), config_hash: 0x1234_5678_9ABC_DEF0, data: b"{\"AI\":\"1\"}".to_vec() },
            Message::ConnectionApproved { versions: versions(&ORDER), client_id: 2, tick: 12345, payload: "{\"DR\":\"Carrot\"}".into() },
            Message::DisconnectReason("no room".into()),
            Message::Named { hash: 5, payload: vec![100, 1, 2, 3] },
            Message::TimeSync { tick: -7 },
            Message::Unnamed(vec![9, 9]),
        ] {
            assert_eq!(Message::decode(m.kind(), &m.encode()), Ok(Some(m)));
        }
    }
}
