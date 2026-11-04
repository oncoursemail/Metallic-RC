//! The notifications hub: ASP.NET SignalR (JSON protocol) at `<Notifications>/hub/v1`.
//!
//! negotiate (POST .../negotiate) -> WebSocket -> the client sends `{"protocol":"json","version":1}` + 0x1E, we answer
//! `{}` + 0x1E, then both sides ping (`{"type":6}`) every 15 s. Invocations are logged so the method names the client
//! calls show up.
//!
//! Who is connected: a negotiate with `Authorization: Bearer <access token>` ties its connection token to the account,
//! and the WebSocket (`?id=<token>`, or `?access_token=<token>` as SignalR clients send it) picks that up. Connections
//! with an account receive the social pushes (`social::push`: the `Notification` target) and count as online.
//! Unauthenticated connections still work, they just get no pushes.
use std::{collections::HashMap, time::Duration};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::HeaderMap,
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

async fn negotiate(State(app): State<Shared>, h: HeaderMap) -> Json<Value> {
    let id: String = rand::thread_rng().sample_iter(&Alphanumeric).take(22).map(char::from).collect();
    if let Some(a) = crate::auth::caller(&app, &h) {
        let mut live = app.live.lock();
        if live.negotiated.len() > 10_000 {
            live.negotiated.clear();
        }
        live.negotiated.insert(id.clone(), a.account_id);
    }
    Json(json!({
        "negotiateVersion": 1, "connectionId": id, "connectionToken": id,
        "availableTransports": [{ "transport": "WebSockets", "transferFormats": ["Text", "Binary"] }],
    }))
}

async fn connect(State(app): State<Shared>, Query(q): Query<HashMap<String, String>>, ws: WebSocketUpgrade) -> Response {
    let by_token = q.get("id").and_then(|t| app.live.lock().negotiated.remove(t));
    let account = by_token.or_else(|| crate::auth::verify(&app, q.get("access_token")?, "access").map(|a| a.account_id));
    ws.on_upgrade(move |ws| async move {
        let Some(id) = account else { return session(ws, None).await };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let key = {
            let mut live = app.live.lock();
            live.next_hub += 1;
            let key = live.next_hub;
            live.hubs.entry(id).or_default().push((key, tx));
            key
        };
        crate::social::announce(&app, id);
        session(ws, Some(rx)).await;
        if let Some(h) = app.live.lock().hubs.get_mut(&id) {
            h.retain(|(k, _)| *k != key);
        }
        crate::social::announce(&app, id);
    })
}

async fn session(mut ws: WebSocket, mut pushes: Option<tokio::sync::mpsc::UnboundedReceiver<String>>) {
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    ping.tick().await;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if ws.send(Message::Text(format!("{{\"type\":6}}{RS}").into())).await.is_err() { break; }
            }
            Some(frame) = async { match pushes.as_mut() { Some(r) => r.recv().await, None => std::future::pending().await } } => {
                if ws.send(Message::Text(frame.into())).await.is_err() { break; }
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
