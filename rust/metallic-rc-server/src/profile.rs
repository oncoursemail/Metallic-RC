//! Profiles: display name, bio, the robot avatar's look, created date, level and XP.
//!
//! - Ours: `GET api/profile/v1/me`, `GET api/profile/v1/{id}`, `POST api/profile/v1/me` with a JSON object of any of
//!   `displayName` (1-32 characters), `bio` (up to 512), `look` (`{"skin","shirt","accent"}` as `#rrggbb` and an
//!   `accessory` the player owns, see `inventory`). Answer and push (`SubscriptionUpdateProfile` to friends):
//!   `{accountId, username, displayName, bio, look, createdAt, level, xp}`; `look` is null until one is saved.
//! - The game's: `GET accounts/account/{id}/bio` -> `{accountId, bio}` (BioDTO; path V), `PUT|POST
//!   accounts/account/me/bio` (`bio`), `PUT|POST accounts/account/me/displayName` (`displayName`) (verbs and the display
//!   name path INFERRED), `api/players/v2/progression/bulk?id=` -> `[{PlayerId, AccountId, Level, XP}]` (the 2026
//!   `Progression` class has AccountId; older clients read PlayerId, so both go out), `api/avatar/v2` + `/set` (what
//!   the game posted, kept as is).
use axum::{
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::{
    inventory,
    social::{fail, push, who, Params},
    store::{Data, RelKind},
    App, Shared,
};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/profile/v1/me", get(me).post(update))
        .route("/api/profile/v1/{id}", get(other))
        .route("/accounts/account/{id}/bio", get(bio))
        .route("/accounts/account/me/bio", post(set_bio).put(set_bio))
        .route("/accounts/account/me/displayName", post(set_name).put(set_name))
        .route("/api/players/v2/progression/bulk", get(progression))
        .route("/api/avatar/v2", get(game_avatar))
        .route("/api/avatar/v2/set", post(set_game_avatar))
}

pub fn profile_json(d: &Data, id: i64) -> Option<Value> {
    let a = d.accounts.iter().find(|a| a.account_id == id)?;
    let p = d.profiles.get(&id).cloned().unwrap_or_default();
    Some(json!({
        "accountId": id, "username": a.username, "displayName": a.display_name, "bio": p.bio, "look": p.look,
        "createdAt": a.created_at, "level": p.level.max(1), "xp": p.xp,
    }))
}

async fn me(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let id = who(&app, &h)?.account_id;
    Ok(Json(app.store.read(|d| profile_json(d, id)).unwrap_or(Value::Null)))
}

async fn other(State(app): State<Shared>, Path(id): Path<i64>) -> Result<Json<Value>, Response> {
    app.store.read(|d| profile_json(d, id)).map(Json).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such player"))
}

fn check_name(s: &str) -> Result<String, &'static str> {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > 32 || s.chars().any(char::is_control) {
        return Err("displayName must be 1 to 32 characters");
    }
    Ok(s.to_string())
}

fn check_bio(s: &str) -> Result<String, &'static str> {
    if s.chars().count() > 512 {
        return Err("bio is longer than 512 characters");
    }
    Ok(s.to_string())
}

fn check_look(d: &mut Data, id: i64, v: &Value) -> Result<Value, &'static str> {
    let hex = |k: &str| {
        let s = v[k].as_str().unwrap_or("");
        (s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())).then(|| s.to_ascii_lowercase())
    };
    let (Some(skin), Some(shirt), Some(accent)) = (hex("skin"), hex("shirt"), hex("accent")) else {
        return Err("look needs skin, shirt and accent as #rrggbb");
    };
    let accessory = v["accessory"].as_str().unwrap_or("none").to_ascii_lowercase();
    if accessory != "none" && !inventory::owns(d, id, &format!("accessory:{accessory}")) {
        return Err("you do not own that accessory");
    }
    Ok(json!({ "skin": skin, "shirt": shirt, "accent": accent, "accessory": accessory }))
}

