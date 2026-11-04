//! Networked objects through the room server, in simulated time over lossy links (10 % loss, up to 30 ms of jitter
//! each way): instantiate with late joiners, transform sync, ownership takeover (also two claims at once), RPC
//! targets with buffering, owner and creator leaving, destroy, and the scene's own objects.
#[path = "../../rr-net/tests/common/mod.rs"]
mod common;

use std::net::SocketAddr;

use common::Link;
use metallic_rc_server::room::Rooms;
use rr_net::events::tagged::Value;
use rr_net::objects::{scene_view_id, ObjectEvent, RpcTarget};
use rr_net::room::RoomClient;
use rr_net::sync::Pose;

const STEP: f64 = 0.005;

struct Player {
    addr: SocketAddr,
    client: RoomClient,
    up: Link,
    down: Link,
    got: Vec<ObjectEvent>,
    shown: Vec<(i32, Pose)>,
    gone: bool,
}

struct World {
    rooms: Rooms,
    players: Vec<Player>,
    now: f64,
    seed: u64,
}

impl World {
    fn new() -> Self {
        World { rooms: Rooms::new(), players: vec![], now: 0.0, seed: 1 }
    }

    /// Adds a player; returns its index.
    fn join(&mut self, name: &str) -> usize {
        self.seed += 2;
        let addr: SocketAddr = format!("127.0.0.1:{}", 5000 + self.players.len()).parse().unwrap();
        let client = RoomClient::connect(self.now, "127.0.0.1:4201".parse().unwrap(), "objects", name, [0.0; 3]);
        let (up, down) = (Link::new(self.seed, 0.1, 0.03), Link::new(self.seed + 1, 0.1, 0.03));
        self.players.push(Player { addr, client, up, down, got: vec![], shown: vec![], gone: false });
        self.players.len() - 1
    }

    fn run(&mut self, secs: f64, mut each: impl FnMut(&mut [Player], f64)) {
        let end = self.now + secs;
        while self.now < end {
            self.now += STEP;
            let now = self.now;
            each(&mut self.players, now);
            for p in self.players.iter_mut().filter(|p| !p.gone) {
                for d in p.down.arrived(now) {
                    p.client.receive(now, &d);
                }
                p.client.update(now);
                while let Some(d) = p.client.poll_transmit() {
                    p.up.send(now, d);
                }
                while let Some(e) = p.client.poll_object_event() {
                    p.got.push(e);
                }
                p.shown = p.client.object_poses(now, STEP);
                for d in p.up.arrived(now) {
                    self.rooms.receive(now, p.addr, &d);
                }
            }
            self.rooms.update(now);
            while let Some((to, d)) = self.rooms.poll_transmit() {
                if let Some(p) = self.players.iter_mut().find(|p| p.addr == to && !p.gone) {
                    p.down.send(now, d);
                }
            }
        }
    }

    fn wait(&mut self, secs: f64) {
        self.run(secs, |_, _| {});
    }

    fn p(&mut self, i: usize) -> &mut Player {
        &mut self.players[i]
    }
}

