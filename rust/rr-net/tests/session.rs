//! Netcode sessions: two clients through lossy links to one server, a refusal, and a real UDP round trip.
use std::net::{SocketAddr, UdpSocket};

use rr_net::ngo::{Client, ClientEvent, NgoDelivery, Server, ServerEvent};

mod common;

use common::Link;

const SERVER: &str = "127.0.0.1:7777";

/// A server accepting any data that starts with `{`, payload `{"DR":"Carrot"}`.
fn server() -> Server {
    Server::new(Box::new(|data: &[u8]| {
        if data.starts_with(b"{") { Ok(r#"{"DR":"Carrot"}"#.to_string()) } else { Err("bad auth payload".into()) }
    }))
}

struct Player {
    addr: SocketAddr,
    client: Client,
    up: Link,
    down: Link,
    events: Vec<ClientEvent>,
}

struct World {
    now: f64,
    server: Server,
    players: Vec<Player>,
    server_events: Vec<ServerEvent>,
}

impl World {
    fn new() -> Self {
        World { now: 0.0, server: server(), players: vec![], server_events: vec![] }
    }

    fn join(&mut self, data: &[u8], drop: f64, max_delay: f64) {
        let n = self.players.len() as u64;
        self.players.push(Player {
            addr: format!("192.0.2.{}:5000", n + 1).parse().unwrap(),
            client: Client::connect(self.now, SERVER.parse().unwrap(), data.to_vec()),
            up: Link::new(0x1234_5678 + n, drop, max_delay),
            down: Link::new(0x8765_4321 + n, drop, max_delay),
            events: vec![],
        });
    }

    /// One 10 ms frame everywhere.
    fn step(&mut self) {
        self.now += 0.01;
        let now = self.now;
        for p in &mut self.players {
            for d in p.up.arrived(now) {
                self.server.receive(now, p.addr, &d);
            }
            for d in p.down.arrived(now) {
                p.client.receive(now, &d);
            }
            p.client.update(now);
            while let Some(d) = p.client.poll_transmit() {
                p.up.send(now, d);
            }
            while let Some(e) = p.client.poll_event() {
                p.events.push(e);
            }
        }
        self.server.update(now);
        while let Some((to, d)) = self.server.poll_transmit() {
            if let Some(p) = self.players.iter_mut().find(|p| p.addr == to) {
                p.down.send(now, d);
            }
        }
        while let Some(e) = self.server.poll_event() {
            self.server_events.push(e);
        }
    }

    fn run_until(&mut self, limit: f64, done: impl Fn(&World) -> bool) {
        while !done(self) {
            assert!(self.now < limit, "timed out at {:.1} s", self.now);
            self.step();
        }
    }
}

#[test]
fn two_clients_get_ids_one_and_two_and_keep_server_time() {
    let mut w = World::new();
    w.join(br#"{"AI":"1"}"#, 0.1, 0.03);
    w.run_until(10.0, |w| w.players[0].client.client_id().is_some());
    w.join(br#"{"AI":"2"}"#, 0.1, 0.03);
    w.run_until(20.0, |w| w.players[1].client.client_id().is_some());

    assert_eq!(w.players[0].client.client_id(), Some(1));
    assert_eq!(w.players[1].client.client_id(), Some(2));
    assert!(w.players[0].events.contains(&ClientEvent::Approved { client_id: 1, payload: r#"{"DR":"Carrot"}"#.into() }));
    assert_eq!(w.server_events[..2], [
        ServerEvent::Joined { client_id: 1, data: br#"{"AI":"1"}"#.to_vec() },
        ServerEvent::Joined { client_id: 2, data: br#"{"AI":"2"}"#.to_vec() },
    ]);

    // a minute and a half, through two resyncs: the clients' idea of server time stays within 50 ms
    let mut worst: f64 = 0.0;
    let end = w.now + 90.0;
    while w.now < end {
        w.step();
        for p in &w.players {
            // server time is the server's own clock, which is the simulation's clock here
            worst = worst.max((p.client.server_time(w.now).unwrap() - w.now).abs());
        }
    }
    assert!(worst < 0.05, "server time off by {worst:.3} s");
    println!("worst server time error {:.1} ms, rtt {:.3}", worst * 1000.0, w.players[0].client.rtt());

    // named messages both ways
    w.players[1].client.send_named(5, &[100, 1, 2], NgoDelivery::ReliableSequenced);
    w.run_until(end + 10.0, |w| w.server_events.iter().any(|e| matches!(e, ServerEvent::Named { .. })));
    assert!(w.server_events.contains(&ServerEvent::Named { from: 2, hash: 5, payload: vec![100, 1, 2] }));
    w.server.send_named(1, 6, &[101, 9], NgoDelivery::ReliableSequenced);
    w.run_until(end + 20.0, |w| w.players[0].events.iter().any(|e| matches!(e, ClientEvent::Named { .. })));
    assert!(w.players[0].events.contains(&ClientEvent::Named { hash: 6, payload: vec![101, 9] }));

    // a client leaves
    let now = w.now;
    w.players[0].client.disconnect(now);
    w.run_until(end + 30.0, |w| w.server_events.contains(&ServerEvent::Left { client_id: 1 }));
    assert_eq!(w.server.clients().collect::<Vec<_>>(), [2]);
}

#[test]
fn refused_client_hears_why_and_is_dropped() {
    let mut w = World::new();
    w.join(b"not json", 0.0, 0.0);
    w.run_until(5.0, |w| w.players[0].events.iter().any(|e| matches!(e, ClientEvent::Disconnected(_))));
    assert_eq!(w.players[0].events[0], ClientEvent::Refused("bad auth payload".into()));
    assert!(w.players[0].client.client_id().is_none());
    assert!(w.server_events.is_empty());
    assert_eq!(w.server.clients().count(), 0);
}

#[test]
fn over_real_udp_sockets() {
    let server_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    server_socket.set_nonblocking(true).unwrap();
    client_socket.set_nonblocking(true).unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let start = std::time::Instant::now();
    let now = || start.elapsed().as_secs_f64();
    let mut server = server();
    let mut client = Client::connect(now(), server_addr, br#"{"AI":"1"}"#.to_vec());
    let mut buf = [0u8; 2048];
    let mut echoed = None;
    while echoed.is_none() {
        assert!(now() < 5.0, "no echo over UDP");
        while let Ok((n, from)) = server_socket.recv_from(&mut buf) {
            server.receive(now(), from, &buf[..n]);
        }
        while let Ok(n) = client_socket.recv(&mut buf) {
            client.receive(now(), &buf[..n]);
        }
        server.update(now());
        while let Some(e) = server.poll_event() {
            // echo every named message back on the down channel
            if let ServerEvent::Named { from, payload, .. } = e {
                server.send_named(from, 6, &payload, NgoDelivery::ReliableSequenced);
            }
        }
        client.update(now());
        while let Some(e) = client.poll_event() {
            match e {
                ClientEvent::Approved { .. } => client.send_named(5, b"ping over udp", NgoDelivery::ReliableSequenced),
                ClientEvent::Named { payload, .. } => echoed = Some(payload),
                _ => {}
            }
        }
        while let Some((to, d)) = server.poll_transmit() {
            server_socket.send_to(&d, to).unwrap();
        }
        while let Some(d) = client.poll_transmit() {
            client_socket.send_to(&d, server_addr).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(echoed.as_deref(), Some(&b"ping over udp"[..]));
    assert_eq!(client.client_id(), Some(1));
}

#[test]
fn a_client_that_goes_silent_times_out_and_leaves() {
    let mut w = World::new();
    w.join(br#"{"AI":"1"}"#, 0.0, 0.0);
    w.join(br#"{"AI":"2"}"#, 0.0, 0.0);
    w.run_until(5.0, |w| w.players.iter().all(|p| p.client.client_id().is_some()));
    // both asked in the same frame, so which got id 1 is up to the server: take player 0's
    let (gone, stays) = (w.players[0].client.client_id().unwrap(), w.players[1].client.client_id().unwrap());
    // player 0 is gone without a word: everything it sends and is sent is lost
    w.players[0].up.drop = 1.0;
    w.players[0].down.drop = 1.0;
    w.run_until(15.0, |w| w.server_events.contains(&ServerEvent::Left { client_id: gone }));
    assert_eq!(w.server.clients().collect::<Vec<_>>(), [stays]);
}
