//! What makers build: the Maker Pen's shapes and props saved per room, and player-made rooms.
//!
//! One JSON file per room in `<data-dir>/creations/<room>.json`, where `<room>` is the realtime room name (the
//! instance's `photonRoomId`): `dorm-<account>` (the dorm's owner may build), `room-<id>` (built-in rooms: nobody
//! unless a file names an owner) or `made-<slug>` (player-made rooms, made by `POST .../rooms`). Each save bumps
//! `version`; the previous `KEEP` saves stay beside it as `<room>.<version>.json`. `format` is the file layout.
//!
//! Routes (signed in): `GET /api/creations/v1/rooms` (player-made rooms), `POST /api/creations/v1/rooms {name,
//! description?, objects?}` (a new room owned by the caller), `GET /api/creations/v1/room/{room}` (the save plus
//! `canEdit` for the caller), `PUT /api/creations/v1/room/{room} {baseVersion?, objects}` (owner and builders only;
//! 409 when `baseVersion` is not the saved version), `PUT /api/creations/v1/room/{room}/editors {editors}` (owner).
//! Matchmaking and the room list call `goto` and `listed` for player-made rooms.
use std::{path::PathBuf, sync::Mutex};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{auth::caller, store::Account, App, Shared};

pub const FORMAT: u32 = 1;
/// older saves kept per room
const KEEP: u64 = 3;
const MAX_OBJECTS: usize = 4000;
/// player-made rooms get room ids from here up
const FIRST_MADE_ID: i64 = 1000;

/// One maker object: its network address (`maker/<shape>/<colour>/<material>#m<uid>` for shapes, a prefab address
/// plus `#m<uid>` for props) and where it stands.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Creation {
    pub address: String,
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoomSave {
    pub format: u32,
    pub room: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub owner: Option<i64>,
    #[serde(default)]
    pub editors: Vec<i64>,
    pub version: u64,
    #[serde(default)]
    pub updated: String,
    /// player-made rooms: their RecNet room id
    #[serde(default)]
    pub room_id: Option<i64>,
    /// the level a player-made room is built in (INFERRED default: the sandbox)
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub objects: Vec<Creation>,
}

impl RoomSave {
    pub fn can_edit(&self, account: i64) -> bool {
        self.owner == Some(account) || self.editors.contains(&account)
    }
}

/// Saves of one server, one at a time (they are small and rare).
static LOCK: Mutex<()> = Mutex::new(());

fn dir(app: &App) -> PathBuf {
    app.data_dir.join("creations")
}

/// A room name fit for a file name: lower-case letters, digits and '-', 1..64 long.
pub fn valid_room(room: &str) -> bool {
    (1..=64).contains(&room.len()) && room.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A player-made room's name as a room key part: "Ada's Fort!" -> "ada-s-fort".
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    s.trim_end_matches('-').chars().take(40).collect()
}

/// The saved room, or what a room with no save starts as (`None` for an unknown player-made room).
pub fn load(app: &App, room: &str) -> Option<RoomSave> {
    if !valid_room(room) {
        return None;
    }
    if let Ok(text) = std::fs::read_to_string(dir(app).join(format!("{room}.json"))) {
        return serde_json::from_str(&text).ok();
    }
    if room.starts_with("made-") {
        return None;
    }
    let owner = room.strip_prefix("dorm-").and_then(|id| id.parse().ok());
    Some(RoomSave { format: FORMAT, room: room.into(), name: room.into(), description: String::new(), owner, editors: vec![], version: 0,
        updated: String::new(), room_id: None, base: String::new(), objects: vec![] })
}

