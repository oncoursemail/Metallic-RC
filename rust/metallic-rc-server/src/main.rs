//! metallic-rc-server [--bind 127.0.0.1:4200] [--public-url http://127.0.0.1:4200] [--data-dir ./data]
//!                    [--room-bind 127.0.0.1:4201] [--open-rooms]
//!
//! `--public-url` is the address the client is told to use for every service (default: http://<bind>).
//! `--room-bind` is the UDP room server (default: the host of `--bind`, port 4201). It lets in only players who send
//! the access token from login, unless `--open-rooms` (tools and old clients).
use std::path::PathBuf;

use metallic_rc_server::{room, router, web, App};

struct Args {
    bind: String,
    public_url: Option<String>,
    data_dir: PathBuf,
    room_bind: Option<String>,
    open_rooms: bool,
    demo: bool,
}

fn args() -> Args {
    let mut a = Args { bind: "127.0.0.1:4200".into(), public_url: None, data_dir: PathBuf::from("data"), room_bind: None, open_rooms: false, demo: false };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| die(&format!("{k} needs a value")));
        match k.as_str() {
            "--bind" => a.bind = v(),
            "--public-url" => a.public_url = Some(v()),
            "--data-dir" => a.data_dir = PathBuf::from(v()),
            "--room-bind" => a.room_bind = Some(v()),
            "--open-rooms" => a.open_rooms = true,
            "--demo" => a.demo = true,
            "-h" | "--help" => {
                println!("metallic-rc-server [--bind ADDR:PORT] [--public-url URL] [--data-dir DIR] [--room-bind ADDR:PORT] [--open-rooms] [--demo]");
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
    let check = app.clone();
    let auth: room::Auth = std::sync::Arc::new(move |token: &str| metallic_rc_server::auth::verify(&check, token, "access").map(|a| (a.account_id, a.display_name)));
    let (room_addr, live) = room::start(&room_bind, (!a.open_rooms).then_some(auth)).unwrap_or_else(|e| die(&format!("room bind {room_bind}: {e}")));
    println!("room server on udp {room_addr}");
    if a.demo {
        // Seed the open-room list so the website's live sections have something to show without real players online.
        // Real joins overwrite this list, so it never masks actual activity.
        use metallic_rc_server::room::LiveRoom;
        *live.lock().unwrap() = vec![
            LiveRoom { room: "reccenter".into(), players: 12 },
            LiveRoom { room: "dorm".into(), players: 3 },
            LiveRoom { room: "driveintheater".into(), players: 7 },
            LiveRoom { room: "Neon Arena".into(), players: 5 },
            LiveRoom { room: "Spiral Coaster".into(), players: 2 },
        ];
        println!("demo: seeded {} open rooms", live.lock().unwrap().len());
    }
    let listener = tokio::net::TcpListener::bind(&a.bind).await.unwrap_or_else(|e| die(&format!("bind {}: {e}", a.bind)));
    println!("metallic-rc-server on {} (name server: {}/?buildId=...)", a.bind, app.public_url);
    println!("website: {}/www/", app.public_url);
    let svc = router(app.clone()).merge(room::routes(app.clone(), live.clone())).merge(web::routes(app, live));
    axum::serve(listener, svc).await.unwrap_or_else(|e| die(&format!("serve: {e}")));
}
