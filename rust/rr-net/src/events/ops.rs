//! Server operations: NamedMessage 5 (client to server) and 6 (server to client), `[u8 NetworkEventCode][struct]`.
//! Flagged structs write only the fields their flags name.
use super::tagged::Value;
use super::{In, Malformed, Out};

pub const SELF_JOIN_MARKER: u8 = 7;
pub const JOIN_REQUEST: u8 = 100;
pub const JOIN_RESPONSE: u8 = 101;
pub const ROOM_UPDATE: u8 = 102;
pub const PLAYER_UPDATE: u8 = 103;
pub const REMOTE_PLAYER_JOINED: u8 = 104;
pub const REMOTE_PLAYER_LEFT: u8 = 105;
pub const LEAVE: u8 = 106;

/// `NetworkedPlayerData`; flags Name 1, AccountId 2, PlatformId 4, Properties 8.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PlayerData {
    pub client_id: u64,
    pub name: Option<String>,
    pub account_id: Option<i32>,
    pub platform_id: Option<String>,
    pub properties: Option<Value>,
}

/// `NetworkedRoomData`; flags MasterClient 1, Players 2, Properties 4, PreviousMasterClient 8.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RoomData {
    pub master: Option<u64>,
    pub previous_master: Option<u64>,
    pub players: Option<Vec<PlayerData>>,
    pub properties: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    JoinRequest { name: String, platform_id: String, properties: Value },
    JoinResponse { success: bool, room: RoomData },
    RoomUpdate(RoomData),
    PlayerUpdate(PlayerData),
    RemotePlayerJoined(PlayerData),
    RemotePlayerLeft { left: u64, update: RoomData },
    Leave,
    /// end of the cached-event replay for a joiner
    SelfJoinMarker,
}

fn flags(bits: &[(bool, u8)]) -> u8 {
    bits.iter().filter(|(on, _)| *on).fold(0, |f, (_, b)| f | b)
}

impl PlayerData {
    fn write(&self, o: &mut Out) {
        o.u8(flags(&[(self.name.is_some(), 1), (self.account_id.is_some(), 2), (self.platform_id.is_some(), 4), (self.properties.is_some(), 8)]));
        o.u64(self.client_id);
        if let Some(n) = &self.name {
            o.string(n);
        }
        if let Some(a) = self.account_id {
            o.i32(a);
        }
        if let Some(p) = &self.platform_id {
            o.string(p);
        }
        if let Some(p) = &self.properties {
            p.write(o);
        }
    }

    fn read(r: &mut In) -> Result<Self, Malformed> {
        let f = r.u8()?;
        Ok(PlayerData {
            client_id: r.u64()?,
            name: if f & 1 != 0 { Some(r.string()?) } else { None },
            account_id: if f & 2 != 0 { Some(r.i32()?) } else { None },
            platform_id: if f & 4 != 0 { Some(r.string()?) } else { None },
            properties: if f & 8 != 0 { Some(Value::read(r, 0)?) } else { None },
        })
    }
}

impl RoomData {
    fn write(&self, o: &mut Out) {
        o.u8(flags(&[(self.master.is_some(), 1), (self.players.is_some(), 2), (self.properties.is_some(), 4), (self.previous_master.is_some(), 8)]));
        if let Some(m) = self.master {
            o.u64(m);
        }
        if let Some(m) = self.previous_master {
            o.u64(m);
        }
        if let Some(ps) = &self.players {
            o.i32(ps.len() as i32);
            ps.iter().for_each(|p| p.write(o));
        }
        if let Some(p) = &self.properties {
            p.write(o);
        }
    }

    fn read(r: &mut In) -> Result<Self, Malformed> {
        let f = r.u8()?;
        Ok(RoomData {
            master: if f & 1 != 0 { Some(r.u64()?) } else { None },
            previous_master: if f & 8 != 0 { Some(r.u64()?) } else { None },
            players: if f & 2 != 0 {
                let n = r.count(9)?;
                Some((0..n).map(|_| PlayerData::read(r)).collect::<Result<_, _>>()?)
            } else {
                None
            },
            properties: if f & 4 != 0 { Some(Value::read(r, 0)?) } else { None },
        })
    }
}

impl Op {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Op::JoinRequest { name, platform_id, properties } => {
                o.u8(JOIN_REQUEST);
                o.string(name);
                o.string(platform_id);
                properties.write(&mut o);
            }
            Op::JoinResponse { success, room } => {
                o.u8(JOIN_RESPONSE);
                o.bool(*success);
                room.write(&mut o);
            }
            Op::RoomUpdate(room) => {
                o.u8(ROOM_UPDATE);
                room.write(&mut o);
            }
            Op::PlayerUpdate(p) => {
                o.u8(PLAYER_UPDATE);
                p.write(&mut o);
            }
            Op::RemotePlayerJoined(p) => {
                o.u8(REMOTE_PLAYER_JOINED);
                p.write(&mut o);
            }
            Op::RemotePlayerLeft { left, update } => {
                o.u8(REMOTE_PLAYER_LEFT);
                o.u64(*left);
                update.write(&mut o);
            }
            Op::Leave => o.u8(LEAVE),
            Op::SelfJoinMarker => o.u8(SELF_JOIN_MARKER),
        }
        o.0
    }

    pub fn decode(b: &[u8]) -> Result<Op, Malformed> {
        let mut r = In::new(b);
        let op = match r.u8()? {
            JOIN_REQUEST => Op::JoinRequest { name: r.string()?, platform_id: r.string()?, properties: Value::read(&mut r, 0)? },
            JOIN_RESPONSE => Op::JoinResponse { success: r.bool()?, room: RoomData::read(&mut r)? },
            ROOM_UPDATE => Op::RoomUpdate(RoomData::read(&mut r)?),
            PLAYER_UPDATE => Op::PlayerUpdate(PlayerData::read(&mut r)?),
            REMOTE_PLAYER_JOINED => Op::RemotePlayerJoined(PlayerData::read(&mut r)?),
            REMOTE_PLAYER_LEFT => Op::RemotePlayerLeft { left: r.u64()?, update: RoomData::read(&mut r)? },
            LEAVE => Op::Leave,
            SELF_JOIN_MARKER => Op::SelfJoinMarker,
            _ => return Err(Malformed),
        };
        if r.at != b.len() {
            return Err(Malformed);
        }
        Ok(op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_round_trip() {
        let p = PlayerData { client_id: 2, name: Some("Bea".into()), account_id: Some(7), platform_id: None, properties: None };
        let room = RoomData { master: Some(1), previous_master: None, players: Some(vec![p.clone(), PlayerData { client_id: 1, ..Default::default() }]), properties: Some(Value::Null) };
        for op in [
            Op::JoinRequest { name: "Bea".into(), platform_id: "".into(), properties: Value::Null },
            Op::JoinResponse { success: true, room: room.clone() },
            Op::RoomUpdate(RoomData { master: Some(2), previous_master: Some(1), ..Default::default() }),
            Op::PlayerUpdate(p.clone()),
            Op::RemotePlayerJoined(p),
            Op::RemotePlayerLeft { left: 1, update: RoomData { master: Some(2), ..Default::default() } },
            Op::Leave,
            Op::SelfJoinMarker,
        ] {
            assert_eq!(Op::decode(&op.encode()), Ok(op));
        }
        assert_eq!(Op::decode(&[JOIN_RESPONSE, 1]), Err(Malformed));
        assert_eq!(Op::decode(&[LEAVE, 0]), Err(Malformed));
    }
}