/// Writes `save` as the next version (the previous one is kept, the oldest beyond `KEEP` removed).
fn write(app: &App, save: &mut RoomSave) -> std::io::Result<()> {
    let d = dir(app);
    std::fs::create_dir_all(&d)?;
    let path = d.join(format!("{}.json", save.room));
    if save.version > 0 && path.exists() {
        let _ = std::fs::rename(&path, d.join(format!("{}.{}.json", save.room, save.version)));
        if save.version > KEEP {
            let _ = std::fs::remove_file(d.join(format!("{}.{}.json", save.room, save.version - KEEP)));
        }
    }
    save.format = FORMAT;
    save.version += 1;
    save.updated = crate::store::now();
    let tmp = d.join(format!("{}.json.tmp", save.room));
    std::fs::write(&tmp, serde_json::to_vec_pretty(save).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

fn check(objects: &[Creation]) -> Result<(), String> {
    if objects.len() > MAX_OBJECTS {
        return Err(format!("at most {MAX_OBJECTS} objects"));
    }
    for o in objects {
        let finite = o.pos.iter().chain(&o.rot).chain(&o.scale).all(|v| v.is_finite());
        if o.address.is_empty() || o.address.len() > 200 || !finite || o.scale.iter().any(|s| !(0.001..=1000.0).contains(&s.abs())) {
            return Err(format!("bad object {:?}", o.address));
        }
    }
    Ok(())
}

/// Saves `objects` in `room` for `who`: the new version, or why not.
pub fn save(app: &App, who: i64, room: &str, base_version: Option<u64>, objects: Vec<Creation>) -> Result<u64, (StatusCode, String)> {
    let _g = LOCK.lock().unwrap();
    let mut s = load(app, room).ok_or((StatusCode::NOT_FOUND, "no such room".into()))?;
    if !s.can_edit(who) {
        return Err((StatusCode::FORBIDDEN, "only the room's owner and their builders can save it".into()));
    }
    if base_version.is_some_and(|v| v != s.version) {
        return Err((StatusCode::CONFLICT, format!("saved meanwhile: version {}", s.version)));
    }
    check(&objects).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    s.objects = objects;
    write(app, &mut s).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(s.version)
}

/// Makes a player-made room owned by `who`.
pub fn create(app: &App, who: i64, name: &str, description: &str, objects: Vec<Creation>) -> Result<RoomSave, (StatusCode, String)> {
    let _g = LOCK.lock().unwrap();
    let name = name.trim();
    let key = format!("made-{}", slug(name));
    if name.chars().count() > 40 || key == "made-" {
        return Err((StatusCode::BAD_REQUEST, "a room name is 1..40 characters with a letter or digit".into()));
    }
    if load(app, &key).is_some() {
        return Err((StatusCode::CONFLICT, "a room with that name exists".into()));
    }
    check(&objects).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let room_id = made(app).iter().filter_map(|r| r.room_id).max().map_or(FIRST_MADE_ID, |m| m + 1);
    let mut s = RoomSave { format: FORMAT, room: key, name: name.into(), description: description.chars().take(200).collect(), owner: Some(who),
        editors: vec![], version: 0, updated: String::new(), room_id: Some(room_id), base: "sandbox".into(), objects };
    write(app, &mut s).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(s)
}

/// Sets who besides the owner may build in `room`; only the owner may.
pub fn set_editors(app: &App, who: i64, room: &str, editors: Vec<i64>) -> Result<u64, (StatusCode, String)> {
    let _g = LOCK.lock().unwrap();
    let mut s = load(app, room).ok_or((StatusCode::NOT_FOUND, "no such room".into()))?;
    if s.owner != Some(who) {
        return Err((StatusCode::FORBIDDEN, "only the room's owner can choose its builders".into()));
    }
    s.editors = editors.into_iter().filter(|&e| e != who).take(64).collect();
    write(app, &mut s).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(s.version)
}

/// Every player-made room, oldest first.
pub fn made(app: &App) -> Vec<RoomSave> {
    let Ok(rd) = std::fs::read_dir(dir(app)) else { return vec![] };
    let mut v: Vec<RoomSave> = rd
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| Some(n.strip_suffix(".json")?.to_string()))
        .filter(|n| n.starts_with("made-") && !n.contains('.'))
        .filter_map(|n| load(app, &n))
        .collect();
    v.sort_by_key(|r| r.room_id);
    v
}

/// A player-made room in RecNet's room shape, for the rooms browser.
pub fn room_json(app: &App, r: &RoomSave) -> Value {
    let id = r.room_id.unwrap_or(0);
    let creator = r.owner.unwrap_or(0);
    let by = app.store.get(creator).map(|a| a.display_name).unwrap_or_default();
    json!({
        "RoomId": id, "Name": r.name, "Description": if r.description.is_empty() { format!("Made by {by}") } else { r.description.clone() },
        "CreatorAccountId": creator, "ImageName": "", "State": 0, "Accessibility": 1, "IsRRO": false,
        "SupportsLevelVoting": false, "SupportsScreens": true, "SupportsWalkVR": true, "SupportsTeleportVR": true,
        "SupportsVRLow": true, "SupportsQuest2": true, "SupportsMobile": true, "SupportsJuniors": true, "MinLevel": 0,
        "CreatedAt": r.updated, "Tags": [{ "Tag": "made", "Type": 0 }],
        "Stats": { "CheerCount": 0, "FavoriteCount": 0, "VisitorCount": 0, "VisitCount": 0 },
        "SubRooms": [{ "SubRoomId": id, "RoomId": id, "Name": "Home", "DataBlob": "", "IsSandbox": true,
                       "MaxPlayers": 20, "Accessibility": 1, "UnitySceneId": r.base }],
    })
}

/// The rooms browser's player-made rooms.
pub fn listed(app: &App) -> Vec<Value> {
    made(app).iter().map(|r| room_json(app, r)).collect()
}

/// Matchmaking into a player-made room by name, key or id: its room instance, or `None` if there is no such room.
pub fn goto(app: &App, a: &Account, name: &str) -> Option<Value> {
    let r = made(app).into_iter().find(|r| r.name.eq_ignore_ascii_case(name) || r.room == name || r.room_id.is_some_and(|id| id.to_string() == name)
        || r.room == format!("made-{}", slug(name)))?;
    let id = r.room_id.unwrap_or(0);
    let instance = json!({
        "roomInstanceId": id, "roomId": id, "subRoomId": id, "location": "", "dataBlob": "",
        "photonRegionId": "us", "photonRoomId": r.room, "name": format!("^{}", r.name),
        "maxCapacity": 20, "isFull": false, "isPrivate": false, "isInProgress": false, "roomInstanceType": 0,
    });
    Some(crate::party::placed(app, a, id, instance))
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/creations/v1/rooms", get(list_route).post(create_route))
        .route("/api/creations/v1/room/{room}", get(get_route).put(save_route))
        .route("/api/creations/v1/room/{room}/editors", put(editors_route))
}

fn who(app: &App, h: &HeaderMap) -> Result<Account, Response> {
    caller(app, h).ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response())
}

