//! The public website (the "WWW" service): metallic.rec revival homepage, served by the same backend that answers the
//! game. It is read-only and needs no login, so a browser can show it straight from the server.
//!
//! Live data comes from the running server: `players online` and the open-room list are the real room server's
//! [`crate::room::Live`] state, `accounts` is the store. Communities and the activity feed are seeded into the data dir
//! on first use (`communities.json`, `feed.json`) so they are real, editable backend state; the Discord bot pushes new
//! feed items with `POST /www/api/feed` carrying the `feed.key` secret.
use axum::{
    extract::{Path as AxPath, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{room::Live, store::now, Shared};

const INDEX: &str = include_str!("../web/index.html");
const HERO: &[u8] = include_bytes!("../web/img/hero.png");
const MULTIPLAYER: &[u8] = include_bytes!("../web/img/multiplayer.png");
const ISLAND: &[u8] = include_bytes!("../web/img/island.png");

/// Mounts the site at `/www` and its JSON API at `/www/api/*`. `live` is the room server's open-room list.
pub fn routes(app: Shared, live: Live) -> Router {
    let stats_live = live.clone();
    let rooms_live = live;
    Router::new()
        .route("/www", get(|| async { Redirect::permanent("/www/") }))
        .route("/www/", get(index))
        .route("/www/index.html", get(index))
        .route("/www/img/{name}", get(image))
        .route("/www/api/stats", get(move |s| stats(s, stats_live.clone())))
        .route("/www/api/rooms", get(move |s| rooms(s, rooms_live.clone())))
        .route("/www/api/communities", get(communities))
        .route("/www/api/feed", get(feed).post(feed_push))
        .with_state(app)
}

async fn index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], INDEX)
}

async fn image(AxPath(name): AxPath<String>) -> Response {
    let bytes: &[u8] = match name.as_str() {
        "hero.png" => HERO,
        "multiplayer.png" => MULTIPLAYER,
        "island.png" => ISLAND,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")], bytes).into_response()
}

// ---- live stats ----

/// `GET /www/api/stats`: the numbers the homepage ticks — players in rooms right now, open rooms, accounts, communities.
async fn stats(State(app): State<Shared>, live: Live) -> Json<Value> {
    let open = live.lock().unwrap().clone();
    let players_online: usize = open.iter().map(|r| r.players).sum();
    let accounts = app.store.read(|d| d.accounts.len());
    let communities = load_communities(&app).len();
    Json(json!({
        "playersOnline": players_online,
        "roomsOpen": open.len(),
        "accounts": accounts,
        "communities": communities,
    }))
}

/// `GET /www/api/rooms`: the open rooms, fullest first, each with a display title/blurb and a poster image key.
async fn rooms(State(_app): State<Shared>, live: Live) -> Json<Value> {
    let open = live.lock().unwrap().clone();
    let list: Vec<Value> = open
        .into_iter()
        .map(|r| {
            let (d, known) = room_display(&r.room);
            // Player-made rooms keep their own name as the heading; known hub rooms get their hand-written title.
            let title = if known { d.title.to_string() } else { r.room.clone() };
            json!({ "name": r.room, "title": title, "blurb": d.blurb, "image": d.image, "accent": d.accent, "players": r.players })
        })
        .collect();
    Json(Value::Array(list))
}

struct Display {
    title: &'static str,
    blurb: &'static str,
    image: &'static str,
    accent: &'static str,
}

/// Known room keys get a hand-written title and look; anything else shows under its own name (`known` = false).
fn room_display(key: &str) -> (Display, bool) {
    match key.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
        "dorm" => (Display { title: "The Dorm", blurb: "Your private starter room — decorate it and invite friends.", image: "hero", accent: "#2fd4c6" }, true),
        "reccenter" | "rec" | "thereccenter" => (Display { title: "The Rec Center", blurb: "The social hub everyone lands in. Find a game and go.", image: "hero", accent: "#2fd4c6" }, true),
        "crescendo" | "theatre" | "theater" | "driveintheater" => (Display { title: "Drive-In Theater", blurb: "Open-mic movie nights under the stars.", image: "island", accent: "#f0813c" }, true),
        _ => (Display { title: "Community Room", blurb: "A player-made room that's live right now.", image: "island", accent: "#4aa3ff" }, false),
    }
}

// ---- communities (seeded, editable backend state) ----

#[derive(Clone, Serialize, Deserialize)]
struct Community {
    name: String,
    blurb: String,
    members: u32,
    tags: Vec<String>,
    accent: String,
}

