//! Storage and the social backend (profiles, friends, blocking, notifications, parties, inventory) against a real
//! server on a free local port. "Restart" = a second server opened on the same data folder after the first is gone.
use std::{net::SocketAddr, path::PathBuf, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("mrc-social-{}-{n}", std::process::id()))
}

/// A server on a data folder.
struct Server {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

async fn start(dir: &std::path::Path) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = metallic_rc_server::App::open(dir, &format!("http://{addr}")).unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, metallic_rc_server::router(app)).await.unwrap() });
    Server { addr, task }
}

/// Stops a server and waits until its task (and so its App and store) is gone.
async fn stop(s: Server) {
    s.task.abort();
    let _ = s.task.await;
}

/// A bare HTTP/1.1 request; `body` is sent as JSON when it starts with `{`, else as a form. Returns (status, body).
async fn http(addr: SocketAddr, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = body.unwrap_or("");
    let ct = if body.starts_with('{') { "application/json" } else { "application/x-www-form-urlencoded" };
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{auth}Content-Type: {ct}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let status = text[9..12].parse().unwrap();
    let (head, mut body) = text.split_once("\r\n\r\n").map(|(h, b)| (h.to_string(), b.to_string())).unwrap();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        let mut joined = String::new();
        let mut rest = body.as_str();
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            joined.push_str(&tail[..n]);
            rest = &tail[n + 2..];
        }
        body = joined;
    }
    (status, body)
}

fn js(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"))
}

/// Logs in by name; returns (access token, account id).
async fn login(addr: SocketAddr, name: &str) -> (String, i64) {
    let (st, body) = http(addr, "POST", "/auth/connect/token", None, Some(&format!("grant_type=steam&username={name}"))).await;
    assert_eq!(st, 200, "{body}");
    let t = js(&body)["access_token"].as_str().unwrap().to_string();
    let (_, me) = http(addr, "GET", "/accounts/account/me", Some(&t), None).await;
    (t, js(&me)["accountId"].as_i64().unwrap())
}

async fn get(addr: SocketAddr, path: &str, token: &str) -> Value {
    let (st, body) = http(addr, "GET", path, Some(token), None).await;
    assert_eq!(st, 200, "GET {path}: {body}");
    js(&body)
}

async fn post(addr: SocketAddr, path: &str, token: &str, body: Option<&str>) -> (u16, Value) {
    let (st, b) = http(addr, "POST", path, Some(token), body).await;
    (st, if b.trim().is_empty() { Value::Null } else { serde_json::from_str(&b).unwrap_or(Value::String(b)) })
}

// ---- 1. storage ----

#[tokio::test]
async fn store_survives_restart_and_concurrent_logins() {
    let dir = temp_dir();
    let s = start(&dir).await;
    let addr = s.addr;
    let logins: Vec<_> = (0..40).map(|i| tokio::spawn(async move { login(addr, &format!("P{i}")).await.1 })).collect();
    let mut ids = vec![];
    for l in logins {
        ids.push(l.await.unwrap());
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 40, "40 parallel logins made 40 accounts");
    stop(s).await;

    let file: Value = js(&std::fs::read_to_string(dir.join("accounts.json")).unwrap());
    assert_eq!(file["version"], 2);
    assert_eq!(file["accounts"].as_array().unwrap().len(), 40);
    let s = start(&dir).await;
    let (_, again) = login(s.addr, "P7").await;
    let p7_first = file["accounts"].as_array().unwrap().iter().find(|a| a["username"] == "P7").unwrap()["accountId"].as_i64().unwrap();
    assert_eq!(again, p7_first, "same name, same account after the restart");
}

#[tokio::test]
async fn store_migrates_the_first_file_and_refuses_a_newer_one() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // the file as the first server wrote it: no version, accounts only
    std::fs::write(dir.join("accounts.json"), r#"{"next_id":2,"accounts":[{"accountId":1,"username":"Old","displayName":"Old","login":"name:Old","createdAt":"2026-11-01T00:00:00Z"}]}"#).unwrap();
    let s = start(&dir).await;
    let (t, id) = login(s.addr, "Old").await;
    assert_eq!(id, 1);
    assert_eq!(get(s.addr, "/api/relationships/v2/get", &t).await, json!([]));
    stop(s).await;
    let file = js(&std::fs::read_to_string(dir.join("accounts.json")).unwrap());
    assert_eq!(file["version"], 2);
    assert_eq!(file["accounts"][0]["username"], "Old");

    std::fs::write(dir.join("accounts.json"), r#"{"version":99,"next_id":1,"accounts":[]}"#).unwrap();
    let err = metallic_rc_server::App::open(&dir, "http://x").err().expect("a newer schema is refused");
    assert!(err.to_string().contains("schema version 99"), "{err}");
}
