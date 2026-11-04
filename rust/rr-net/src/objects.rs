//! Networked objects, the RRNetworkView side of the room (realtime.md 5.4): view ids, instantiate and destroy,
//! ownership and RPC targets. `room::RoomClient` sends and receives them; this module holds the state and the
//! content formats.
//!
//! - View ids follow the PUN scheme: `actor * 100000 + sub id`; actor 0 holds the scene's own objects, numbered by
//!   the room's layout so every player agrees without a message.
//! - An object outlives its creator (instantiate is cached global); an owner who leaves hands its objects to the
//!   room's master, which every player works out alike from the leave.
//! - Ownership is a takeover (a grab does not wait for the owner): event 34 carries `[view, new owner, version]`, and
//!   a higher version wins, the higher actor on a tie, so two players taking the same object at once end up agreeing.
//!   The stock event is `int[2]` (I); the version is ours.
//! - The master answers a newcomer with event 35, every object's `[view, owner, version]`, since ownership changes
//!   are not cached.
use crate::events::relay::{Options, Receivers, ADD_TO_ROOM_CACHE};
use crate::events::tagged::Value;
use crate::sync::{Pose, Smoother};

pub const MAX_VIEW_IDS: i32 = 100_000;
/// Sub ids of objects a player instantiates at runtime start here (`RUNTIME_INSTANTIATE_VIEW_ID_OFFSET`; that the
/// stock client counts from it is INFERRED from the name).
pub const RUNTIME_SUB_OFFSET: i32 = 90_000;
/// Event 32 (`RR_Destroy`), 34 (`RR_TransferOwnership`) and 35 (`RR_OwnershipUpdate`).
pub const DESTROY: u8 = 32;
pub const TRANSFER_OWNERSHIP: u8 = 34;
pub const OWNERSHIP_UPDATE: u8 = 35;
/// How long an owned object keeps sending after it stops moving, so its resting place gets through a lossy link.
pub const SETTLE: f64 = 1.0;

pub fn view_id(actor: i32, sub: i32) -> i32 {
    actor * MAX_VIEW_IDS + sub
}

/// The player whose range a view id is in (0 for scene objects).
pub fn creator(view: i32) -> i32 {
    view / MAX_VIEW_IDS
}

/// The view id of the `index`th networked object of the room's own layout (from 0).
pub fn scene_view_id(index: u32) -> i32 {
    1 + (index as i32 % (MAX_VIEW_IDS - 1))
}

/// `RpcTarget`, with the single-player target the stock client sends through `TargetActors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcTarget {
    All,
    Others,
    Master,
    AllBuffered,
    OthersBuffered,
    Actor(i32),
}

impl RpcTarget {
    /// The relay options, and whether the sender runs the RPC itself too (PUN runs "All" locally at once).
    pub fn options(self) -> (Options, bool) {
        let cached = Options { cache: ADD_TO_ROOM_CACHE, ..Default::default() };
        match self {
            RpcTarget::All => (Options::default(), true),
            RpcTarget::Others => (Options::default(), false),
            RpcTarget::Master => (Options { receivers: Receivers::Master, ..Default::default() }, false),
            RpcTarget::AllBuffered => (cached, true),
            RpcTarget::OthersBuffered => (cached, false),
            RpcTarget::Actor(a) => (Options { targets: Some(vec![a]), ..Default::default() }, false),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectEvent {
    /// an object another player instantiated (or one this player did, from the room cache after a rejoin)
    Spawned { view: i32, address: String, position: [f32; 3], rotation: [f32; 4], scale: [f32; 3], owner: i32, data: Vec<Value> },
    Destroyed { view: i32 },
    /// the actor that owns `view` now
    OwnerChanged { view: i32, owner: i32 },
    /// an RPC on an object (or on view 0, the room) that the room layer does not handle itself
    Rpc { view: i32, sender: i32, method: String, params: Vec<Value> },
}

pub struct Object {
    pub address: String,
    /// `None`: the room's master
    pub owner: Option<i32>,
    pub version: i32,
    /// the other owner's samples, smoothed for drawing
    pub shown: Smoother,
    /// this player's pose for it, while it owns it
    pub pose: Pose,
    /// what went out last, and until when to keep sending
    pub sent: Option<Pose>,
    pub busy_until: f64,
}

impl Object {
    pub fn new(address: String, owner: Option<i32>, pose: Pose) -> Self {
        Object { address, owner, version: 0, shown: Smoother::default(), pose, sent: None, busy_until: 0.0 }
    }

    /// Takes an ownership claim if it beats the one held: a higher version, or the higher actor on the same one.
    pub fn claim(&mut self, owner: Option<i32>, version: i32) -> bool {
        if (version, owner.unwrap_or(0)) > (self.version, self.owner.unwrap_or(0)) {
            (self.owner, self.version) = (owner, version);
            true
        } else {
            false
        }
    }
}

/// Whether two poses differ enough to send again.
pub fn moved(a: &Pose, b: &Pose) -> bool {
    let d = (0..3).map(|i| (a.position[i] - b.position[i]).powi(2)).sum::<f32>().sqrt();
    let dot = (0..4).map(|i| a.rotation[i] * b.rotation[i]).sum::<f32>().abs();
    d > 5e-4 || dot < 0.99999 || a.velocity.iter().any(|v| v.abs() > 1e-3)
}

/// Event 34 and 35 content: `[view, owner (0 = master), version]` triples.
pub fn owners_content(entries: &[(i32, Option<i32>, i32)]) -> Value {
    Value::IntArray(entries.iter().flat_map(|&(v, o, n)| [v, o.unwrap_or(0), n]).collect())
}

pub fn read_owners(v: &Value) -> Vec<(i32, Option<i32>, i32)> {
    let Value::IntArray(a) = v else { return vec![] };
    a.chunks_exact(3).map(|c| (c[0], (c[1] > 0).then_some(c[1]), c[2])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_ids_and_claims() {
        assert_eq!((view_id(3, RUNTIME_SUB_OFFSET + 1), creator(390_001), creator(scene_view_id(7))), (390_001, 3, 0));
        let mut o = Object::new("Cube".into(), Some(1), Pose::default());
        // two players take it at once with the same version: the higher actor wins, whichever claim comes first
        let mut p = Object::new("Cube".into(), Some(1), Pose::default());
        assert!(o.claim(Some(2), 1) && !o.claim(Some(1), 1));
        assert!(p.claim(Some(1), 1) && p.claim(Some(2), 1));
        assert_eq!((o.owner, p.owner), (Some(2), Some(2)));
        // an older claim arriving late changes nothing
        assert!(!o.claim(Some(5), 0));
        let c = owners_content(&[(5, None, 2), (7, Some(3), 9)]);
        assert_eq!(read_owners(&c), vec![(5, None, 2), (7, Some(3), 9)]);
    }
}