fn default_communities() -> Vec<Community> {
    let c = |name: &str, blurb: &str, members: u32, tags: &[&str], accent: &str| Community {
        name: name.into(),
        blurb: blurb.into(),
        members,
        tags: tags.iter().map(|s| s.to_string()).collect(),
        accent: accent.into(),
    };
    vec![
        c("Paintball League", "Weekly 4v4 ladder matches in the arenas. Everyone welcome, no rank gate.", 2184, &["Competitive", "Shooter"], "#f0813c"),
        c("Build & Circuits", "Maker Pen tutorials, circuit builds and room jams every weekend.", 1670, &["Creation", "Maker"], "#2fd4c6"),
        c("Quest Runners", "Co-op quest nights — speedruns, boss raids and new-map first looks.", 1442, &["Co-op", "Adventure"], "#4aa3ff"),
        c("Open Mic Lounge", "Karaoke, stand-up and chill voice hangs in the Drive-In every night.", 988, &["Social", "Events"], "#ffd23f"),
        c("New Player Welcome", "Guides, free rooms and friendly mentors for your first week.", 3120, &["Help", "Newcomers"], "#4aa3ff"),
        c("Golden Trophy Club", "Showcase room of the week and the community awards.", 742, &["Showcase"], "#f0813c"),
    ]
}

fn load_communities(app: &crate::App) -> Vec<Community> {
    load_or_seed(app, "communities.json", default_communities)
}

async fn communities(State(app): State<Shared>) -> Json<Vec<Community>> {
    let mut list = load_communities(&app);
    list.sort_by(|a, b| b.members.cmp(&a.members));
    Json(list)
}

// ---- activity feed (Discord-bot ready) ----

#[derive(Clone, Serialize, Deserialize)]
struct FeedItem {
    kind: String,
    text: String,
    room: Option<String>,
    at: String,
}

fn default_feed() -> Vec<FeedItem> {
    let f = |kind: &str, text: &str, room: Option<&str>| FeedItem {
        kind: kind.into(),
        text: text.into(),
        room: room.map(|s| s.into()),
        at: now(),
    };
    vec![
        f("room", "Nova opened a new room — Neon Arena", Some("Neon Arena")),
        f("match", "Paintball League finished a 4v4 — Orange 50, Teal 47", Some("Arena 2")),
        f("join", "Twelve players are hanging out in the Rec Center right now", Some("The Rec Center")),
        f("build", "Pixel published a Maker Pen build: Spiral Coaster", Some("Build & Circuits")),
        f("quest", "Quest Runners cleared Isle Quest with 4 players", Some("Isle Quest")),
    ]
}

async fn feed(State(app): State<Shared>) -> Json<Vec<FeedItem>> {
    Json(load_or_seed(&app, "feed.json", default_feed))
}

/// `POST /www/api/feed` with header `x-feed-key: <secret>` appends an item (the Discord bot's hook). The secret is the
/// contents of `<data-dir>/feed.key`, made on first start.
async fn feed_push(State(app): State<Shared>, headers: HeaderMap, body: Json<FeedItem>) -> Response {
    let key = feed_key(&app);
    let given = headers.get("x-feed-key").and_then(|v| v.to_str().ok()).unwrap_or("");
    if key.is_empty() || given != key {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut list = load_or_seed(&app, "feed.json", default_feed);
    let mut item = body.0;
    item.at = now();
    list.insert(0, item);
    list.truncate(50);
    let _ = std::fs::write(app.data_dir.join("feed.json"), serde_json::to_vec_pretty(&list).unwrap_or_default());
    StatusCode::CREATED.into_response()
}

fn feed_key(app: &crate::App) -> String {
    let path = app.data_dir.join("feed.key");
    if let Ok(s) = std::fs::read_to_string(&path) {
        return s.trim().to_string();
    }
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    let key = b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let _ = std::fs::write(&path, &key);
    key
}

/// Reads `<data-dir>/<file>` as JSON, or writes `default()` there and returns it.
fn load_or_seed<T: Serialize + for<'de> Deserialize<'de>>(app: &crate::App, file: &str, default: fn() -> Vec<T>) -> Vec<T> {
    let path = app.data_dir.join(file);
    if let Ok(b) = std::fs::read(&path) {
        if let Ok(v) = serde_json::from_slice::<Vec<T>>(&b) {
            return v;
        }
    }
    let v = default();
    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&v).unwrap_or_default());
    v
}
