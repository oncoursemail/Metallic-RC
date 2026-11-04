//! Phase 3 check: headless players in one dorm instance over real UDP see each other's position within 0.1 m; a late
//! joiner gets the avatar from the event cache; when the master leaves, the other player takes over.
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use metallic_rc_server::room;
use rr_net::room::{RoomClient, RoomEvent, Shown};
use rr_net::sync::{AvatarData, Pose};

struct Player {
    socket: UdpSocket,
    client: RoomClient,
    events: Vec<RoomEvent>,
    /// where this player really is (feet)
    at: [f32; 3],
    seen: Vec<Shown>,
}

impl Player {
    fn new(now: f64, server: SocketAddr, name: &str) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let client = RoomClient::connect(now, server, "dorm-test", name, [0.0, 0.0, 0.0]);
        Player { socket, client, events: vec![], at: [0.0; 3], seen: vec![] }
    }

    fn pump(&mut self, server: SocketAddr, now: f64, dt: f64) {
        let mut buf = [0u8; 2048];
        while let Ok(n) = self.socket.recv(&mut buf) {
            self.client.receive(now, &buf[..n]);
        }
        self.client.update(now);
        while let Some(d) = self.client.poll_transmit() {
            let _ = self.socket.send_to(&d, server);
        }
        while let Some(e) = self.client.poll_event() {
            if let RoomEvent::Spawned { position, .. } = e {
                self.at = position;
            }
            self.events.push(e);
        }
        self.seen = self.client.remotes(now, dt);
    }

    fn walk_to(&mut self, at: [f32; 3], velocity: [f32; 3]) {
        self.at = at;
        let body = Pose { position: at, rotation: [0.0, 0.0, 0.0, 1.0], velocity };
        let head = Pose { position: [at[0], at[1] + 1.55, at[2]], ..body };
        self.client.set_pose(AvatarData { tracking_space: Some(body), head: Some(head), ..Default::default() });
    }

    fn sees(&self, actor: i32) -> Option<[f32; 3]> {
        self.seen.iter().find(|s| s.actor == actor).map(|s| s.body.position)
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

struct Clock(Instant);

impl Clock {
    fn now(&self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
}

/// Runs every player for `secs`, calling `step` before each frame with the frame's time.
fn run(players: &mut [&mut Player], server: SocketAddr, clock: &Clock, secs: f64, mut step: impl FnMut(&mut [&mut Player], f64)) {
    let end = clock.now() + secs;
    let mut last = clock.now();
    while clock.now() < end {
        let now = clock.now();
        step(players, now);
        for p in players.iter_mut() {
            p.pump(server, now, now - last);
        }
        last = now;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn two_players_see_each_other_move() {
    let server = room::listen("127.0.0.1:0").unwrap();
    let clock = Clock(Instant::now());
    let mut a = Player::new(clock.now(), server, "Ada");
    let mut b = Player::new(clock.now(), server, "Bo");

    run(&mut [&mut a, &mut b], server, &clock, 1.5, |_, _| {});
    assert_eq!((a.client.actor(), b.client.actor()), (Some(1), Some(2)));
    assert!(a.client.is_master() && !b.client.is_master());
    assert!(a.events.contains(&RoomEvent::PlayerJoined { actor: 2, name: "Bo".into() }), "{:?}", a.events);
    // Ada (master) spawned herself at the spawn point; Bo asked her and was put on the ring 1.2 m away
    assert!(a.client.is_spawned() && b.client.is_spawned(), "{:?} / {:?}", a.events, b.events);
    assert_eq!(a.at, [0.0, 0.0, 0.0]);
    assert!((dist(b.at, [0.0; 3]) - 1.2).abs() < 1e-4, "{:?}", b.at);
    assert_eq!(a.sees(2), Some(b.at));
    assert_eq!(b.sees(1), Some(a.at));
    assert_eq!(b.seen[0].name, "Ada");

    // both walk at 2 m/s for three seconds: Ada round a circle, Bo along x
    let (start, b0) = (clock.now(), b.at);
    let mut worst_moving: f32 = 0.0;
    run(&mut [&mut a, &mut b], server, &clock, 3.0, |ps, now| {
        let t = (now - start) as f32;
        let (s, c) = (t * 2.0 / 1.5).sin_cos();
        ps[0].walk_to([1.5 * s, 0.0, 1.5 * c - 1.5], [2.0 * c, 0.0, -2.0 * s]);
        ps[1].walk_to([b0[0] + 2.0 * t, 0.0, b0[2]], [2.0, 0.0, 0.0]);
        if t > 0.5 {
            worst_moving = worst_moving.max(dist(ps[0].sees(2).unwrap(), ps[1].at)).max(dist(ps[1].sees(1).unwrap(), ps[0].at));
        }
    });
    // then stand still
    let (a_at, b_at) = (a.at, b.at);
    a.walk_to(a_at, [0.0; 3]);
    b.walk_to(b_at, [0.0; 3]);
    run(&mut [&mut a, &mut b], server, &clock, 1.0, |_, _| {});
    let (ab, ba) = (dist(a.sees(2).unwrap(), b.at), dist(b.sees(1).unwrap(), a.at));
    println!("while walking at 2 m/s the other was shown up to {worst_moving:.3} m off; standing: {ab:.4} m and {ba:.4} m; rtt {:.4} s", a.client.rtt());
    assert!(ab < 0.1 && ba < 0.1, "{ab} {ba}");
    assert!(b.seen[0].head.is_some_and(|h| (h.position[1] - 1.55).abs() < 1e-4));

    // a late joiner gets both avatars from the room cache, at their spawn places, then where they are now
    let mut c = Player::new(clock.now(), server, "Cy");
    run(&mut [&mut a, &mut b, &mut c], server, &clock, 1.5, |_, _| {});
    assert_eq!(c.client.actor(), Some(3));
    assert!(c.client.is_spawned());
    assert!(dist(c.sees(1).unwrap(), a.at) < 0.1 && dist(c.sees(2).unwrap(), b.at) < 0.1, "{:?}", c.seen);
    assert!(a.sees(3).is_some() && b.sees(3).is_some());

    // the master leaves: the others hear it, Bo becomes master, and Ada's avatar is gone from the cache
    let now = clock.now();
    a.client.leave(now);
    run(&mut [&mut a, &mut b, &mut c], server, &clock, 1.0, |_, _| {});
    assert!(b.events.contains(&RoomEvent::PlayerLeft { actor: 1 }) && b.events.contains(&RoomEvent::MasterChanged { actor: 2 }), "{:?}", b.events);
    assert!(b.client.is_master() && c.client.master() == Some(2));
    assert!(b.sees(1).is_none() && c.sees(1).is_none());
    let mut d = Player::new(clock.now(), server, "Di");
    run(&mut [&mut b, &mut c, &mut d], server, &clock, 1.5, |_, _| {});
    assert!(d.client.is_spawned(), "{:?}", d.events);
    assert!(d.sees(1).is_none() && d.sees(2).is_some() && d.sees(3).is_some(), "{:?}", d.seen);
}

#[test]
fn a_connection_without_a_room_is_refused() {
    let server = room::listen("127.0.0.1:0").unwrap();
    let clock = Clock(Instant::now());
    let mut p = Player::new(clock.now(), server, "Ed");
    // swap in a client whose connection data names no room
    p.client = RoomClient::connect(clock.now(), server, "", "Ed", [0.0; 3]);
    run(&mut [&mut p], server, &clock, 1.5, |_, _| {});
    assert_eq!(p.events.first(), Some(&RoomEvent::Refused("no room instance in the connection data".into())));
    assert!(p.client.actor().is_none());
}
