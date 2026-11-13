//! The pen's side of the server's creations (`metallic-rc-server/src/creations.rs`): the room's save and who may
//! build, saving, publishing a room of one's own and choosing builders. The calls block; `attach` and `run` put them
//! on threads and the answers come back through `take`.
use std::sync::Mutex;
use std::time::Duration;

use bevy::prelude::*;
use serde_json::{json, Value};

use super::edit::ObjState;
use crate::net::Session;

/// Where the creations API is and who calls it.
#[derive(Clone, Debug)]
pub struct Link {
    /// the API service's base address (`<server>/api`)
    pub api: String,
    pub token: String,
    /// the realtime room this player is in: the save's key
    pub room: String,
    pub me: String,
}

/// A room's save as the server has it, and whether this player may build there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomInfo {
    pub room: String,
    pub can_edit: bool,
    pub owner: Option<String>,
    pub version: u64,
    pub objects: Vec<ObjState>,
}

#[derive(Debug)]
pub enum Answer {
    Info(Result<RoomInfo, String>),
    Saved(Result<u64, String>),
    Published(Result<String, String>),
    Builders(Result<Vec<i64>, String>),
}

#[derive(Default)]
struct Shared {
    link: Option<Link>,
    /// bumped by every login: answers for an older one are dropped
    gen: u64,
    answers: Vec<(u64, Answer)>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { link: None, gen: 0, answers: Vec::new() });

/// Called by the login thread once it knows the realtime room: remembers the link and fetches the room's save.
pub fn attach(s: &Session, room: &str) {
    let Some(api) = s.service("API") else { return };
    let link = Link { api: api.trim_end_matches('/').to_string(), token: s.access_token.clone(), room: room.to_string(), me: s.account.display_name.clone() };
    {
        let mut sh = SHARED.lock().unwrap();
        sh.gen += 1;
        sh.link = Some(link.clone());
        sh.answers.clear();
    }
    run(move |l| Answer::Info(fetch(l)));
}

/// The link of the current login, if any.
pub fn link() -> Option<Link> {
    SHARED.lock().unwrap().link.clone()
}

/// Runs a call on a thread with the current link; its answer comes back through `take`.
pub fn run(call: impl FnOnce(&Link) -> Answer + Send + 'static) {
    let Some((link, gen)) = ({ let s = SHARED.lock().unwrap(); s.link.clone().map(|l| (l, s.gen)) }) else { return };
    std::thread::spawn(move || {
        let a = call(&link);
        let mut s = SHARED.lock().unwrap();
        if s.gen == gen {
            s.answers.push((gen, a));
        }
    });
}

/// The answers that came back since the last call (for the current login only).
pub fn take() -> Vec<Answer> {
    std::mem::take(&mut SHARED.lock().unwrap().answers).into_iter().map(|(_, a)| a).collect()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).http_status_as_error(false).build().into()
}

/// A call's JSON answer; a status of 400 and up is an error with the server's message.
fn call(r: Result<ureq::http::Response<ureq::Body>, ureq::Error>, what: &str) -> Result<Value, String> {
    let mut r = r.map_err(|e| format!("{what}: {e}"))?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().map_err(|e| format!("{what}: {e}"))?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    if status >= 400 {
        return Err(v["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("{what}: {status} {v}")));
    }
    Ok(v)
}

pub fn to_json(o: &ObjState) -> Value {
    json!({ "address": o.address, "pos": o.at.translation.to_array(), "rot": o.at.rotation.to_array(), "scale": o.at.scale.to_array() })
}

pub fn from_json(v: &Value) -> Option<ObjState> {
    let f = |k: &str, n: usize| -> Option<Vec<f32>> {
        let a: Vec<f32> = v[k].as_array()?.iter().filter_map(|x| x.as_f64().map(|x| x as f32)).collect();
        (a.len() == n).then_some(a)
    };
    let (p, r, s) = (f("pos", 3)?, f("rot", 4)?, f("scale", 3)?);
    let at = Transform { translation: Vec3::from_slice(&p), rotation: Quat::from_slice(&r).normalize(), scale: Vec3::from_slice(&s) };
    Some(ObjState { address: v["address"].as_str()?.to_string(), at })
}

pub fn fetch(l: &Link) -> Result<RoomInfo, String> {
    let url = format!("{}/creations/v1/room/{}", l.api, l.room);
    let v = call(agent().get(&url).header("Authorization", format!("Bearer {}", l.token)).call(), &url)?;
    Ok(RoomInfo {
        room: l.room.clone(),
        can_edit: v["canEdit"].as_bool().unwrap_or(false),
        owner: v["ownerName"].as_str().map(str::to_string),
        version: v["version"].as_u64().unwrap_or(0),
        objects: v["objects"].as_array().map(|a| a.iter().filter_map(from_json).collect()).unwrap_or_default(),
    })
}

/// Saves the room's maker objects over `version`; the new version.
pub fn save(l: &Link, version: Option<u64>, objects: &[ObjState]) -> Result<u64, String> {
    let url = format!("{}/creations/v1/room/{}", l.api, l.room);
    let body = json!({ "baseVersion": version, "objects": objects.iter().map(to_json).collect::<Vec<_>>() });
    let v = call(agent().put(&url).header("Authorization", format!("Bearer {}", l.token)).send_json(body), &url)?;
    v["version"].as_u64().ok_or_else(|| format!("{url}: no version in {v}"))
}

/// Makes a room of this player's own named `name` holding `objects`; its name as the server keeps it.
pub fn publish(l: &Link, name: &str, objects: &[ObjState]) -> Result<String, String> {
    let url = format!("{}/creations/v1/rooms", l.api);
    let body = json!({ "name": name, "objects": objects.iter().map(to_json).collect::<Vec<_>>() });
    let v = call(agent().post(&url).header("Authorization", format!("Bearer {}", l.token)).send_json(body), &url)?;
    Ok(v["name"].as_str().unwrap_or(name).to_string())
}

/// Lets the players `names` build in this room (the owner only); their account ids.
pub fn builders(l: &Link, names: &[String]) -> Result<Vec<i64>, String> {
    let url = format!("{}/creations/v1/room/{}/editors", l.api, l.room);
    let v = call(agent().put(&url).header("Authorization", format!("Bearer {}", l.token)).send_json(json!({ "names": names })), &url)?;
    Ok(v["editors"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default())
}
