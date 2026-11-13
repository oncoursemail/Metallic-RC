//! One end of a LiteNetLib connection, without I/O: feed it received packets with [`Peer::receive`], call
//! [`Peer::update`] every frame, and send what [`Peer::poll_transmit`] gives out. Time is seconds from any start.
//!
//! Everything goes on channel 0, as the 2026 client's transport does. Reliable packets travel in a window of 64
//! sequence numbers; the receiver acks the window as a bitfield (bit `seq % 64`) behind the first sequence it still
//! waits for, and the sender resends whatever stays unacked for about two round trips.
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

use super::packet::{self as pk, ConnectRequest, Delivery, Packet, Property, ACK_BYTES, MAX_SEQUENCE};
use crate::layer;

#[derive(Clone, Debug)]
pub struct Config {
    pub ping_interval: f64,
    /// silence after which the peer is dropped
    pub timeout: f64,
    pub reconnect_delay: f64,
    pub max_connect_attempts: u32,
    /// UDP payload size this side keeps to (LiteNetLib's smallest safe MTU)
    pub mtu: usize,
}

impl Default for Config {
    /// The 2026 client's LiteNetLibTransport defaults.
    fn default() -> Self {
        Config { ping_interval: 1.0, timeout: 5.0, reconnect_delay: 0.5, max_connect_attempts: 10, mtu: 508 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    ConnectionFailed,
    Timeout,
    RemoteClose,
    LocalClose,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Connected,
    Message { delivery: Delivery, data: Vec<u8> },
    Disconnected(Reason),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    /// unreliable messages are not split; this one does not fit one packet
    TooBig,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Connecting { attempts: u32, last_sent: f64 },
    Connected,
    ShuttingDown { since: f64, last_sent: f64 },
    Closed,
}

const WINDOW: usize = 64;
type Fragment = Option<(u16, u16, u16)>;

struct InFlight {
    seq: u16,
    bytes: Vec<u8>,
    sent_at: Option<f64>,
    acked: bool,
}

/// A reliable channel, both directions.
struct Reliable {
    delivery: Delivery,
    waiting: VecDeque<(Fragment, Vec<u8>)>,
    in_flight: VecDeque<InFlight>,
    next_seq: u16,
    /// first sequence not yet received (everything before it was)
    remote_start: u16,
    got: [bool; WINDOW],
    held: Vec<Option<(Fragment, Vec<u8>)>>,
    ack_due: bool,
}

impl Reliable {
    fn new(delivery: Delivery) -> Self {
        Reliable {
            delivery,
            waiting: VecDeque::new(),
            in_flight: VecDeque::new(),
            next_seq: 0,
            remote_start: 0,
            got: [false; WINDOW],
            held: (0..WINDOW).map(|_| None).collect(),
            ack_due: false,
        }
    }

    fn channel_id(&self) -> u8 {
        self.delivery as u8
    }

    /// Fills the window from the queue and (re)sends what is due.
    fn pump(&mut self, conn: u8, now: f64, resend: f64, out: &mut VecDeque<Vec<u8>>) {
        while self.in_flight.len() < WINDOW {
            let Some((frag, body)) = self.waiting.pop_front() else { break };
            let seq = self.next_seq;
            self.next_seq = (self.next_seq + 1) % MAX_SEQUENCE;
            self.in_flight.push_back(InFlight { seq, bytes: pk::channeled(conn, seq, self.channel_id(), frag, &body), sent_at: None, acked: false });
        }
        for p in self.in_flight.iter_mut().filter(|p| !p.acked) {
            if p.sent_at.is_none_or(|t| now - t >= resend) {
                out.push_back(p.bytes.clone());
                p.sent_at = Some(now);
            }
        }
        if self.ack_due {
            let mut bits = [0u8; ACK_BYTES];
            for (i, _) in self.got.iter().enumerate().filter(|(_, g)| **g) {
                bits[i / 8] |= 1 << (i % 8);
            }
            out.push_back(pk::ack(conn, self.remote_start, self.channel_id(), &bits));
            self.ack_due = false;
        }
    }

    fn on_ack(&mut self, start: u16, bits: &[u8]) {
        for p in self.in_flight.iter_mut() {
            let d = pk::seq_diff(p.seq, start);
            let i = p.seq as usize % WINDOW;
            if d < 0 || (d < WINDOW as i32 && bits.get(i / 8).is_some_and(|b| b & (1 << (i % 8)) != 0)) {
                p.acked = true;
            }
        }
        while self.in_flight.front().is_some_and(|p| p.acked) {
            self.in_flight.pop_front();
        }
    }

    /// A received packet; returns what is now deliverable, in order for the ordered channel.
    fn on_packet(&mut self, seq: u16, frag: Fragment, body: &[u8]) -> Vec<(Fragment, Vec<u8>)> {
        self.ack_due = true;
        let d = pk::seq_diff(seq, self.remote_start);
        let i = seq as usize % WINDOW;
        if d < 0 || d >= WINDOW as i32 || self.got[i] {
            return vec![];
        }
        self.got[i] = true;
        let mut out = vec![];
        if self.delivery == Delivery::ReliableOrdered {
            self.held[i] = Some((frag, body.to_vec()));
        } else {
            out.push((frag, body.to_vec()));
        }
        loop {
            let j = self.remote_start as usize % WINDOW;
            if !self.got[j] {
                break;
            }
            if let Some(m) = self.held[j].take() {
                out.push(m);
            }
            self.got[j] = false;
            self.remote_start = (self.remote_start + 1) % MAX_SEQUENCE;
        }
        out
    }

    fn idle(&self) -> bool {
        self.waiting.is_empty() && self.in_flight.is_empty()
    }
}

pub struct Peer {
    cfg: Config,
    state: State,
    connection_time: i64,
    conn: u8,
    /// the other side's peer id, from its request or accept
    pub remote_id: i32,
    hello: Vec<u8>,
    last_recv: f64,
    ping_seq: u16,
    ping_sent: Option<f64>,
    last_ping: f64,
    /// smoothed round trip, seconds
    pub rtt: f64,
    reliable: [Reliable; 2],
    seq_out: u16,
    seq_in: Option<u16>,
    next_fragment: u16,
    fragments: HashMap<(u8, u16), Vec<Option<Vec<u8>>>>,
    out: VecDeque<Vec<u8>>,
    events: VecDeque<Event>,
}

impl Peer {
    fn new(cfg: Config, state: State, connection_time: i64, hello: Vec<u8>, now: f64) -> Self {
        Peer {
            cfg,
            state,
            connection_time,
            conn: 0,
            remote_id: -1,
            hello,
            last_recv: now,
            ping_seq: 0,
            ping_sent: None,
            last_ping: now,
            rtt: 0.1,
            reliable: [Reliable::new(Delivery::ReliableUnordered), Reliable::new(Delivery::ReliableOrdered)],
            seq_out: 0,
            seq_in: None,
            next_fragment: 0,
            fragments: HashMap::new(),
            out: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    /// A client starting to connect to `target`; `connection_time` tells this attempt apart (the 2026 client uses
    /// `DateTime.UtcNow.Ticks`, see [`pk::dotnet_ticks_now`]).
    pub fn connect(cfg: Config, now: f64, connection_time: i64, local_id: i32, target: SocketAddr, data: &[u8]) -> Self {
        let request = pk::connect_request(connection_time, local_id, target, data);
        let mut p = Peer::new(cfg, State::Connecting { attempts: 1, last_sent: now }, connection_time, request.clone(), now);
        p.out.push_back(request);
        p
    }

    /// The server's side of a connection it accepts, numbered `connection` (0..3, changes on each reconnect from the
    /// same address).
    pub fn accept(cfg: Config, now: f64, request: &ConnectRequest, connection: u8, local_id: i32) -> Self {
        let accept = pk::connect_accept(request.connection_time, connection, local_id);
        let mut p = Peer::new(cfg, State::Connected, request.connection_time, accept.clone(), now);
        p.conn = connection;
        p.remote_id = request.peer_id;
        p.out.push_back(accept);
        p.events.push_back(Event::Connected);
        p
    }

    pub fn is_connected(&self) -> bool {
        self.state == State::Connected
    }

    pub fn is_closed(&self) -> bool {
        self.state == State::Closed
    }

    pub fn connection_time(&self) -> i64 {
        self.connection_time
    }

    pub fn connection_number(&self) -> u8 {
        self.conn
    }

    /// Nothing reliable waiting or unacknowledged.
    pub fn is_idle(&self) -> bool {
        self.reliable.iter().all(Reliable::idle)
    }

    /// Largest message body that fits one packet.
    fn max_body(&self) -> usize {
        self.cfg.mtu - layer::EXTRA - pk::header_size(Property::Channeled)
    }

    pub fn send(&mut self, delivery: Delivery, data: &[u8]) -> Result<(), SendError> {
        if matches!(self.state, State::Closed | State::ShuttingDown { .. }) {
            return Err(SendError::Closed);
        }
        let max = self.max_body();
        match delivery {
            Delivery::Unreliable | Delivery::Sequenced if data.len() > max => Err(SendError::TooBig),
            Delivery::Unreliable => {
                if self.state == State::Connected {
                    self.out.push_back(pk::unreliable(self.conn, data));
                }
                Ok(())
            }
            Delivery::Sequenced => {
                if self.state == State::Connected {
                    self.out.push_back(pk::channeled(self.conn, self.seq_out, Delivery::Sequenced as u8, None, data));
                    self.seq_out = (self.seq_out + 1) % MAX_SEQUENCE;
                }
                Ok(())
            }
            Delivery::ReliableUnordered | Delivery::ReliableOrdered => {
                let ch = &mut self.reliable[(delivery == Delivery::ReliableOrdered) as usize];
                if data.len() <= max {
                    ch.waiting.push_back((None, data.to_vec()));
                    return Ok(());
                }
                let part = max - pk::FRAGMENT_HEADER;
                let total = data.len().div_ceil(part);
                if total > u16::MAX as usize {
                    return Err(SendError::TooBig);
                }
                let id = self.next_fragment;
                self.next_fragment = self.next_fragment.wrapping_add(1);
                for (i, chunk) in data.chunks(part).enumerate() {
                    ch.waiting.push_back((Some((id, i as u16, total as u16)), chunk.to_vec()));
                }
                Ok(())
            }
        }
    }

    pub fn disconnect(&mut self, now: f64) {
        match self.state {
            State::Connected => {
                self.out.push_back(pk::disconnect(self.conn, self.connection_time, &[]));
                self.state = State::ShuttingDown { since: now, last_sent: now };
            }
            State::Connecting { .. } => self.close(Reason::LocalClose),
            _ => {}
        }
    }

    fn close(&mut self, why: Reason) {
        self.state = State::Closed;
        self.events.push_back(Event::Disconnected(why));
    }

    pub fn update(&mut self, now: f64) {
        match self.state {
            State::Connecting { attempts, last_sent } if now - last_sent >= self.cfg.reconnect_delay => {
                if attempts >= self.cfg.max_connect_attempts {
                    return self.close(Reason::ConnectionFailed);
                }
                self.out.push_back(self.hello.clone());
                self.state = State::Connecting { attempts: attempts + 1, last_sent: now };
            }
            State::Connected => {
                if now - self.last_recv > self.cfg.timeout {
                    return self.close(Reason::Timeout);
                }
                if now - self.last_ping >= self.cfg.ping_interval {
                    self.ping_seq = self.ping_seq.wrapping_add(1);
                    self.out.push_back(pk::ping(self.conn, self.ping_seq));
                    self.ping_sent = Some(now);
                    self.last_ping = now;
                }
                let resend = 0.025 + self.rtt * 2.1;
                for ch in &mut self.reliable {
                    ch.pump(self.conn, now, resend, &mut self.out);
                }
            }
            State::ShuttingDown { since, last_sent } => {
                if now - since > self.cfg.timeout {
                    return self.close(Reason::LocalClose);
                }
                if now - last_sent >= self.cfg.reconnect_delay {
                    self.out.push_back(pk::disconnect(self.conn, self.connection_time, &[]));
                    self.state = State::ShuttingDown { since, last_sent: now };
                }
            }
            _ => {}
        }
    }

    /// A transport packet from the other side (the RR layer already taken off).
    pub fn receive(&mut self, now: f64, bytes: &[u8]) {
        let Some(p) = Packet::parse(bytes) else { return };
        if self.state == State::Closed {
            return;
        }
        let ours = p.connection == self.conn;
        match p.property {
            Property::ConnectAccept => {
                if matches!(self.state, State::Connecting { .. }) && p.i64_at(1) == self.connection_time {
                    self.conn = p.bytes[9] & 3;
                    self.remote_id = p.i32_at(11);
                    self.state = State::Connected;
                    self.last_ping = now;
                    self.events.push_back(Event::Connected);
                }
            }
            // our accept was lost: the client asks again
            Property::ConnectRequest => {
                if ConnectRequest::parse(&p).is_some_and(|r| r.connection_time == self.connection_time) {
                    self.out.push_back(self.hello.clone());
                }
            }
            Property::Disconnect => {
                if p.i64_at(1) == self.connection_time {
                    self.out.push_back(pk::bare(Property::ShutdownOk, self.conn));
                    self.close(Reason::RemoteClose);
                }
                return;
            }
            Property::ShutdownOk => {
                if matches!(self.state, State::ShuttingDown { .. }) {
                    self.close(Reason::LocalClose);
                }
                return;
            }
            _ if !ours || self.state != State::Connected => return,
            Property::Merged => {
                for inner in pk::split_merged(bytes) {
                    self.receive(now, inner);
                }
            }
            Property::Unreliable => self.events.push_back(Event::Message { delivery: Delivery::Unreliable, data: p.body().to_vec() }),
            Property::Channeled => self.on_channeled(&p),
            Property::Ack => {
                if let Some(ch) = self.reliable.iter_mut().find(|c| c.channel_id() == p.channel_id()) {
                    ch.on_ack(p.sequence(), p.body());
                }
            }
            Property::Ping => self.out.push_back(pk::pong(self.conn, p.sequence(), pk::dotnet_ticks_now())),
            Property::Pong => {
                if let (true, Some(t)) = (p.sequence() == self.ping_seq, self.ping_sent.take()) {
                    self.rtt = self.rtt * 0.8 + (now - t) * 0.2;
                }
            }
            // the same bytes back, marked as answered
            Property::MtuCheck => {
                let mut v = bytes.to_vec();
                v[0] = pk::first_byte(Property::MtuOk, self.conn, false);
                self.out.push_back(v);
            }
            _ => {}
        }
        self.last_recv = now;
    }

    fn on_channeled(&mut self, p: &Packet) {
        // channel 0 only, as the 2026 transport sends
        let id = p.channel_id();
        let Some(delivery) = Delivery::from_channel_id(id).filter(|_| id < 4) else { return };
        let frag = p.fragmented.then(|| p.fragment());
        if delivery == Delivery::Sequenced {
            if self.seq_in.is_none_or(|last| pk::seq_diff(p.sequence(), last) > 0) {
                self.seq_in = Some(p.sequence());
                self.events.push_back(Event::Message { delivery, data: p.body().to_vec() });
            }
            return;
        }
        let ready = self.reliable[(delivery == Delivery::ReliableOrdered) as usize].on_packet(p.sequence(), frag, p.body());
        for (frag, body) in ready {
            match frag {
                None => self.events.push_back(Event::Message { delivery, data: body }),
                Some((fid, part, total)) => {
                    let parts = self.fragments.entry((id, fid)).or_insert_with(|| vec![None; total as usize]);
                    if let Some(slot) = parts.get_mut(part as usize) {
                        *slot = Some(body);
                    }
                    if parts.iter().all(Option::is_some) {
                        let data = self.fragments.remove(&(id, fid)).unwrap().into_iter().flatten().flatten().collect();
                        self.events.push_back(Event::Message { delivery, data });
                    }
                }
            }
        }
    }

    /// The next transport packet to send (wrap it with [`layer::wrap`]).
    pub fn poll_transmit(&mut self) -> Option<Vec<u8>> {
        self.out.pop_front()
    }

    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
}
