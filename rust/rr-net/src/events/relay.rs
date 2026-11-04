//! Relayed events, PUN's RaiseEvent: the client sends NamedMessage 3 with an envelope (code, delivery, options) and
//! tagged content; the server passes the content on, unread, as NamedMessage 4 with the sender's actor number.
use super::{In, Malformed, Out, RrDelivery};

/// `NetworkEventCode`s the room layer handles so far.
pub const BUFFERED_ROOM_EVENT: u8 = 22;
pub const CLEAR_BUFFERED_ROOM_EVENT: u8 = 23;
pub const NETWORK_SYNCHRONIZATION: u8 = 28;
pub const NETWORK_INSTANTIATE: u8 = 29;
pub const RPC: u8 = 36;

/// `RpcCacheOption`
pub const DO_NOT_CACHE: u8 = 0;
pub const ADD_TO_ROOM_CACHE: u8 = 4;
pub const ADD_TO_ROOM_CACHE_GLOBAL: u8 = 5;
pub const REMOVE_FROM_ROOM_CACHE: u8 = 6;
pub const REMOVE_FROM_ROOM_CACHE_FOR_ACTORS_LEFT: u8 = 7;

/// `RRReceiverGroup`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Receivers {
    #[default]
    Others = 0,
    All = 1,
    Master = 2,
}

/// `RaiseEventOptions`. `targets` overrides `receivers`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Options {
    pub cache: u8,
    pub group: u8,
    pub targets: Option<Vec<i32>>,
    pub receivers: Receivers,
}

/// Client to server.
#[derive(Clone, Debug, PartialEq)]
pub struct Raise {
    pub code: u8,
    pub delivery: RrDelivery,
    pub options: Options,
    /// tagged content (`tagged::Value`), opaque to the server
    pub content: Vec<u8>,
}

/// Server to client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proxied {
    pub sender: i32,
    pub code: u8,
    /// the server tick when it relayed the event (our reading of the third header field)
    pub tick: i32,
    pub content: Vec<u8>,
}

impl Raise {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        o.u8(self.code);
        o.u8(self.delivery as u8);
        o.u8(self.options.cache);
        o.u8(self.options.group);
        match &self.options.targets {
            // -1: no list (null), as distinct from an empty one
            None => o.i32(-1),
            Some(t) => {
                o.i32(t.len() as i32);
                t.iter().for_each(|&a| o.i32(a));
            }
        }
        o.u8(self.options.receivers as u8);
        o.0.extend_from_slice(&self.content);
        o.0
    }

    pub fn decode(b: &[u8]) -> Result<Raise, Malformed> {
        let mut r = In::new(b);
        let code = r.u8()?;
        let delivery = RrDelivery::from_u8(r.u8()?).ok_or(Malformed)?;
        let (cache, group) = (r.u8()?, r.u8()?);
        let targets = if r.b.get(r.at..r.at + 4) == Some(&(-1i32).to_le_bytes()[..]) {
            r.i32()?;
            None
        } else {
            let n = r.count(4)?;
            Some((0..n).map(|_| r.i32()).collect::<Result<_, _>>()?)
        };
        let receivers = match r.u8()? {
            0 => Receivers::Others,
            1 => Receivers::All,
            2 => Receivers::Master,
            _ => return Err(Malformed),
        };
        Ok(Raise { code, delivery, options: Options { cache, group, targets, receivers }, content: r.rest().to_vec() })
    }
}

impl Proxied {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        o.i32(self.sender);
        o.u8(self.code);
        o.i32(self.tick);
        o.0.extend_from_slice(&self.content);
        o.0
    }

    pub fn decode(b: &[u8]) -> Result<Proxied, Malformed> {
        let mut r = In::new(b);
        Ok(Proxied { sender: r.i32()?, code: r.u8()?, tick: r.i32()?, content: r.rest().to_vec() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_round_trip() {
        let mut raise = Raise { code: RPC, delivery: RrDelivery::Reliable, options: Options { cache: ADD_TO_ROOM_CACHE, group: 0, targets: Some(vec![2, 3]), receivers: Receivers::Master }, content: vec![0] };
        assert_eq!(Raise::decode(&raise.encode()), Ok(raise.clone()));
        raise.options.targets = None;
        assert_eq!(Raise::decode(&raise.encode()), Ok(raise.clone()));
        raise.options.targets = Some(vec![]);
        assert_eq!(Raise::decode(&raise.encode()), Ok(raise));
        let p = Proxied { sender: 2, code: NETWORK_SYNCHRONIZATION, tick: 900, content: vec![24, 1, 0, 0, 0, 7] };
        assert_eq!(Proxied::decode(&p.encode()), Ok(p));
        assert_eq!(Raise::decode(&[RPC, 9]), Err(Malformed));
    }
}
