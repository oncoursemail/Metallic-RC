//! Fixed four-byte loopback probes. No supplied host, payload or command.
use crate::Result;
use metallic_external_guard_fixture::{
    classify_probe,
    kernel::{Operation, Record},
    Probe,
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub const PROBES: [Probe; 4] = [Probe::Tcp4, Probe::Tcp6, Probe::Udp4, Probe::Udp6];
pub fn exact_metadata(total: u64, prefix: &[u8], eof: bool) -> Option<[u8; 32]> {
    if eof && total == 32 && prefix.len() == 32 {
        Some(prefix.try_into().unwrap())
    } else {
        None
    }
}
pub fn parse(name: &str, port: &str) -> Result<(Probe, u16)> {
    let probe = PROBES
        .into_iter()
        .find(|p| p.name() == name)
        .ok_or("closed_probe_required")?;
    if port.is_empty()
        || port.len() > 5
        || !port.bytes().all(|b| b.is_ascii_digit())
        || port.starts_with('0')
    {
        return Err("closed_probe_port_required");
    }
    let port: u16 = port.parse().map_err(|_| "closed_probe_port_required")?;
    if port == 0 {
        return Err("closed_probe_port_required");
    }
    Ok((probe, port))
}
fn address(probe: Probe, port: u16) -> SocketAddr {
    SocketAddr::new(probe.host().parse().unwrap(), port)
}

/// Called only by the closed sibling-worker role, after its sole thread resumes.
/// UDP output has one exact MRU1 frame, including on a socket error. The parent
/// owns its only stdout reader and accepts the record only after EOF/exit.
pub fn child(probe: Probe, port: u16) -> i32 {
    if port == 0 {
        return 21;
    }
    let mut metadata = Record::new(probe, port);
    let exchange = (|| -> std::io::Result<bool> {
        if probe.tcp() {
            let mut socket =
                TcpStream::connect_timeout(&address(probe, port), Duration::from_millis(200))?;
            socket.set_read_timeout(Some(Duration::from_millis(200)))?;
            socket.set_write_timeout(Some(Duration::from_millis(200)))?;
            socket.write_all(b"MRC1")?;
            let mut response = [0; 4];
            socket.read_exact(&mut response)?;
            Ok(response == *b"MRC1")
        } else {
            let record = metadata.as_mut().unwrap();
            let socket = UdpSocket::bind(address(probe, 0))?;
            record.source_port = socket.local_addr()?.port();
            record.operation = Operation::ReadTimeout;
            socket.set_read_timeout(Some(Duration::from_millis(200)))?;
            record.operation = Operation::WriteTimeout;
            socket.set_write_timeout(Some(Duration::from_millis(200)))?;
            record.operation = Operation::Connect;
            socket.connect(address(probe, port))?;
            record.operation = Operation::LocalAddress;
            let local = socket.local_addr()?;
            if local != address(probe, record.source_port) {
                return Ok(false);
            }
            record.operation = Operation::Send;
            record.sent = socket.send(b"MRC1")?.try_into().unwrap_or(u8::MAX);
            if record.sent != 4 {
                return Ok(false);
            }
            record.operation = Operation::Receive;
            let mut response = [0; 4];
            let n = socket.recv(&mut response)?;
            record.operation = Operation::Reply;
            let good = n == 4 && response == *b"MRC1";
            if good {
                record.operation = Operation::Complete
            }
            Ok(good)
        }
    })();
    let code = classify_probe(exchange.map_err(|error| error.raw_os_error()));
    if let Some(record) = metadata {
        if std::io::stdout()
            .write_all(&record.encode())
            .and_then(|_| std::io::stdout().flush())
            .is_err()
        {
            return 21;
        }
    }
    code
}

pub struct Listener {
    pub port: u16,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<Result<()>>>,
}
impl Listener {
    pub fn new(probe: Probe) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (port, operation): (u16, Box<dyn FnOnce() -> Result<()> + Send>) = if probe.tcp() {
            let socket = TcpListener::bind(address(probe, 0))
                .map_err(|_| "owned_tcp_listener_unavailable")?;
            socket
                .set_nonblocking(true)
                .map_err(|_| "owned_listener_mode_failed")?;
            let port = socket
                .local_addr()
                .map_err(|_| "owned_listener_address_failed")?
                .port();
            (
                port,
                Box::new(move || {
                    let end = Instant::now() + Duration::from_secs(2);
                    while Instant::now() < end && rx.try_recv().is_err() {
                        match socket.accept() {
                            Ok((mut stream, peer)) => {
                                if !peer.ip().is_loopback() {
                                    return Err("owned_listener_peer_refused");
                                }
                                stream
                                    .set_nonblocking(false)
                                    .map_err(|_| "owned_listener_mode_failed")?;
                                stream
                                    .set_read_timeout(Some(Duration::from_millis(200)))
                                    .map_err(|_| "owned_listener_timeout_failed")?;
                                stream
                                    .set_write_timeout(Some(Duration::from_millis(200)))
                                    .map_err(|_| "owned_listener_timeout_failed")?;
                                let mut bytes = [0; 4];
                                if stream.read_exact(&mut bytes).is_ok() && bytes == *b"MRC1" {
                                    let _ = stream.write_all(b"MRC1");
                                }
                                break;
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => return Err("owned_listener_accept_failed"),
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    Ok(())
                }),
            )
        } else {
            let socket =
                UdpSocket::bind(address(probe, 0)).map_err(|_| "owned_udp_listener_unavailable")?;
            socket
                .set_nonblocking(true)
                .map_err(|_| "owned_listener_mode_failed")?;
            let port = socket
                .local_addr()
                .map_err(|_| "owned_listener_address_failed")?
                .port();
            (
                port,
                Box::new(move || {
                    let end = Instant::now() + Duration::from_secs(2);
                    let mut bytes = [0; 5];
                    while Instant::now() < end && rx.try_recv().is_err() {
                        match socket.recv_from(&mut bytes) {
                            Ok((4, peer)) if peer.ip().is_loopback() && bytes[..4] == *b"MRC1" => {
                                let _ = socket.send_to(b"MRC1", peer);
                                break;
                            }
                            Ok(_) => return Err("owned_listener_packet_refused"),
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => return Err("owned_listener_receive_failed"),
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    Ok(())
                }),
            )
        };
        let thread = thread::Builder::new()
            .name("owned-network-listener".into())
            .spawn(operation)
            .map_err(|_| "owned_listener_thread_failed")?;
        Ok(Self {
            port,
            stop: Some(tx),
            thread: Some(thread),
        })
    }
    pub fn finish(mut self) -> Result<()> {
        self.stop();
        self.join()
    }
    fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
    fn join(&mut self) -> Result<()> {
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "owned_listener_thread_panicked")??
        }
        Ok(())
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
        let _ = self.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_admission_is_fixed_protocol_and_decimal_nonzero_port() {
        for probe in PROBES {
            assert_eq!(parse(probe.name(), "4187"), Ok((probe, 4187)));
        }
        for (name, port) in [
            ("tcp", "4187"),
            ("http://example.com", "4187"),
            ("tcp4", "0"),
            ("udp6", "65536"),
            ("tcp4", "01"),
            ("tcp4", "+1"),
            ("tcp4", " 1"),
            ("tcp4", "1 --wait"),
            ("tcp4", ""),
        ] {
            assert!(parse(name, port).is_err());
        }
    }
    #[test]
    fn single_reader_metadata_requires_exact_total_and_eof() {
        let bytes = [7; 32];
        assert_eq!(exact_metadata(32, &bytes, true), Some(bytes));
        for (total, size, eof) in [
            (31, 31, true),
            (33, 32, true),
            (32, 31, true),
            (32, 32, false),
            (65536, 32, true),
            (32, 33, true),
        ] {
            assert_eq!(exact_metadata(total, &vec![7; size], eof), None);
        }
    }
    #[test]
    fn owned_tcp_roundtrip_and_listener_teardown() {
        for probe in [Probe::Tcp4, Probe::Tcp6] {
            let listener = Listener::new(probe).unwrap();
            assert_eq!(child(probe, listener.port), 0);
            listener.finish().unwrap();
        }
    }
}
