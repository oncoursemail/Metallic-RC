//! The room connection, on its own thread as `net` is for HTTP: a UDP socket pumping an `rr_net::room::RoomClient`
//! that the game reads and feeds through a shared lock.
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rr_net::room::RoomClient;

/// The default room server: the API server's host, UDP port 4201.
pub fn default_address(api_url: &str) -> String {
    let rest = api_url.split_once("://").map_or(api_url, |(_, r)| r);
    let host_port = rest.split('/').next().unwrap_or(rest);
    let host = match host_port.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host_port,
    };
    format!("{host}:4201")
}

#[derive(Clone)]
pub struct Realtime {
    pub room: Arc<Mutex<RoomClient>>,
    start: Instant,
}

impl Realtime {
    /// The clock the room client runs on, in seconds.
    pub fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

/// Connects to the room server at `address` to join `room` as `name` (signed in with the access `token` from login),
/// handing out `spawn` (feet position, facing `yaw`) when this player is the master. The thread ends when the
/// connection does.
pub fn start(address: &str, room: &str, token: &str, name: &str, spawn: [f32; 3], yaw: f32) -> std::io::Result<Realtime> {
    let server: SocketAddr = address.to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other(format!("no address for {address}")))?;
    let socket = UdpSocket::bind(if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?;
    socket.connect(server)?;
    socket.set_read_timeout(Some(Duration::from_millis(5)))?;
    let start = Instant::now();
    let rt = Realtime { room: Arc::new(Mutex::new(RoomClient::connect_signed(0.0, server, room, Some(token), name, spawn).facing(yaw))), start };
    let shared = rt.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        loop {
            let got = socket.recv(&mut buf).ok();
            let now = shared.now();
            let mut room = shared.room.lock().unwrap();
            if let Some(n) = got {
                room.receive(now, &buf[..n]);
            }
            room.update(now);
            while let Some(d) = room.poll_transmit() {
                let _ = socket.send(&d);
            }
            if room.is_closed() {
                return;
            }
        }
    });
    Ok(rt)
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_address_takes_the_api_host() {
        assert_eq!(super::default_address("http://127.0.0.1:4200"), "127.0.0.1:4201");
        assert_eq!(super::default_address("https://rc.example.org/api"), "rc.example.org:4201");
        assert_eq!(super::default_address("localhost"), "localhost:4201");
    }
}