impl Player {
    fn sees(&self, view: i32) -> Option<[f32; 3]> {
        self.shown.iter().find(|(v, _)| *v == view).map(|(_, p)| p.position)
    }
    fn spawned(&self, view: i32) -> Option<&ObjectEvent> {
        self.got.iter().find(|e| matches!(e, ObjectEvent::Spawned { view: v, .. } if *v == view))
    }
    fn rpcs(&self, method: &str) -> usize {
        self.got.iter().filter(|e| matches!(e, ObjectEvent::Rpc { method: m, .. } if m == method)).count()
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

fn at(position: [f32; 3], velocity: [f32; 3]) -> Pose {
    Pose { position, rotation: [0.0, 0.0, 0.0, 1.0], velocity }
}

/// Moves `view` (owned by player `i`) from `from` to `to` over `secs`, then leaves it there.
fn slide(w: &mut World, i: usize, view: i32, from: [f32; 3], to: [f32; 3], secs: f64) {
    let start = w.now;
    let v = [0, 1, 2].map(|k| (to[k] - from[k]) / secs as f32);
    w.run(secs, |ps, now| {
        let t = ((now - start) / secs).min(1.0) as f32;
        let p = [0, 1, 2].map(|k| from[k] + (to[k] - from[k]) * t);
        ps[i].client.set_object_pose(view, at(p, v));
    });
    w.p(i).client.set_object_pose(view, at(to, [0.0; 3]));
}

#[test]
fn objects_sync_change_hands_and_outlive_their_owner() {
    let mut w = World::new();
    let a = w.join("Ada");
    w.wait(1.0);
    let b = w.join("Bo");
    w.wait(2.0);
    assert_eq!((w.p(a).client.actor(), w.p(b).client.actor()), (Some(1), Some(2)));

    // Ada makes a cube; Bo gets it with its data, owned by Ada
    let now = w.now;
    let cube = w.p(a).client.instantiate(now, "Props/Cube", [1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0], [0.5; 3], vec![Value::Int(7)]).unwrap();
    assert!(w.p(a).client.is_mine(cube));
    w.wait(1.0);
    match w.p(b).spawned(cube) {
        Some(ObjectEvent::Spawned { address, owner: 1, scale, data, .. }) => {
            assert_eq!((address.as_str(), *scale, data.as_slice()), ("Props/Cube", [0.5; 3], &[Value::Int(7)][..]));
        }
        other => panic!("{other:?}"),
    }

    // she slides it 2 m: Bo sees it follow, then come to rest where she left it
    slide(&mut w, a, cube, [1.0, 1.0, 1.0], [3.0, 1.0, 1.0], 2.0);
    w.wait(1.5);
    assert!(dist(w.p(b).sees(cube).unwrap(), [3.0, 1.0, 1.0]) < 0.01, "{:?}", w.p(b).sees(cube));

    // a late joiner gets the cube from the cache, at where it is now
    let c = w.join("Cy");
    w.wait(2.5);
    assert!(matches!(w.p(c).spawned(cube), Some(ObjectEvent::Spawned { owner: 1, .. })), "{:?}", w.p(c).got);
    assert!(dist(w.p(c).sees(cube).unwrap(), [3.0, 1.0, 1.0]) < 0.01, "{:?}", w.p(c).sees(cube));

    // Bo grabs it and lifts it: everyone agrees he owns it and sees it move
    assert!(w.p(b).client.request_ownership(cube));
    slide(&mut w, b, cube, [3.0, 1.0, 1.0], [3.0, 2.0, 1.0], 1.0);
    w.wait(1.5);
    for i in [a, b, c] {
        assert_eq!(w.p(i).client.owner_of(cube), Some(2));
    }
    assert!(dist(w.p(a).sees(cube).unwrap(), [3.0, 2.0, 1.0]) < 0.01 && dist(w.p(c).sees(cube).unwrap(), [3.0, 2.0, 1.0]) < 0.01);

    // Ada and Cy grab it in the same instant: all three settle on the same owner (the higher actor wins a tie)
    w.p(a).client.request_ownership(cube);
    w.p(c).client.request_ownership(cube);
    w.wait(1.5);
    let owners: Vec<_> = [a, b, c].iter().map(|&i| w.players[i].client.owner_of(cube)).collect();
    assert_eq!(owners, vec![Some(3); 3]);

    // RPCs: buffered to all (the sender runs it at once too), and one to the master only
    let now = w.now;
    w.p(b).client.send_rpc(now, cube, "Paint", vec![Value::ByteArray(vec![1, 2, 3])], RpcTarget::AllBuffered);
    w.p(c).client.send_rpc(now, cube, "Ping", vec![], RpcTarget::Master);
    w.wait(1.0);
    assert_eq!([a, b, c].map(|i| w.players[i].rpcs("Paint")), [1, 1, 1]);
    assert_eq!([a, b, c].map(|i| w.players[i].rpcs("Ping")), [1, 0, 0]);
    assert!(w.p(a).got.contains(&ObjectEvent::Rpc { view: cube, sender: 2, method: "Paint".into(), params: vec![Value::ByteArray(vec![1, 2, 3])] }));

    // the owner (Cy) leaves: the cube stays and passes to the master, who carries on moving it
    let now = w.now;
    w.p(c).client.leave(now);
    w.wait(1.0);
    w.p(c).gone = true;
    assert_eq!((w.p(a).client.owner_of(cube), w.p(b).client.owner_of(cube)), (Some(1), Some(1)));
    assert!(w.p(b).got.contains(&ObjectEvent::OwnerChanged { view: cube, owner: 1 }));
    slide(&mut w, a, cube, [3.0, 2.0, 1.0], [0.0, 1.0, 0.0], 1.0);
    w.wait(1.5);
    assert!(dist(w.p(b).sees(cube).unwrap(), [0.0, 1.0, 0.0]) < 0.01);

    // a newcomer gets the cube, its owner, where it is, and the buffered RPC
    let d = w.join("Di");
    w.wait(2.5);
    assert!(matches!(w.p(d).spawned(cube), Some(ObjectEvent::Spawned { .. })));
    assert_eq!((w.p(d).client.owner_of(cube), w.p(d).rpcs("Paint")), (Some(1), 1));
    assert!(dist(w.p(d).sees(cube).unwrap(), [0.0, 1.0, 0.0]) < 0.01);

    // the creator (and master) leaves: the cube outlives her and goes to the new master, also for later joiners
    let now = w.now;
    w.p(a).client.leave(now);
    w.wait(1.0);
    w.p(a).gone = true;
    assert_eq!((w.p(b).client.owner_of(cube), w.p(d).client.owner_of(cube)), (Some(2), Some(2)));
    let e = w.join("Ed");
    w.wait(2.5);
    assert!(matches!(w.p(e).spawned(cube), Some(ObjectEvent::Spawned { owner: 2, .. })), "{:?}", w.p(e).got);
    assert!(dist(w.p(e).sees(cube).unwrap(), [0.0, 1.0, 0.0]) < 0.01);

    // destroyed: gone for everyone, and not handed to anyone who joins afterwards
    w.p(b).client.destroy(cube);
    w.wait(1.0);
    for i in [b, d, e] {
        assert!(w.players[i].got.contains(&ObjectEvent::Destroyed { view: cube }) && w.players[i].client.object_count() == 0);
    }
    let f = w.join("Fay");
    w.wait(2.5);
    assert!(w.p(f).client.is_spawned() && w.p(f).spawned(cube).is_none() && w.p(f).rpcs("Paint") == 0, "{:?}", w.p(f).got);
}

#[test]
fn scene_objects_are_shared_by_view_id() {
    let mut w = World::new();
    let a = w.join("Ada");
    w.wait(1.0);
    let b = w.join("Bo");
    w.wait(2.0);
    let chair = scene_view_id(4);
    for i in [a, b] {
        w.p(i).client.register_scene_object(chair, "Dorm/Chair", [0.0; 3], [0.0, 0.0, 0.0, 1.0]);
    }
    // the master owns it until Bo takes it and moves it
    assert!(w.p(a).client.is_mine(chair) && !w.p(b).client.is_mine(chair));
    w.p(b).client.request_ownership(chair);
    slide(&mut w, b, chair, [0.0; 3], [2.0, 0.0, -1.0], 1.0);
    w.wait(1.5);
    assert_eq!(w.p(a).client.owner_of(chair), Some(2));
    assert!(dist(w.p(a).sees(chair).unwrap(), [2.0, 0.0, -1.0]) < 0.01);

    // a newcomer whose room loads late learns the owner and the place it had before registering it
    let c = w.join("Cy");
    w.wait(2.5);
    w.p(c).client.register_scene_object(chair, "Dorm/Chair", [0.0; 3], [0.0, 0.0, 0.0, 1.0]);
    w.wait(0.1);
    assert_eq!(w.p(c).client.owner_of(chair), Some(2));
    assert!(dist(w.p(c).sees(chair).unwrap(), [2.0, 0.0, -1.0]) < 0.01, "{:?}", w.p(c).sees(chair));

    // the master destroys it: everyone drops it, and a later joiner hears so from the cache
    w.p(a).client.destroy(chair);
    w.wait(1.0);
    assert!(w.p(b).got.contains(&ObjectEvent::Destroyed { view: chair }) && w.p(c).client.object_count() == 0);
    let d = w.join("Di");
    w.wait(2.5);
    w.p(d).client.register_scene_object(chair, "Dorm/Chair", [0.0; 3], [0.0, 0.0, 0.0, 1.0]);
    assert!(w.p(d).client.is_spawned() && w.p(d).client.object_count() == 0);
}

#[test]
fn voice_reaches_the_rest_of_the_room_stamped_with_the_speaker() {
    let mut w = World::new();
    let a = w.join("Ada");
    w.wait(1.0);
    let (b, c) = (w.join("Bo"), w.join("Cy"));
    w.wait(2.0);
    for k in 0..50u8 {
        w.p(a).client.send_voice(vec![k; 40], 100);
        w.wait(0.02);
    }
    w.wait(0.5);
    let got = |w: &mut World, i: usize| std::iter::from_fn(|| w.p(i).client.poll_voice()).collect::<Vec<_>>();
    let (at_a, at_b, at_c) = (got(&mut w, a), got(&mut w, b), got(&mut w, c));
    assert!(at_a.is_empty());
    for heard in [at_b, at_c] {
        // unreliable: most frames arrive through 10 % loss each way, in order of sequence, from actor 1
        assert!(heard.len() >= 35 && heard.iter().all(|v| v.sender == 1 && v.amplitude == 100 && v.frame.len() == 40), "{}", heard.len());
        assert!(heard.windows(2).all(|p| p[0].sequence != p[1].sequence));
    }
}
