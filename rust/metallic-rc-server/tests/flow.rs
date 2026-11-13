//! The client's path from the name server to a dorm room, against a real server on a free local port.
use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

async fn start() -> (SocketAddr, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mrc-test-{}-{}", std::process::id(), rand_suffix()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = metallic_rc_server::App::open(&dir, &format!("http://{addr}")).unwrap();
    tokio::spawn(async move { axum::serve(listener, metallic_rc_server::router(app)).await.unwrap() });
    (addr, dir)
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

/// A bare HTTP/1.1 request; returns (status, body).
async fn http(addr: SocketAddr, method: &str, path: &str, token: Option<&str>, form: Option<&str>) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = form.unwrap_or("");
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{auth}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let status = text[9..12].parse().unwrap();
    let (head, mut body) = text.split_once("\r\n\r\n").map(|(h, b)| (h.to_string(), b.to_string())).unwrap();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        // one or more chunks: keep only the data lines
        let mut joined = String::new();
        let mut rest = body.as_str();
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 { break; }
            joined.push_str(&tail[..n]);
            rest = &tail[n + 2..];
        }
        body = joined;
    }
    (status, body)
}

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"))
}

#[tokio::test]
async fn nameserver_lists_every_service_on_this_server() {
    let (addr, _) = start().await;
    let (st, body) = http(addr, "GET", "/?buildId=23191908", None, None).await;
    assert_eq!(st, 200);
    let v = json(&body);
    let m = v.as_object().unwrap();
    assert_eq!(m.len(), 32);
    assert_eq!(m.keys().next().unwrap(), "RecNetStatus", "field order is kept");
    assert!(v["RecNetStatus"].is_null());
    assert_eq!(v["Auth"], format!("http://{addr}/auth"));
    assert_eq!(v["StringsCDN"], format!("http://{addr}/stringscdn"));
}

#[tokio::test]
async fn login_then_me_then_dorm() {
    let (addr, _) = start().await;
    let (st, body) = http(addr, "POST", "/auth/connect/token", None, Some("grant_type=steam&platform_id=765&username=Alex")).await;
    assert_eq!(st, 200, "{body}");
    let t = json(&body);
    let names: Vec<_> = t.as_object().unwrap().keys().cloned().collect();
    assert_eq!(names, ["Error", "error_description", "access_token", "refresh_token", "Key", "code_hint", "login_context", "hcc"]);
    let access = t["access_token"].as_str().unwrap().to_string();
    let refresh = t["refresh_token"].as_str().unwrap().to_string();

    let (st, me) = http(addr, "GET", "/accounts/account/me", Some(&access), None).await;
    assert_eq!(st, 200);
    assert_eq!(json(&me)["username"], "Alex");
    assert_eq!(http(addr, "GET", "/accounts/account/me", None, None).await.0, 401);
    assert_eq!(http(addr, "GET", "/accounts/account/me", Some(&format!("{access}x")), None).await.0, 401);
    assert_eq!(http(addr, "GET", "/accounts/account/me", Some(&refresh), None).await.0, 401, "a refresh token is not an access token");

    // the same Steam id logs into the same account; a refresh token gives a new access token for it
    let (_, again) = http(addr, "POST", "/auth/connect/token", None, Some("grant_type=steam&platform_id=765")).await;
    let (_, me2) = http(addr, "GET", "/accounts/account/me", Some(json(&again)["access_token"].as_str().unwrap()), None).await;
    assert_eq!(json(&me2)["accountId"], 1);
    let (st, r) = http(addr, "POST", "/auth/connect/token", None, Some(&format!("grant_type=refresh_token&refresh_token={refresh}"))).await;
    assert_eq!(st, 200);
    assert!(json(&r)["access_token"].is_string());
    let (st, _) = http(addr, "POST", "/auth/connect/token", None, Some("grant_type=refresh_token&refresh_token=nope")).await;
    assert_eq!(st, 400);

    assert_eq!(http(addr, "POST", "/matchmaking/player/login", Some(&access), None).await.0, 200);
    let (st, goto) = http(addr, "POST", "/matchmaking/goto/room/dormroom", Some(&access), None).await;
    assert_eq!(st, 200);
    let g = json(&goto);
    assert_eq!(g["errorCode"], 0);
    assert_eq!(g["roomInstance"]["roomId"], 1);
    assert_eq!(g["roomInstance"]["isPrivate"], true);
    // goto tells the client which map to load (so joining a room loads its scene in multiplayer)
    assert_eq!(g["roomInstance"]["UnitySceneId"], "dorm");
    assert_eq!(g["roomInstance"]["subRooms"][0]["UnitySceneId"], "dorm");
}

#[tokio::test]
async fn the_room_list_has_every_room_and_pages() {
    let (addr, _) = start().await;
    let (st, body) = http(addr, "GET", "/rooms/rooms/hot", None, None).await;
    assert_eq!(st, 200);
    let v = json(&body);
    let names: Vec<&str> = v["Results"].as_array().unwrap().iter().map(|r| r["Name"].as_str().unwrap()).collect();
    // every built-in 2026 room is listed (the browser can reach them all), the Dorm first, then the Rec Center
    assert_eq!(names.len(), 19);
    assert_eq!(&names[..2], &["DormRoom", "RecCenter"]);
    assert_eq!(names.last(), Some(&"OrientationPracticeGym"));
    assert_eq!(v["TotalResults"], 19);
    assert_eq!(v["Results"][1]["RoomId"], 2);
    // each room carries the scene the client loads (UnitySceneId in its SubRooms)
    assert_eq!(v["Results"][1]["SubRooms"][0]["UnitySceneId"], "reccenter");
    assert!(v["Results"][0]["Description"].is_string());
    let (_, page) = http(addr, "GET", "/rooms/rooms/hot?skip=1&take=5", None, None).await;
    let page = json(&page);
    assert_eq!((page["Results"].as_array().unwrap().len(), page["Results"][0]["Name"].as_str(), page["TotalResults"].as_i64()), (5, Some("RecCenter"), Some(19)));
}

#[tokio::test]
async fn unknown_requests_are_written_down() {
    let (addr, dir) = start().await;
    assert_eq!(http(addr, "POST", "/api/newthing/v9?x=1", None, Some("a=1")).await.0, 404);
    let log = std::fs::read_to_string(dir.join("unknown.jsonl")).unwrap();
    let line = json(log.lines().next().unwrap());
    assert_eq!(line["path"], "/api/newthing/v9");
    assert_eq!(line["query"], "x=1");
    assert_eq!(line["body"], "a=1");
}

#[tokio::test]
async fn hub_handshake_and_invocation() {
    let (addr, _) = start().await;
    let (st, neg) = http(addr, "POST", "/notifications/hub/v1/negotiate?negotiateVersion=1", None, None).await;
    assert_eq!(st, 200);
    let token = json(&neg)["connectionToken"].as_str().unwrap().to_string();
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/notifications/hub/v1?id={token}")).await.unwrap();
    ws.send(Message::text("{\"protocol\":\"json\",\"version\":1}\u{1e}")).await.unwrap();
    assert_eq!(ws.next().await.unwrap().unwrap().into_text().unwrap().as_str(), "{}\u{1e}");
    ws.send(Message::text("{\"type\":1,\"invocationId\":\"7\",\"target\":\"SubscribeToPlayers\",\"arguments\":[[1]]}\u{1e}")).await.unwrap();
    let reply = ws.next().await.unwrap().unwrap().into_text().unwrap();
    let v = json(reply.trim_end_matches('\u{1e}'));
    assert_eq!(v["type"], 3);
    assert_eq!(v["invocationId"], "7");
}
