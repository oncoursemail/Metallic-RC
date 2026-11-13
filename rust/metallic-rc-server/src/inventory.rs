//! Inventory: items an account owns, saved per account. Only the server grants items (no currency, no purchases).
//! Every account gets the starter kit the first time its inventory is read or its look checked.
//!
//! Ours: `GET api/inventory/v1/me` -> `[{itemId, category, grantedAt, source}]`. Item ids are `<category>:<name>`;
//! `accessory:*` and `outfit:*` are the robot avatar's accessories and outfit pieces (`none` needs no item).
use axum::{extract::State, http::HeaderMap, response::Response, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::{
    social::who,
    store::{now, Data, Item},
    Shared,
};

/// What every account starts with: the robot's accessories and outfit pieces.
pub const STARTER: [&str; 11] = ["accessory:antenna", "accessory:headphones", "accessory:halo", "accessory:cap", "accessory:beanie",
    "accessory:ears", "accessory:sprout", "accessory:goggles", "outfit:scarf", "outfit:backpack", "outfit:bowtie"];

pub fn routes() -> Router<Shared> {
    Router::new().route("/api/inventory/v1/me", get(mine))
}

/// Gives `owner` the item unless it already has it; true if it was new.
pub fn grant(d: &mut Data, owner: i64, item_id: &str, source: &str) -> bool {
    if d.items.iter().any(|i| i.owner == owner && i.item_id == item_id) {
        return false;
    }
    let category = item_id.split_once(':').map_or("misc", |(c, _)| c).to_string();
    d.items.push(Item { owner, item_id: item_id.into(), category, granted_at: now(), source: source.into() });
    true
}

fn starter(d: &mut Data, owner: i64) {
    for i in STARTER {
        grant(d, owner, i, "starter");
    }
}

pub fn owns(d: &mut Data, owner: i64, item_id: &str) -> bool {
    starter(d, owner);
    d.items.iter().any(|i| i.owner == owner && i.item_id == item_id)
}

async fn mine(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let id = who(&app, &h)?.account_id;
    let fresh = app.store.read(|d| STARTER.iter().any(|s| !d.items.iter().any(|i| i.owner == id && i.item_id == *s)));
    if fresh {
        app.store.write(|d| starter(d, id));
    }
    Ok(Json(app.store.read(|d| {
        Value::Array(
            d.items
                .iter()
                .filter(|i| i.owner == id)
                .map(|i| json!({ "itemId": i.item_id, "category": i.category, "grantedAt": i.granted_at, "source": i.source }))
                .collect(),
        )
    })))
}