/// Applies the fields present; all are checked before anything changes. Returns the new profile or why not.
fn apply(app: &App, id: i64, name: Option<&Value>, bio: Option<&Value>, look: Option<&Value>) -> Result<Value, Response> {
    let bad = |e: &str| fail(StatusCode::BAD_REQUEST, e);
    let name = name.map(|v| check_name(v.as_str().unwrap_or(""))).transpose().map_err(bad)?;
    let bio = bio.map(|v| check_bio(v.as_str().unwrap_or(""))).transpose().map_err(bad)?;
    let profile = app.store.write(|d| {
        let look = look.map(|v| check_look(d, id, v)).transpose()?;
        if let Some(n) = name {
            if let Some(a) = d.accounts.iter_mut().find(|a| a.account_id == id) {
                a.display_name = n;
            }
        }
        let p = d.profiles.entry(id).or_default();
        if let Some(b) = bio {
            p.bio = b;
        }
        if look.is_some() {
            p.look = look;
        }
        Ok(profile_json(d, id).unwrap_or(Value::Null))
    });
    let profile = profile.map_err(bad)?;
    let friends: Vec<i64> = app.store.read(|d| d.relations.iter().filter(|r| r.from == id && r.kind == RelKind::Friend).map(|r| r.to).collect());
    for f in friends {
        push(app, f, "SubscriptionUpdateProfile", profile.clone());
    }
    Ok(profile)
}

async fn update(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Result<Json<Value>, Response> {
    let id = who(&app, &h)?.account_id;
    let v: Value = serde_json::from_slice(&body).map_err(|_| fail(StatusCode::BAD_REQUEST, "send a JSON object"))?;
    let field = |k: &str| v.get(k).filter(|x| !x.is_null());
    Ok(Json(apply(&app, id, field("displayName"), field("bio"), field("look"))?))
}

async fn bio(State(app): State<Shared>, Path(id): Path<i64>) -> Result<Json<Value>, Response> {
    let bio = app.store.read(|d| d.accounts.iter().any(|a| a.account_id == id).then(|| d.profiles.get(&id).map(|p| p.bio.clone()).unwrap_or_default()));
    bio.map(|b| Json(json!({ "accountId": id, "bio": b }))).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such player"))
}

async fn set_bio(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes) -> Result<Json<Value>, Response> {
    let id = who(&app, &h)?.account_id;
    let p = Params::new(&q, &body);
    let bio = p.get(&["bio"]).cloned().ok_or_else(|| fail(StatusCode::BAD_REQUEST, "no bio"))?;
    apply(&app, id, None, Some(&bio), None)?;
    Ok(Json(json!({ "accountId": id, "bio": bio })))
}

async fn set_name(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes) -> Result<Json<Value>, Response> {
    let id = who(&app, &h)?.account_id;
    let p = Params::new(&q, &body);
    let name = p.get(&["displayName", "name"]).cloned().ok_or_else(|| fail(StatusCode::BAD_REQUEST, "no displayName"))?;
    Ok(Json(apply(&app, id, Some(&name), None, None)?))
}

async fn progression(State(app): State<Shared>, RawQuery(q): RawQuery) -> Json<Value> {
    let ids: Vec<i64> = serde_urlencoded::from_str::<Vec<(String, String)>>(q.as_deref().unwrap_or(""))
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("id"))
        .filter_map(|(_, v)| v.parse().ok())
        .collect();
    Json(app.store.read(|d| {
        Value::Array(
            ids.iter()
                .map(|&id| {
                    let p = d.profiles.get(&id).cloned().unwrap_or_default();
                    json!({ "PlayerId": id, "AccountId": id, "Level": p.level.max(1), "XP": p.xp })
                })
                .collect(),
        )
    }))
}

async fn game_avatar(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let id = who(&app, &h)?.account_id;
    let saved = app.store.read(|d| d.profiles.get(&id).and_then(|p| p.game_avatar.clone()));
    Ok(Json(saved.unwrap_or_else(|| json!({ "OutfitSelections": "", "FaceFeatures": "", "SkinColor": "", "HairColor": "" }))))
}

async fn set_game_avatar(State(app): State<Shared>, h: HeaderMap, body: Bytes) -> Result<StatusCode, Response> {
    let id = who(&app, &h)?.account_id;
    let v: Value = serde_json::from_slice(&body).map_err(|_| fail(StatusCode::BAD_REQUEST, "send a JSON object"))?;
    app.store.write(|d| d.profiles.entry(id).or_default().game_avatar = Some(v));
    Ok(StatusCode::OK)
}
