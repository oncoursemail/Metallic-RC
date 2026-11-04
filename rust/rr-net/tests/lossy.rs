//! A client and a server peer over a simulated link that drops and reorders datagrams, with the RR layer on.
use rr_net::{
    layer,
    lnl::{packet, Config, ConnectRequest, Delivery, Event, Packet, Peer, Property, Reason},
};

/// Deterministic xorshift, so a failure replays.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() % 10_000) as f64 / 10_000.0 < p
    }
    fn up_to(&mut self, max: f64) -> f64 {
        (self.next() % 10_000) as f64 / 10_000.0 * max
    }
}

/// Datagrams in flight one way: (arrival time, bytes).
struct Link {
    rng: Rng,
    drop: f64,
    max_delay: f64,
    queue: Vec<(f64, Vec<u8>)>,
    sent: usize,
}

impl Link {
    fn new(seed: u64, drop: f64, max_delay: f64) -> Self {
        Link { rng: Rng(seed), drop, max_delay, queue: vec![], sent: 0 }
    }
    fn send(&mut self, now: f64, packet: Vec<u8>) {
        self.sent += 1;
        if !self.rng.chance(self.drop) {
            // a base delay plus jitter: later datagrams often overtake earlier ones
            let at = now + 0.01 + self.rng.up_to(self.max_delay);
            self.queue.push((at, layer::wrap(&packet)));
        }
    }
    fn arrived(&mut self, now: f64) -> Vec<Vec<u8>> {
        let (ready, wait): (Vec<_>, Vec<_>) = self.queue.drain(..).partition(|(t, _)| *t <= now);
        self.queue = wait;
        ready.into_iter().map(|(_, d)| d).collect()
    }
}

struct Pair {
    now: f64,
    client: Peer,
    server: Option<Peer>,
    up: Link,
    down: Link,
    client_events: Vec<Event>,
    server_events: Vec<Event>,
}

impl Pair {
    fn new(drop: f64, max_delay: f64) -> Self {
        let client = Peer::connect(Config::default(), 0.0, packet::dotnet_ticks_now(), 7, "127.0.0.1:7777".parse().unwrap(), &[0, 0]);
        Pair { now: 0.0, client, server: None, up: Link::new(0x9E37_79B9_7F4A_7C15, drop, max_delay), down: Link::new(0xD1B5_4A32_D192_ED03, drop, max_delay), client_events: vec![], server_events: vec![] }
    }

    /// One 10 ms frame on both sides.
    fn step(&mut self) {
        self.now += 0.01;
        let now = self.now;
        for d in self.up.arrived(now) {
            let bytes = layer::unwrap(&d).unwrap();
            match &mut self.server {
                Some(s) => s.receive(now, bytes),
                // the server's listener: a connect request makes the peer
                None => {
                    let p = Packet::parse(bytes).unwrap();
                    if p.property == Property::ConnectRequest {
                        let r = ConnectRequest::parse(&p).unwrap();
                        assert_eq!(r.protocol, packet::PROTOCOL_ID);
                        self.server = Some(Peer::accept(Config::default(), now, &r, 1, 0));
                    }
                }
            }
        }
        for d in self.down.arrived(now) {
            self.client.receive(now, layer::unwrap(&d).unwrap());
        }
        self.client.update(now);
        while let Some(p) = self.client.poll_transmit() {
            self.up.send(now, p);
        }
        while let Some(e) = self.client.poll_event() {
            self.client_events.push(e);
        }
        if let Some(s) = &mut self.server {
            s.update(now);
            while let Some(p) = s.poll_transmit() {
                self.down.send(now, p);
            }
            while let Some(e) = s.poll_event() {
                self.server_events.push(e);
            }
        }
    }

    fn connect(&mut self) {
        while !self.client.is_connected() {
            assert!(self.now < 10.0, "no connection: {:?}", self.client_events);
            self.step();
        }
    }
}

fn messages(events: &[Event], delivery: Delivery) -> Vec<&Vec<u8>> {
    events.iter().filter_map(|e| match e {
        Event::Message { delivery: d, data } if *d == delivery => Some(data),
        _ => None,
    }).collect()
}

