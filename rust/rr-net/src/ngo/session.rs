//! A Netcode session over the transport, both sides, without I/O: datagrams in through `receive`, out through
//! `poll_transmit` (RR layer included), `update` once a frame.
//!
//! The client sends ConnectionRequest as soon as the transport connects; the server answers ConnectionApproved with
//! the client's id (1, 2, ...: the RR actor number), the server tick and the approval payload, or DisconnectReason.
//! Server time is the server's clock at 30 ticks a second; the client estimates it from the tick in the approval
//! and in TimeSync (sent on approval and every 30 s), plus half the round trip.
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

use super::batch::{self, Batch};
use super::message::{self, Kind, Message, ORDER};
use crate::layer;
use crate::lnl::{self, packet, ConnectRequest, Delivery, Packet, Peer, Reason};

pub const TICK_RATE: f64 = 30.0;
pub const RESYNC_INTERVAL: f64 = 30.0;
/// The client's NetworkConfig hash. The 2026 server compares it; ours does not, so ours is a constant.
pub const CONFIG_HASH: u64 = 0;

/// Netcode's delivery kinds, and the transport delivery each one uses in the 2026 client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NgoDelivery {
    Unreliable,
    UnreliableSequenced,
    Reliable,
    ReliableSequenced,
    ReliableFragmentedSequenced,
}

impl NgoDelivery {
    pub fn transport(self) -> Delivery {
        match self {
            NgoDelivery::Unreliable => Delivery::Unreliable,
            NgoDelivery::UnreliableSequenced => Delivery::Sequenced,
            NgoDelivery::Reliable => Delivery::ReliableUnordered,
            NgoDelivery::ReliableSequenced | NgoDelivery::ReliableFragmentedSequenced => Delivery::ReliableOrdered,
        }
    }
}

/// Unreliable batches stay within one packet; reliable ones may be split by the transport.
const UNRELIABLE_BATCH: usize = 480;
const RELIABLE_BATCH: usize = 4096;

/// One transport peer with its message table and batches waiting to go.
struct Link {
    peer: Peer,
    order: Vec<Kind>,
    batches: Vec<(Delivery, Batch)>,
}

impl Link {
    fn new(peer: Peer) -> Self {
        Link { peer, order: ORDER.to_vec(), batches: vec![] }
    }

    fn queue(&mut self, m: &Message, d: NgoDelivery) {
        let Some(type_id) = self.order.iter().position(|k| *k == m.kind()) else { return };
        let body = m.encode();
        let d = d.transport();
        let cap = if d.is_reliable() { RELIABLE_BATCH } else { UNRELIABLE_BATCH };
        let i = match self.batches.iter().position(|(bd, b)| *bd == d && b.len() + body.len() + 10 <= cap) {
            Some(i) => i,
            None => {
                self.batches.push((d, Batch::default()));
                self.batches.len() - 1
            }
        };
        self.batches[i].1.push(type_id as u32, &body);
    }

    fn flush(&mut self) {
        for (d, b) in self.batches.drain(..) {
            // a batch too big for an unreliable packet is dropped, as the transport would
            let _ = self.peer.send(d, &b.finish());
        }
    }

