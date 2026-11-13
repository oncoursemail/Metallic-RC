//! Built-in game modes on the room server (Paintball first): the server runs the match, so scores, timers, teams,
//! flags and hits are its word, not a client's. The room server hands it what it needs: who is in which room, the
//! game messages (`rr_net::games::Up`), and the players' movement (event 28, read here for where everyone stands);
//! it sends back `Down` messages. A room gets a match the first time one of its players sends a game message, so
//! rooms that never play have no game traffic.
use std::collections::HashMap;

use rr_net::events::tagged::Value;
use rr_net::games::Up;
use rr_net::ngo::NgoDelivery;
use rr_net::sync::{self, AvatarData};

pub mod bots;
pub mod paintball;

pub use paintball::Match;

/// One message for one client.
pub struct Send {
    pub to: u64,
    pub payload: Vec<u8>,
    pub delivery: NgoDelivery,
}

#[derive(Default)]
pub struct Games {
    /// players in each room: client id and name
    members: HashMap<String, Vec<(u64, String)>>,
    matches: HashMap<String, Match>,
}

impl Games {
    pub fn joined(&mut self, now: f64, room: &str, client: u64, name: &str) {
        self.members.entry(room.to_string()).or_default().push((client, name.to_string()));
        if let Some(m) = self.matches.get_mut(room) {
            m.join(now, client, name);
        }
    }

    pub fn left(&mut self, room: &str, client: u64) {
        let empty = self.members.get_mut(room).is_none_or(|m| {
            m.retain(|(c, _)| *c != client);
            m.is_empty()
        });
        if empty {
            self.members.remove(room);
            self.matches.remove(room);
        } else if let Some(m) = self.matches.get_mut(room) {
            m.leave(client);
        }
    }

    /// A game message from `from`, a player in `room`.
    pub fn message(&mut self, now: f64, room: &str, from: u64, payload: &[u8]) {
        let Ok(up) = Up::decode(payload) else { return };
        let Some(members) = self.members.get(room) else { return };
        let m = self.matches.entry(room.to_string()).or_insert_with(|| {
            let mut m = Match::new(now);
            for (c, n) in members {
                m.join(now, *c, n);
            }
            m
        });
        m.handle(now, from, up);
    }

    /// The content of an event 28 (movement) `from` sent: its avatar record tells where the player stands.
    pub fn sync(&mut self, room: &str, from: u64, content: &[u8]) {
        let Some(m) = self.matches.get_mut(room) else { return };
        let Ok(Value::ByteArray(buf)) = Value::decode(content) else { return };
        let Ok((_, _, records)) = sync::read_buffer(&buf) else { return };
        let view = rr_net::room::view_id(from as i32);
        for (v, kind, bytes) in records {
            if v != view || kind != sync::PLAYER_AVATAR_DATA {
                continue;
            }
            if let Ok(AvatarData { tracking_space: Some(body), .. }) = AvatarData::decode(bytes) {
                let [_, y, _, w] = body.rotation;
                m.moved(from, body.position, 2.0 * y.atan2(w));
            }
        }
    }

    /// Runs the matches; what to send.
    pub fn update(&mut self, now: f64) -> Vec<Send> {
        let mut out = vec![];
        for m in self.matches.values_mut() {
            m.update(now);
            out.extend(m.take_outbox());
        }
        out
    }

    pub fn get(&self, room: &str) -> Option<&Match> {
        self.matches.get(room)
    }
}
