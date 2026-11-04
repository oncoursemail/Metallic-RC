//! metallic-rc-server [--bind 127.0.0.1:4200] [--public-url http://127.0.0.1:4200] [--data-dir ./data]
//!                    [--room-bind 127.0.0.1:4201]
//!
//! `--public-url` is the address the client is told to use for every service (default: http://<bind>).
//! `--room-bind` is the UDP room server (default: the host of `--bind`, port 4201).
use std::path::PathBuf;

use metallic_rc_server::{room, router, App};

struct Args {
    bind: String,
    public_url: Option<String>,
    data_dir: PathBuf,
    room_bind: Option<String>,
}

fn args() -> Args {
    let mut a = Args { bind: "127.0.0.1:4200".into(), public_url: None, data_dir: PathBuf::from("data"), room_bind: None };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| die(&format!("{k} needs a value")));
        match k.as_str() {
            "--bind" => a.bind = v(),
            "--public-url" => a.public_url = Some(v()),
            "--data-dir" => a.data_dir = PathBuf::from(v()),
            "--room-bind" => a.room_bind = Some(v()),
            "-h" | "--help" => {
                println!("metallic-rc-server [--bind ADDR:PORT] [--public-url URL] [--data-dir DIR] [--room-bind ADDR:PORT]");
                std::process::exit(0)
            }
            _ => die(&format!("unknown argument {k}")),
        }
    }
    a
}

fn die(msg: &str) -> ! {
    eprintln!("metallic-rc-server: {msg}");
    std::process::exit(2)
}

#[tokio::main]
async fn main() {
    let a = args();
    let public_url = a.public_url.unwrap_or_else(|| format!("http://{}", a.bind));
    let app = App::open(&a.data_dir, &public_url).unwrap_or_else(|e| die(&format!("data dir {}: {e}", a.data_dir.display())));
    let room_bind = a.room_bind.unwrap_or_else(|| format!("{}:4201", a.bind.rsplit_once(':').map_or(a.bind.as_str(), |(h, _)| h)));
    let room_addr = room::listen(&room_bind).unwrap_or_else(|e| die(&format!("room bind {room_bind}: {e}")));
    println!("room server on udp {room_addr}");
    let listener = tokio::net::TcpListener::bind(&a.bind).await.unwrap_or_else(|e| die(&format!("bind {}: {e}", a.bind)));
    println!("metallic-rc-server on {} (name server: {}/?buildId=...)", a.bind, app.public_url);
    axum::serve(listener, router(app)).await.unwrap_or_else(|e| die(&format!("serve: {e}")));
}
