//! Metallic-RC backend: one HTTP server that answers as every RecNet service the 2026 client asks the name server for.
//!
//! The name server (`GET /` or `/nameserver`) hands the client one URL per service, all on this server:
//! `<public-url>/<service in lower case>` (auth, api, accounts, rooms, matchmaking, notifications, ...).
//! Every request is logged; requests nothing answers are also appended to `<data-dir>/unknown.jsonl`, which is the
//! list of what to build next.
pub mod auth;
pub mod creations;
pub mod discovery;
pub mod game;
pub mod games;
pub mod hub;
pub mod inventory;
pub mod party;
pub mod profile;
pub mod room;
pub mod social;
pub mod store;
pub mod web;

use std::{path::{Path, PathBuf}, sync::Arc, time::Instant};

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};

pub struct App {
    pub public_url: String,
    pub data_dir: PathBuf,
    pub store: store::Store,
    pub signing_key: Vec<u8>,
    /// presence, open hubs and parties (not saved)
    pub live: social::Live,
}
pub type Shared = Arc<App>;

impl App {
    /// The server state for a data folder: accounts and the signing key are loaded from it (made on first use).
    pub fn open(data_dir: &Path, public_url: &str) -> std::io::Result<Shared> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Arc::new(App {
            store: store::Store::open(data_dir)?,
            signing_key: store::signing_key(data_dir)?,
            public_url: public_url.trim_end_matches('/').to_string(),
            data_dir: data_dir.to_path_buf(),
            live: social::Live::default(),
        }))
    }
}

pub fn router(app: Shared) -> Router {
    Router::new()
        .merge(discovery::routes())
        .nest("/auth", auth::routes())
        .merge(game::routes())
        .merge(creations::routes())
        .merge(social::routes().merge(party::routes()).merge(profile::routes()).merge(inventory::routes()))
        .nest("/notifications", hub::routes())
        .fallback(unknown)
        .layer(middleware::from_fn(log))
        .with_state(app)
}

async fn log(req: Request, next: Next) -> Response {
    let (method, uri, t) = (req.method().clone(), req.uri().clone(), Instant::now());
    let res = next.run(req).await;
    println!("{} {} {} {}ms", res.status().as_u16(), method, uri, t.elapsed().as_millis());
    res
}

/// Anything no route answers: 404, and one line in unknown.jsonl with the method, path, query and the start of the body.
async fn unknown(State(app): State<Shared>, req: Request<Body>) -> Response {
    let (parts, body) = req.into_parts();
    let body = to_bytes(body, 1 << 20).await.unwrap_or_default();
    let line = serde_json::json!({
        "at": store::now(),
        "method": parts.method.as_str(),
        "path": parts.uri.path(),
        "query": parts.uri.query(),
        "contentType": parts.headers.get("content-type").and_then(|v| v.to_str().ok()),
        "body": String::from_utf8_lossy(&body[..body.len().min(2048)]),
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(app.data_dir.join("unknown.jsonl")) {
        let _ = writeln!(f, "{line}");
    }
    (StatusCode::NOT_FOUND, "not served by metallic-rc-server").into_response()
}