    /// The messages in what the transport delivered since the last call, and its other events.
    fn take(&mut self) -> (Vec<Message>, Vec<lnl::Event>) {
        let (mut msgs, mut other) = (vec![], vec![]);
        while let Some(e) = self.peer.poll_event() {
            let lnl::Event::Message { data, .. } = e else {
                other.push(e);
                continue;
            };
            // a damaged batch is dropped whole, as Netcode does
            let Ok(list) = batch::read(&data) else { continue };
            for (type_id, body) in list {
                if let Some(kind) = self.order.get(type_id as usize) {
                    if let Ok(Some(m)) = Message::decode(*kind, body) {
                        msgs.push(m);
                    }
                }
            }
        }
        (msgs, other)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClientEvent {
    Approved { client_id: u64, payload: String },
    Refused(String),
    Named { hash: u16, payload: Vec<u8> },
    Disconnected(Reason),
}

pub struct Client {
    link: Link,
    data: Vec<u8>,
    client_id: Option<u64>,
    /// server time minus local time
    clock: Option<f64>,
    events: VecDeque<ClientEvent>,
}

impl Client {
    /// Starts connecting; `connection_data` goes in ConnectionRequest (the 2026 client's AuthPayload JSON).
    pub fn connect(now: f64, target: SocketAddr, connection_data: Vec<u8>) -> Self {
        // the 2026 transport's connect data: an empty .NET string
        let peer = Peer::connect(lnl::Config::default(), now, packet::dotnet_ticks_now(), 0, target, &[0, 0]);
        Client { link: Link::new(peer), data: connection_data, client_id: None, clock: None, events: VecDeque::new() }
    }

    pub fn client_id(&self) -> Option<u64> {
        self.client_id
    }

    /// The server's clock now, in seconds, once approved.
    pub fn server_time(&self, now: f64) -> Option<f64> {
        self.clock.map(|c| now + c)
    }

    pub fn rtt(&self) -> f64 {
        self.link.peer.rtt
    }

    pub fn receive(&mut self, now: f64, datagram: &[u8]) {
        if let Ok(p) = layer::unwrap(datagram) {
            self.link.peer.receive(now, p);
        }
    }

    pub fn send_named(&mut self, hash: u16, payload: &[u8], d: NgoDelivery) {
        self.link.queue(&Message::Named { hash, payload: payload.to_vec() }, d);
    }

    pub fn disconnect(&mut self, now: f64) {
        self.link.flush();
        self.link.peer.disconnect(now);
    }

    fn set_clock(&mut self, now: f64, tick: i32) {
        self.clock = Some(tick as f64 / TICK_RATE + self.link.peer.rtt / 2.0 - now);
    }

    pub fn update(&mut self, now: f64) {
        let (msgs, other) = self.link.take();
        for e in other {
            match e {
                lnl::Event::Connected => {
                    let request = Message::ConnectionRequest { versions: message::versions(&ORDER), config_hash: CONFIG_HASH, data: self.data.clone() };
                    self.link.queue(&request, NgoDelivery::ReliableFragmentedSequenced);
                }
                lnl::Event::Disconnected(r) => self.events.push_back(ClientEvent::Disconnected(r)),
                lnl::Event::Message { .. } => {}
            }
        }
        for m in msgs {
            match m {
                Message::ConnectionApproved { versions, client_id, tick, payload } => {
                    // from now on, type ids follow the server's table
                    let order: Option<Vec<Kind>> = versions.iter().map(|(h, _)| Kind::from_hash(*h)).collect();
                    if let Some(order) = order.filter(|o| o.len() == ORDER.len()) {
                        self.link.order = order;
                    }
                    self.client_id = Some(client_id);
                    self.set_clock(now, tick);
                    self.events.push_back(ClientEvent::Approved { client_id, payload });
                }
                Message::TimeSync { tick } => self.set_clock(now, tick),
                Message::DisconnectReason(r) => self.events.push_back(ClientEvent::Refused(r)),
                Message::Named { hash, payload } => self.events.push_back(ClientEvent::Named { hash, payload }),
                _ => {}
            }
        }
        self.link.flush();
        self.link.peer.update(now);
    }

    pub fn poll_transmit(&mut self) -> Option<Vec<u8>> {
        self.link.peer.poll_transmit().map(|p| layer::wrap(&p))
    }

    pub fn poll_event(&mut self) -> Option<ClientEvent> {
        self.events.pop_front()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServerEvent {
    /// approved; `data` is what the client sent in its ConnectionRequest
    Joined { client_id: u64, data: Vec<u8> },
    Left { client_id: u64 },
    Named { from: u64, hash: u16, payload: Vec<u8> },
}

struct Conn {
    link: Link,
    client_id: Option<u64>,
    /// a refused client is let go once its DisconnectReason had time to arrive
    drop_at: Option<f64>,
}

/// Decides on a ConnectionRequest's data: the approval payload, or the reason for refusing.
pub type Approve = Box<dyn FnMut(&[u8]) -> Result<String, String> + Send>;

pub struct Server {
    conns: HashMap<SocketAddr, Conn>,
    approve: Approve,
    next_client: u64,
    next_peer: i32,
    last_sync: f64,
    out: VecDeque<(SocketAddr, Vec<u8>)>,
    events: VecDeque<ServerEvent>,
}

impl Server {
    pub fn new(approve: Approve) -> Self {
        Server { conns: HashMap::new(), approve, next_client: 1, next_peer: 0, last_sync: 0.0, out: VecDeque::new(), events: VecDeque::new() }
    }

    pub fn tick(now: f64) -> i32 {
        (now * TICK_RATE) as i32
    }

    pub fn clients(&self) -> impl Iterator<Item = u64> + '_ {
        self.conns.values().filter_map(|c| c.client_id)
    }

    pub fn receive(&mut self, now: f64, from: SocketAddr, datagram: &[u8]) {
        let Ok(bytes) = layer::unwrap(datagram) else { return };
        let Some(p) = Packet::parse(bytes) else { return };
        let request = ConnectRequest::parse(&p).filter(|r| r.protocol == packet::PROTOCOL_ID);
        let number = match (self.conns.get_mut(&from), &request) {
            // same attempt, or ordinary traffic
            (Some(c), r) if r.as_ref().is_none_or(|r| r.connection_time == c.link.peer.connection_time()) => {
                return c.link.peer.receive(now, bytes);
            }
            // a new attempt from the same address replaces the old connection
            (Some(c), Some(_)) => (c.link.peer.connection_number() + 1) % 4,
            (None, Some(_)) => 0,
            _ => return,
        };
        if let Some(old) = self.conns.remove(&from).and_then(|c| c.client_id) {
            self.events.push_back(ServerEvent::Left { client_id: old });
        }
        let peer = Peer::accept(lnl::Config::default(), now, request.as_ref().unwrap(), number, self.next_peer);
        self.next_peer += 1;
        self.conns.insert(from, Conn { link: Link::new(peer), client_id: None, drop_at: None });
    }

    fn conn_of(&mut self, client_id: u64) -> Option<&mut Conn> {
        self.conns.values_mut().find(|c| c.client_id == Some(client_id))
    }

    pub fn send_named(&mut self, to: u64, hash: u16, payload: &[u8], d: NgoDelivery) {
        if let Some(c) = self.conn_of(to) {
            c.link.queue(&Message::Named { hash, payload: payload.to_vec() }, d);
        }
    }

    pub fn update(&mut self, now: f64) {
        let resync = now - self.last_sync >= RESYNC_INTERVAL;
        if resync {
            self.last_sync = now;
        }
        let Server { conns, approve, next_client, out, events, .. } = self;
        for (addr, c) in conns.iter_mut() {
            let (msgs, other) = c.link.take();
            for e in other {
                if let (lnl::Event::Disconnected(_), Some(id)) = (e, c.client_id) {
                    events.push_back(ServerEvent::Left { client_id: id });
                }
            }
            for m in msgs {
                match (m, c.client_id) {
                    (Message::ConnectionRequest { data, .. }, None) if c.drop_at.is_none() => match approve(&data) {
                        Ok(payload) => {
                            let client_id = *next_client;
                            *next_client += 1;
                            c.client_id = Some(client_id);
                            let approved = Message::ConnectionApproved { versions: message::versions(&ORDER), client_id, tick: Server::tick(now), payload };
                            c.link.queue(&approved, NgoDelivery::ReliableFragmentedSequenced);
                            c.link.queue(&Message::TimeSync { tick: Server::tick(now) }, NgoDelivery::Unreliable);
                            events.push_back(ServerEvent::Joined { client_id, data });
                        }
                        Err(reason) => {
                            c.link.queue(&Message::DisconnectReason(reason), NgoDelivery::ReliableSequenced);
                            c.drop_at = Some(now + 0.5);
                        }
                    },
                    (Message::Named { hash, payload }, Some(from)) => events.push_back(ServerEvent::Named { from, hash, payload }),
                    _ => {}
                }
            }
            if resync && c.client_id.is_some() {
                c.link.queue(&Message::TimeSync { tick: Server::tick(now) }, NgoDelivery::Unreliable);
            }
            c.link.flush();
            if c.drop_at.is_some_and(|t| now >= t) {
                c.drop_at = None;
                c.link.peer.disconnect(now);
            }
            c.link.peer.update(now);
            while let Some(p) = c.link.peer.poll_transmit() {
                out.push_back((*addr, layer::wrap(&p)));
            }
        }
        conns.retain(|_, c| {
            if !c.link.peer.is_closed() {
                return true;
            }
            // a timeout closes the peer in `update` above: its Disconnected event is still waiting
            let (_, other) = c.link.take();
            if let (true, Some(id)) = (other.iter().any(|e| matches!(e, lnl::Event::Disconnected(_))), c.client_id) {
                events.push_back(ServerEvent::Left { client_id: id });
            }
            false
        });
    }

    pub fn poll_transmit(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.out.pop_front()
    }

    pub fn poll_event(&mut self) -> Option<ServerEvent> {
        self.events.pop_front()
    }
}
