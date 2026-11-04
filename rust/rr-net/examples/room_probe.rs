//! room_probe <room server ADDR:PORT> <room> [seconds]
//!
//! Joins a room as a headless player that stands still and prints where it sees the others, once a second.
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use rr_net::room::RoomClient;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(addr), Some(room)) = (args.first(), args.get(1)) else {
        eprintln!("room_probe <room server ADDR:PORT> <room> [seconds]");
        std::process::exit(2)
    };
    let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let server = addr.to_socket_addrs().expect("address").next().expect("address");
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    socket.connect(server).unwrap();
    socket.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
    let start = Instant::now();
    let mut client = RoomClient::connect(0.0, server, room, "probe", [0.0, 0.0, 0.0]);
    let (mut buf, mut last, mut printed) = ([0u8; 2048], 0.0, 0.0);
    while start.elapsed().as_secs_f64() < secs {
        let got = socket.recv(&mut buf).ok();
        let now = start.elapsed().as_secs_f64();
        if let Some(n) = got {
            client.receive(now, &buf[..n]);
        }
        client.update(now);
        while let Some(d) = client.poll_transmit() {
            let _ = socket.send(&d);
        }
        while let Some(e) = client.poll_event() {
            println!("{now:6.2} {e:?}");
        }
        let shown = client.remotes(now, now - last);
        last = now;
        if now - printed >= 1.0 {
            printed = now;
            for s in shown {
                let [x, y, z] = s.body.position;
                let h = s.head.map(|h| h.position);
                println!("{now:6.2} actor {} {:>6}: feet ({x:.2}, {y:.2}, {z:.2}) head {h:.2?}", s.actor, s.name);
            }
        }
    }
    client.leave(start.elapsed().as_secs_f64());
}
