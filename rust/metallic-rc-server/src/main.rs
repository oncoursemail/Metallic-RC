//! metallic-rc-server [--bind 127.0.0.1:4200] [--public-url http://127.0.0.1:4200] [--data-dir ./data]
//!
//! `--public-url` is the address the client is told to use for every service (default: http://<bind>).
use std::path::PathBuf;

use metallic_rc_server::{router, App};

struct Args {
    bind: String,
    public_url: Option<String>,
    data_dir: PathBuf,
}

fn args() -> Args {
    let mut a = Args { bind: "127.0.0.1:4200".into(), public_url: None, data_dir: PathBuf::from("data") };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| die(&format!("{k} needs a value")));
        match k.as_str() {
            "--bind" => a.bind = v(),
            "--public-url" => a.public_url = Some(v()),
            "--data-dir" => a.data_dir = PathBuf::from(v()),
            "-h" | "--help" => {
                println!("metallic-rc-server [--bind ADDR:PORT] [--public-url URL] [--data-dir DIR]");
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
    let listener = tokio::net::TcpListener::bind(&a.bind).await.unwrap_or_else(|e| die(&format!("bind {}: {e}", a.bind)));
    println!("metallic-rc-server on {} (name server: {}/?buildId=...)", a.bind, app.public_url);
    axum::serve(listener, router(app)).await.unwrap_or_else(|e| die(&format!("serve: {e}")));
}