/// Message `i`: its number, then filler; every 50th is long enough to be sent in fragments.
fn message(i: u32) -> Vec<u8> {
    let len = if i % 50 == 0 { 3000 } else { 4 + (i % 200) as usize };
    let mut v = i.to_le_bytes().to_vec();
    v.extend((4..len).map(|k| (k as u32 ^ i) as u8));
    v
}

#[test]
fn ten_thousand_reliable_messages_arrive_in_order_through_loss_and_reordering() {
    let mut pair = Pair::new(0.2, 0.08);
    pair.connect();
    let mut last_sequenced = None;
    for i in 0..10_000u32 {
        pair.client.send(Delivery::ReliableOrdered, &message(i)).unwrap();
        pair.client.send(Delivery::Sequenced, &i.to_le_bytes()).unwrap();
        if i % 20 == 0 {
            pair.step();
        }
    }
    while !pair.client.is_idle() {
        assert!(pair.now < 600.0, "stuck: {} delivered", messages(&pair.server_events, Delivery::ReliableOrdered).len());
        pair.step();
        // sequenced messages never go back in time
        for m in messages(&pair.server_events, Delivery::Sequenced) {
            let n = u32::from_le_bytes(m[..4].try_into().unwrap());
            assert!(last_sequenced.is_none_or(|l| n > l), "sequenced {n} after {last_sequenced:?}");
            last_sequenced = Some(n);
        }
        pair.server_events.retain(|e| !matches!(e, Event::Message { delivery: Delivery::Sequenced, .. }));
    }
    for _ in 0..50 {
        pair.step();
    }
    let got = messages(&pair.server_events, Delivery::ReliableOrdered);
    assert_eq!(got.len(), 10_000);
    for (i, m) in got.iter().enumerate() {
        assert_eq!(**m, message(i as u32), "message {i}");
    }
    assert!(last_sequenced.is_some());
    assert!(pair.client.is_connected() && pair.server.as_ref().unwrap().is_connected());
    println!("{:.1} s simulated, {} datagrams up, {} down, rtt {:.3}", pair.now, pair.up.sent, pair.down.sent, pair.client.rtt);
}

#[test]
fn unordered_reliable_delivers_each_message_once() {
    let mut pair = Pair::new(0.2, 0.08);
    pair.connect();
    let server = pair.server.as_mut().unwrap();
    for i in 0..2_000u32 {
        server.send(Delivery::ReliableUnordered, &message(i)).unwrap();
    }
    while !pair.server.as_ref().unwrap().is_idle() {
        assert!(pair.now < 300.0);
        pair.step();
    }
    let mut got: Vec<u32> = messages(&pair.client_events, Delivery::ReliableUnordered).iter().map(|m| u32::from_le_bytes(m[..4].try_into().unwrap())).collect();
    got.sort();
    assert_eq!(got, (0..2_000).collect::<Vec<_>>());
}

#[test]
fn disconnect_and_timeout() {
    let mut pair = Pair::new(0.0, 0.0);
    pair.connect();
    pair.client.disconnect(pair.now);
    for _ in 0..20 {
        pair.step();
    }
    assert!(pair.client_events.contains(&Event::Disconnected(Reason::LocalClose)));
    assert!(pair.server_events.contains(&Event::Disconnected(Reason::RemoteClose)));

    // a server that goes silent: the client gives up after the 5 s timeout, and keeps pinging until then
    let mut pair = Pair::new(0.0, 0.0);
    pair.connect();
    pair.down.drop = 1.0;
    let start = pair.now;
    while !pair.client.is_closed() {
        pair.step();
    }
    assert!(pair.client_events.contains(&Event::Disconnected(Reason::Timeout)));
    assert!((pair.now - start - 5.0).abs() < 0.1, "{}", pair.now - start);

    // nobody answers at all: ten tries half a second apart, then failure
    let mut pair = Pair::new(1.0, 0.0);
    while !pair.client.is_closed() {
        pair.step();
    }
    assert_eq!(pair.client_events, [Event::Disconnected(Reason::ConnectionFailed)]);
    assert_eq!(pair.up.sent, 10);
}
