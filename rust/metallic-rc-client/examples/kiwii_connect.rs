//! Minimal client that runs the Kiwii realtime handshake against the live realtime server, sends Input, and prints
//! the remote avatars it sees in the replication frames — the end-to-end check for the rr_net::kiwii transport
//! (ConnectRequest → ClientHello → ServerHello → key → Input → Frame) before wiring it into the Bevy client.
//!
//! Server: `C:\rrt-rt\debug\realtime-capture.exe realtime 0.0.0.0:7777`
//! Run two instances at different positions to see them find each other:
//!   cargo run --profile fast -p metallic-rc-client --example kiwii_connect -- 0,0,0
//!   cargo run --profile fast -p metallic-rc-client --example kiwii_connect -- 3,0,0
use std::collections::HashSet;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use rr_net::kiwii::{Client, DeviceProfile, Event};

fn main() {
    let server = std::env::var("KIWII_SERVER").unwrap_or_else(|_| "127.0.0.1:7777".into());
    let pos = std::env::args().nth(1).and_then(|s| {
        let p: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
        (p.len() == 3).then(|| [p[0], p[1], p[2]])
    }).unwrap_or([0.0, 0.0, 0.0]);

    let server_addr: std::net::SocketAddr = server.parse().expect("server addr");
    // Bind on loopback explicitly: a socket bound to 0.0.0.0 didn't receive the server's loopback replies on Windows
    // (the ConnectAccept was sent to 127.0.0.1:<ourport> but never arrived). Bind the same interface we send from.
    let bind = if server_addr.ip().is_loopback() { "127.0.0.1:0" } else { "0.0.0.0:0" };
    let socket = UdpSocket::bind(bind).expect("bind");
    // NOTE: no socket.connect() — a connected UDP socket on Windows drops replies whose source doesn't exactly match
    // the peer (the server is bound to 0.0.0.0), so use send_to/recv_from and accept the reply from any source.
    socket.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
    println!("kiwii_connect: {} -> {server_addr}, my pos {pos:?}", socket.local_addr().unwrap());

    let mut client = Client::new(DeviceProfile::PC);
    let n = socket.send_to(&client.connect_request(), server_addr).unwrap();
    println!("sent ConnectRequest ({n}B)");
    std::thread::sleep(Duration::from_millis(80)); // let the server accept the connection
    let n = socket.send_to(&client.client_hello(), server_addr).unwrap();
    println!("sent ClientHello ({n}B)");

    let (start, mut last_input) = (Instant::now(), Instant::now());
    let mut buf = [0u8; 2048];
    let mut seen: HashSet<u32> = HashSet::new();
    let mut frames = 0u32;
    let mut connected = false;

    while start.elapsed() < Duration::from_secs(10) {
        if client.is_connected() && last_input.elapsed() > Duration::from_millis(33) {
            let _ = socket.send_to(&client.input_datagram(pos, 0.0), server_addr); // ~30 Hz input
            last_input = Instant::now();
        }
        if let Ok((n, _src)) = socket.recv_from(&mut buf) {
            let dg = &buf[..n];
            println!("RECV {n}B trailer=0x{:02x} first3={:02x?}", dg.last().copied().unwrap_or(0), &dg[..n.min(3)]);
            if let Some(ev) = client.on_datagram(&buf[..n]) {
                match ev {
                    Event::Connected => {
                        connected = true;
                        println!("CONNECTED — Rabbit key derived, encryption on");
                    }
                    Event::Frame(f) => {
                        frames += 1;
                        for e in &f.entities {
                            if seen.insert(e.id) {
                                println!("frame {}: NEW entity id {} lod {:?} pos {:?}", f.server_tick, e.id, e.lod, e.pos);
                            }
                        }
                    }
                    Event::Transport => {}
                }
            }
            for dg in client.outbox.drain(..) {
                let _ = socket.send_to(&dg, server_addr);
            }
        }
    }
    println!("done: connected={connected} frames={frames} entities_seen={:?}", seen);
    if !connected {
        eprintln!("FAILED: never completed the handshake");
        std::process::exit(1);
    }
}
