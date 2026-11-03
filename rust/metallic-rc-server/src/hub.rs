//! The notifications hub: ASP.NET SignalR (JSON protocol) at `<Notifications>/hub/v1`.
//!
//! negotiate (POST .../negotiate) -> WebSocket -> the client sends `{"protocol":"json","version":1}` + 0x1E, we answer
//! `{}` + 0x1E, then both sides ping (`{"type":6}`) every 15 s. Nothing is pushed yet; invocations are logged so the
//! method names the client calls show up.
use std::time::Duration;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::{any, post},
    Json, Router,
};
use rand::{distributions::Alphanumeric, Rng};
use serde_json::{json, Value};

use crate::Shared;

const RS: char = '\u{1e}';

pub fn routes() -> Router<Shared> {
    Router::new().route("/hub/v1/negotiate", post(negotiate)).route("/hub/v1", any(connect))
}

async fn negotiate() -> Json<Value> {
    let id: String = rand::thread_rng().sample_iter(&Alphanumeric).take(22).map(char::from).collect();
    Json(json!({
        "negotiateVersion": 1, "connectionId": id, "connectionToken": id,
        "availableTransports": [{ "transport": "WebSockets", "transferFormats": ["Text", "Binary"] }],
    }))
}

async fn connect(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(session)
}

async fn session(mut ws: WebSocket) {
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    ping.tick().await;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if ws.send(Message::Text(format!("{{\"type\":6}}{RS}").into())).await.is_err() { break; }
            }
            m = ws.recv() => {
                let Some(Ok(m)) = m else { break };
                match m {
                    Message::Text(t) => {
                        for frame in t.split(RS).filter(|f| !f.is_empty()) {
                            let v: Value = serde_json::from_str(frame).unwrap_or(Value::Null);
                            if v.get("protocol").is_some() {
                                println!("  hub: handshake {frame}");
                                if ws.send(Message::Text(format!("{{}}{RS}").into())).await.is_err() { return; }
                            } else if v["type"] == 6 {
                            } else if v["type"] == 7 {
                                return;
                            } else {
                                println!("  hub: {frame}");
                                // answer an invocation that wants a result, so the client does not wait forever
                                if let Some(id) = v.get("invocationId").and_then(Value::as_str) {
                                    let done = json!({ "type": 3, "invocationId": id, "result": null });
                                    if ws.send(Message::Text(format!("{done}{RS}").into())).await.is_err() { return; }
                                }
                            }
                        }
                    }
                    Message::Binary(b) => println!("  hub: {} binary bytes (MessagePack protocol is not handled)", b.len()),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }
}
