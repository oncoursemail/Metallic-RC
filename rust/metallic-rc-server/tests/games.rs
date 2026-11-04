//! Paintball through the room server, in simulated time over lossy links (5 % loss, up to 30 ms of jitter each
//! way): a match started by one player reaches both, the server reads where each player stands from their movement,
//! a paintball fired by one player puts the other out (hit sent to both, score up), the out player is moved back to
//! a team spawn, and bots added to the room show up in the state.
#[path = "../../rr-net/tests/common/mod.rs"]
mod common;

use std::net::SocketAddr;

use common::Link;
use metallic_rc_server::room::Rooms;
use rr_net::games::{Down, MapInfo, Mode, Phase, Spot, State, Up, BLUE, RED};
use rr_net::room::RoomClient;
use rr_net::sync::{AvatarData, Pose};

const STEP: f64 = 0.005;

struct Player {
    addr: SocketAddr,
    client: RoomClient,
    up: Link,
    down: Link,
    got: Vec<Down>,
}

impl Player {
    fn state(&self) -> Option<&State> {
        self.got.iter().rev().find_map(|d| match d {
            Down::State(s) => Some(s),
            _ => None,
        })
    }
}

fn map() -> MapInfo {
    MapInfo {
        name: "test".into(),
        lobby: vec![Spot { pos: [0.0, 0.0, 0.0], yaw: 0.0 }],
        spawns: [vec![Spot { pos: [0.0, 0.0, -20.0], yaw: std::f32::consts::PI }], vec![Spot { pos: [0.0, 0.0, 20.0], yaw: 0.0 }]],
        flags: [[0.0, 1.0, -22.0], [0.0, 1.0, 22.0]],
        goals: [[0.0, 0.8, -22.0], [0.0, 0.8, 22.0]],
        bounds: None,
    }
}

fn standing(feet: [f32; 3]) -> AvatarData {
    AvatarData { tracking_space: Some(Pose { position: feet, ..Default::default() }), ..Default::default() }
}

#[test]
fn a_match_through_the_room_server() {
    let mut rooms = Rooms::new();
    let mut players: Vec<Player> = vec![];
    for (i, name) in ["Ada", "Bo"].iter().enumerate() {
        let addr: SocketAddr = format!("127.0.0.1:{}", 6000 + i).parse().unwrap();
        let client = RoomClient::connect(0.0, "127.0.0.1:4201".parse().unwrap(), "paintball", name, [0.0; 3]);
        players.push(Player { addr, client, up: Link::new(11 + 2 * i as u64, 0.05, 0.03), down: Link::new(12 + 2 * i as u64, 0.05, 0.03), got: vec![] });
    }
    let mut now = 0.0;
    let mut run = |players: &mut Vec<Player>, rooms: &mut Rooms, secs: f64, each: &mut dyn FnMut(&mut Vec<Player>, f64)| {
        let end = now + secs;
        while now < end {
            now += STEP;
            each(players, now);
            for p in players.iter_mut() {
                for d in p.down.arrived(now) {
                    p.client.receive(now, &d);
                }
                p.client.update(now);
                while let Some(d) = p.client.poll_transmit() {
                    p.up.send(now, d);
                }
                while p.client.poll_event().is_some() {}
                while let Some(g) = p.client.poll_game() {
                    p.got.push(Down::decode(&g).unwrap());
                }
                for d in p.up.arrived(now) {
                    rooms.receive(now, p.addr, &d);
                }
            }
            rooms.update(now);
            while let Some((to, d)) = rooms.poll_transmit() {
                if let Some(p) = players.iter_mut().find(|p| p.addr == to) {
                    p.down.send(now, d);
                }
            }
        }
        now
    };
    run(&mut players, &mut rooms, 3.0, &mut |_, _| {});
    assert!(players.iter().all(|p| p.client.is_spawned()), "both in the room");
    let (ada, bo) = (players[0].client.actor().unwrap(), players[1].client.actor().unwrap());

    players[0].client.send_game(&Up::Start { mode: Mode::TeamBattle, target: 5, seconds: 120, map: map() });
    run(&mut players, &mut rooms, 0.5, &mut |_, _| {});
    for p in &players {
        assert!(p.got.iter().any(|d| matches!(d, Down::Map(m) if m.name == "test")), "map reached everyone");
        assert_eq!(p.state().map(|s| s.phase), Some(Phase::Countdown));
    }
    let team = |p: &Player, id: i32| p.state().unwrap().members.iter().find(|m| m.id == id).map(|m| m.team);
    assert_eq!((team(&players[0], ada), team(&players[0], bo)), (Some(RED), Some(BLUE)));

    // both stand 10 m apart while the countdown runs out (movement is how the server knows where they are)
    let t = run(&mut players, &mut rooms, 5.5, &mut |ps, _| {
        ps[0].client.set_pose(standing([0.0, 0.0, -10.0]));
        ps[1].client.set_pose(standing([0.0, 0.0, 0.0]));
    });
    assert_eq!(players[1].state().map(|s| s.phase), Some(Phase::Playing));
    let _ = t;
    players[0].client.send_game(&Up::Fire { origin: [0.0, 1.4, -9.5], velocity: [0.0, 0.0, 33.0], life: 2.0 });
    run(&mut players, &mut rooms, 1.0, &mut |ps, _| {
        ps[0].client.set_pose(standing([0.0, 0.0, -10.0]));
        ps[1].client.set_pose(standing([0.0, 0.0, 0.0]));
    });
    for p in &players {
        assert!(p.got.iter().any(|d| matches!(d, Down::Hit { shooter, victim, .. } if *shooter == ada && *victim == bo)), "everyone hears of the hit");
    }
    assert!(players[1].got.iter().any(|d| matches!(d, Down::Shot { shooter, .. } if *shooter == ada)), "the other player draws the shot");
    assert_eq!(players[1].state().unwrap().scores, [1, 0]);
    // out for a moment, then back at blue's spawn
    run(&mut players, &mut rooms, 3.5, &mut |_, _| {});
    assert!(players[1].got.iter().any(|d| matches!(d, Down::Respawn { id, spot } if *id == bo && spot.pos == [0.0, 0.0, 20.0])));
    assert!(!players[1].state().unwrap().members.iter().any(|m| m.out));

    // bots join the state
    players[1].client.send_game(&Up::Bots { count: 2 });
    run(&mut players, &mut rooms, 0.5, &mut |_, _| {});
    let s = players[0].state().unwrap();
    assert_eq!(s.members.iter().filter(|m| m.bot.is_some()).count(), 2);
    assert!(rooms.games().get("paintball").is_some());
}
