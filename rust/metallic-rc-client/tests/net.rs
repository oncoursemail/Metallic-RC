//! The client's start-up against a real metallic-rc-server on a free local port.
use metallic_rc_client::net::{connect, Login};

fn start_server() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("mrc-client-test-{}-{nanos}", std::process::id()));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let app = metallic_rc_server::App::open(&dir, &url).unwrap();
            tx.send(url).unwrap();
            axum::serve(listener, metallic_rc_server::router(app)).await.unwrap();
        })
    });
    rx.recv().unwrap()
}

#[test]
fn starts_up_into_a_private_dorm_and_opens_the_hub() {
    let url = start_server();
    let mut s = connect(&url, &Login::Name("Alex".into())).unwrap();
    assert_eq!(s.account.username, "Alex");
    assert_eq!(s.room.name, "DormRoom");
    assert!(s.room.private);
    assert_eq!(s.room.photon_room, format!("dorm-{}", s.account.id));
    assert!(!s.motd.is_empty());

    let mut hub = s.open_hub().unwrap();
    assert_eq!(hub.invoke("SubscribeToPlayers", serde_json::json!([[s.account.id]])).unwrap(), serde_json::Value::Null);
    hub.close();

    let rc = s.goto("RecCenter").unwrap();
    assert_eq!((rc.room_id, rc.private), (2, false));
    assert!(s.goto("NoSuchRoom").is_err());
}

#[test]
fn the_same_device_gets_the_same_account() {
    let url = start_server();
    let a = connect(&url, &Login::Device("dev-1".into())).unwrap();
    let b = connect(&url, &Login::Device("dev-1".into())).unwrap();
    assert_eq!(a.account.id, b.account.id);
}
