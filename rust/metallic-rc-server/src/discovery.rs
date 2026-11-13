//! The name server: the first request the client makes (`ns.rec.net/?buildId=<steam build>`).
//! The response is one object with 32 string-or-null fields: `RecNetStatus` (null or empty = OK; any text is shown as
//! an outage message) and 31 service base URLs. Field names and order are those of build 23191908's formatter
//! (see src/Metallic.RC.Compatibility/DiscoveryContract.cs).
use axum::{extract::State, routing::get, Json, Router};
use serde_json::{Map, Value};

use crate::Shared;

pub const SERVICES: [&str; 31] = [
    "AI", "Auth", "API", "WWW", "Notifications", "Images", "CDN", "Commerce", "Matchmaking", "Storage", "Chat",
    "Leaderboard", "Accounts", "Link", "Lists", "RoomComments", "Clubs", "Rooms", "PlatformNotifications",
    "Moderation", "DataCollection", "BugReporting", "Discovery", "PlayerSettings", "Studio", "GameLogs", "Strings",
    "Econ", "StringsCDN", "Data", "Cards",
];

pub fn routes() -> Router<Shared> {
    Router::new().route("/", get(nameserver)).route("/nameserver", get(nameserver))
}

pub fn service_url(app: &crate::App, service: &str) -> String {
    format!("{}/{}", app.public_url, service.to_ascii_lowercase())
}

async fn nameserver(State(app): State<Shared>) -> Json<Value> {
    let mut m = Map::new();
    m.insert("RecNetStatus".into(), Value::Null);
    for s in SERVICES {
        m.insert(s.into(), Value::String(service_url(&app, s)));
    }
    Json(Value::Object(m))
}