fn fail((status, msg): (StatusCode, String)) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

async fn list_route(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    who(&app, &h)?;
    let rooms: Vec<Value> = made(&app).iter().map(|r| json!({
        "room": r.room, "name": r.name, "roomId": r.room_id, "owner": r.owner, "version": r.version,
        "objects": r.objects.len(), "updated": r.updated,
    })).collect();
    Ok(Json(Value::Array(rooms)))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    objects: Vec<Creation>,
}

async fn create_route(State(app): State<Shared>, h: HeaderMap, Json(b): Json<CreateBody>) -> Result<(StatusCode, Json<Value>), Response> {
    let a = who(&app, &h)?;
    let s = create(&app, a.account_id, &b.name, &b.description, b.objects).map_err(fail)?;
    println!("  {} made room {} ({})", a.username, s.name, s.room);
    Ok((StatusCode::CREATED, Json(json!({ "room": s.room, "name": s.name, "roomId": s.room_id, "version": s.version }))))
}

async fn get_route(State(app): State<Shared>, h: HeaderMap, Path(room): Path<String>) -> Result<Json<Value>, Response> {
    let a = who(&app, &h)?;
    let s = load(&app, &room).ok_or_else(|| fail((StatusCode::NOT_FOUND, "no such room".into())))?;
    let mut v = serde_json::to_value(&s).unwrap_or_default();
    v["canEdit"] = json!(s.can_edit(a.account_id));
    v["ownerName"] = json!(s.owner.and_then(|o| app.store.get(o)).map(|o| o.display_name));
    Ok(Json(v))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveBody {
    #[serde(default)]
    base_version: Option<u64>,
    objects: Vec<Creation>,
}

async fn save_route(State(app): State<Shared>, h: HeaderMap, Path(room): Path<String>, Json(b): Json<SaveBody>) -> Result<Json<Value>, Response> {
    let a = who(&app, &h)?;
    let n = b.objects.len();
    let version = save(&app, a.account_id, &room, b.base_version, b.objects).map_err(fail)?;
    println!("  {} saved {room}: {n} objects, version {version}", a.username);
    Ok(Json(json!({ "room": room, "version": version })))
}

/// Builders by account id, or by name (user name or display name: the players the owner sees in the room).
#[derive(Deserialize)]
struct EditorsBody {
    #[serde(default)]
    editors: Vec<i64>,
    #[serde(default)]
    names: Vec<String>,
}

async fn editors_route(State(app): State<Shared>, h: HeaderMap, Path(room): Path<String>, Json(b): Json<EditorsBody>) -> Result<Json<Value>, Response> {
    let a = who(&app, &h)?;
    let mut ids = b.editors;
    ids.extend(app.store.read(|d| {
        d.accounts.iter().filter(|x| b.names.iter().any(|n| n.eq_ignore_ascii_case(&x.username) || n.eq_ignore_ascii_case(&x.display_name)))
            .map(|x| x.account_id).collect::<Vec<_>>()
    }));
    ids.sort_unstable();
    ids.dedup();
    let version = set_editors(&app, a.account_id, &room, ids.clone()).map_err(fail)?;
    println!("  {} lets {ids:?} build in {room}", a.username);
    Ok(Json(json!({ "room": room, "version": version, "editors": ids })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> Shared {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("mrc-creations-{}-{nanos}", std::process::id()));
        App::open(&d, "http://127.0.0.1:1").unwrap()
    }

    fn cube(x: f32) -> Creation {
        Creation { address: "maker/cube/1/9#m1".into(), pos: [x, 1.0, 0.0], rot: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] }
    }

    #[test]
    fn names_become_room_keys() {
        assert_eq!(slug("Ada's Fort!"), "ada-s-fort");
        assert_eq!(slug("  --Big   Room-- "), "big-room");
        assert!(valid_room("dorm-12") && valid_room("made-ada-s-fort"));
        assert!(!valid_room("../x") && !valid_room("Dorm") && !valid_room(""));
    }

    #[test]
    fn the_dorm_owner_saves_and_others_may_not() {
        let app = app();
        let (ada, bo) = (app.store.login("name:ada", "Ada").account_id, app.store.login("name:bo", "Bo").account_id);
        let room = format!("dorm-{ada}");
        assert_eq!(load(&app, &room).unwrap().owner, Some(ada));
        assert_eq!(save(&app, ada, &room, Some(0), vec![cube(1.0)]), Ok(1));
        assert_eq!(save(&app, bo, &room, None, vec![]).unwrap_err().0, StatusCode::FORBIDDEN);
        // built-in rooms have no owner: nobody builds there
        assert_eq!(save(&app, ada, "room-2", None, vec![]).unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(load(&app, &room).unwrap().objects, vec![cube(1.0)]);
    }

    #[test]
    fn saves_are_versioned_and_stale_saves_refused() {
        let app = app();
        let ada = app.store.login("name:ada", "Ada").account_id;
        let room = format!("dorm-{ada}");
        for i in 0..5 {
            assert_eq!(save(&app, ada, &room, Some(i), vec![cube(i as f32)]), Ok(i + 1));
        }
        assert_eq!(save(&app, ada, &room, Some(2), vec![]).unwrap_err().0, StatusCode::CONFLICT);
        let s = load(&app, &room).unwrap();
        assert_eq!((s.version, s.format, s.objects[0].pos[0]), (5, FORMAT, 4.0));
        // the last KEEP older saves are kept, older ones removed
        let d = dir(&app);
        assert!(d.join(format!("{room}.4.json")).exists() && d.join(format!("{room}.2.json")).exists());
        assert!(!d.join(format!("{room}.1.json")).exists());
        let old: RoomSave = serde_json::from_str(&std::fs::read_to_string(d.join(format!("{room}.4.json"))).unwrap()).unwrap();
        assert_eq!(old.objects[0].pos[0], 3.0);
    }

    #[test]
    fn bad_objects_are_refused() {
        let app = app();
        let ada = app.store.login("name:ada", "Ada").account_id;
        let room = format!("dorm-{ada}");
        let mut nan = cube(0.0);
        nan.pos[1] = f32::NAN;
        assert_eq!(save(&app, ada, &room, None, vec![nan]).unwrap_err().0, StatusCode::BAD_REQUEST);
        let mut flat = cube(0.0);
        flat.scale[2] = 0.0;
        assert_eq!(save(&app, ada, &room, None, vec![flat]).unwrap_err().0, StatusCode::BAD_REQUEST);
        assert_eq!(save(&app, ada, &room, None, vec![cube(0.0); MAX_OBJECTS + 1]).unwrap_err().0, StatusCode::BAD_REQUEST);
        assert!(load(&app, "../etc").is_none());
    }

    #[test]
    fn made_rooms_are_listed_joinable_and_shared_with_builders() {
        let app = app();
        let ada_acc = app.store.login("name:ada", "Ada");
        let (ada, bo) = (ada_acc.account_id, app.store.login("name:bo", "Bo").account_id);
        let r = create(&app, ada, "Ada's Fort", "", vec![cube(2.0)]).unwrap();
        assert_eq!((r.room.as_str(), r.room_id, r.version, r.owner), ("made-ada-s-fort", Some(FIRST_MADE_ID), 1, Some(ada)));
        assert_eq!(create(&app, bo, "ada's fort", "", vec![]).unwrap_err().0, StatusCode::CONFLICT);
        assert_eq!(create(&app, bo, "!!!", "", vec![]).unwrap_err().0, StatusCode::BAD_REQUEST);
        let r2 = create(&app, bo, "Bo Land", "", vec![]).unwrap();
        assert_eq!(r2.room_id, Some(FIRST_MADE_ID + 1));

        let list = listed(&app);
        assert_eq!(list.len(), 2);
        assert_eq!((list[0]["Name"].as_str(), list[0]["Description"].as_str()), (Some("Ada's Fort"), Some("Made by Ada")));
        let inst = goto(&app, &ada_acc, "Ada's Fort").unwrap();
        assert_eq!(inst["photonRoomId"], "made-ada-s-fort");
        assert!(goto(&app, &ada_acc, &FIRST_MADE_ID.to_string()).is_some());
        assert!(goto(&app, &ada_acc, "nowhere").is_none());

        // Bo may build once Ada lets him; only Ada chooses
        assert_eq!(save(&app, bo, &r.room, None, vec![]).unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(set_editors(&app, bo, &r.room, vec![bo]).unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(set_editors(&app, ada, &r.room, vec![bo, ada]), Ok(2));
        assert_eq!(load(&app, &r.room).unwrap().editors, vec![bo]);
        assert_eq!(save(&app, bo, &r.room, Some(2), vec![cube(3.0), cube(4.0)]), Ok(3));
    }
}
