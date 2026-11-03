//! `POST <Auth>/connect/token` (form-encoded, OAuth-style) and bearer tokens for every other service.
//!
//! The response object uses build 23191908's field names and order (LoginResponseContract.cs):
//! Error, error_description, access_token, refresh_token, Key, code_hint, login_context, hcc.
//! Access tokens are HS256 JWTs signed with the server's key; `sub` is the account id.
//!
//! Who you are: a Steam login sends its platform id, so `platform_id` (or `steam_id`) picks the account; otherwise a
//! `device_id`, otherwise `username`. This server cannot check a Steam ticket (that needs Valve's publisher key for the
//! app), so it trusts the id it is sent: fine for a private test server, not for a public one.
use std::collections::HashMap;

use axum::{
    extract::{Form, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use crate::{store::Account, Shared};

const TOKEN_SECONDS: u64 = 60 * 60 * 24;

pub fn routes() -> Router<Shared> {
    Router::new().route("/connect/token", post(token))
}

fn unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn sign(key: &[u8], data: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac takes any key length");
    mac.update(data.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}

pub fn issue(app: &crate::App, a: &Account, kind: &str) -> String {
    let now = unix();
    let head = B64.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims = json!({
        "iss": crate::discovery::service_url(app, "Auth"),
        "sub": a.account_id.to_string(),
        "name": a.username,
        "role": "gameClient",
        "typ": kind,
        "iat": now,
        "nbf": now,
        "exp": now + if kind == "refresh" { TOKEN_SECONDS * 30 } else { TOKEN_SECONDS },
    });
    let body = B64.encode(claims.to_string());
    let sig = sign(&app.signing_key, &format!("{head}.{body}"));
    format!("{head}.{body}.{sig}")
}

/// The account a token belongs to, if the signature is ours, it is not expired and it is of the wanted kind.
pub fn verify(app: &crate::App, token: &str, kind: &str) -> Option<Account> {
    let mut p = token.split('.');
    let (h, b, s) = (p.next()?, p.next()?, p.next()?);
    if p.next().is_some() || sign(&app.signing_key, &format!("{h}.{b}")) != s {
        return None;
    }
    let c: Value = serde_json::from_slice(&B64.decode(b).ok()?).ok()?;
    if c["typ"] != kind || c["exp"].as_u64()? < unix() {
        return None;
    }
    app.store.get(c["sub"].as_str()?.parse().ok()?)
}

/// The account behind `Authorization: Bearer <token>`.
pub fn caller(app: &crate::App, headers: &HeaderMap) -> Option<Account> {
    let h = headers.get("authorization")?.to_str().ok()?;
    let t = h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer "))?;
    verify(app, t.trim(), "access")
}

fn answer(status: StatusCode, error: Option<&str>, desc: Option<&str>, access: Option<String>, refresh: Option<String>) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({
            "Error": error, "error_description": desc, "access_token": access, "refresh_token": refresh,
            "Key": null, "code_hint": null, "login_context": null, "hcc": null,
        })),
    )
}

async fn token(State(app): State<Shared>, Form(f): Form<HashMap<String, String>>) -> (StatusCode, Json<Value>) {
    let field = |k: &str| f.get(k).map(|s| s.trim()).filter(|s| !s.is_empty());
    let grant = field("grant_type").unwrap_or("");
    println!("  connect/token grant_type={grant} fields={:?}", { let mut k: Vec<_> = f.keys().collect(); k.sort(); k });

    let account = if grant == "refresh_token" {
        match field("refresh_token").and_then(|t| verify(&app, t, "refresh")) {
            Some(a) => a,
            None => return answer(StatusCode::BAD_REQUEST, Some("invalid_grant"), Some("refresh token is not valid"), None, None),
        }
    } else if let Some(id) = field("platform_id").or_else(|| field("steam_id")) {
        app.store.login(&format!("steam:{id}"), field("username").or_else(|| field("display_name")).unwrap_or("Player"))
    } else if let Some(dev) = field("device_id") {
        app.store.login(&format!("device:{dev}"), field("username").unwrap_or("Player"))
    } else if let Some(name) = field("username") {
        app.store.by_name(name).unwrap_or_else(|| app.store.login(&format!("name:{name}"), name))
    } else {
        return answer(StatusCode::BAD_REQUEST, Some("invalid_request"), Some("no platform_id, device_id or username"), None, None);
    };
    println!("  -> account {} ({})", account.account_id, account.username);
    answer(StatusCode::OK, None, None, Some(issue(&app, &account, "access")), Some(issue(&app, &account, "refresh")))
}
