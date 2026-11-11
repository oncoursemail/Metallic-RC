//! Accounts, the API service (config, settings, avatar, inventory, social), rooms and matchmaking.
//!
//! Shapes follow what earlier RecNet clients read (PascalCase on api/rooms, camelCase on accounts/matchmaking). The
//! 2026 client may want more or renamed fields: each client run's requests and unknown.jsonl show what to change.
//! Empty lists stand for "you own nothing / no friends / no messages yet".
use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::{auth::caller, store::Account, Shared};

pub fn routes() -> Router<Shared> {
    Router::new()
        // accounts
        .route("/accounts/account/me", get(me))
        .route("/accounts/account/bulk", get(accounts_bulk))
        .route("/accounts/account/{id}", get(account))
        // api
        .route("/api/config/v2", get(config))
        .route("/api/gameconfigs/v1/all", get(empty_list))
        .route("/api/settings/v2", get(empty_list).post(ok))
        .route("/api/settings/v2/set", post(ok))
        .route("/api/avatar/v4/items", get(empty_list))
        .route("/api/avatar/v3/items", get(empty_list))
        .route("/api/equipment/v2/getUnlocked", get(empty_list))
        .route("/api/consumables/v2/getUnlocked", get(empty_list))
        .route("/api/playerevents/v1/all", get(player_events))
        .route("/api/objectives/v1/myprogress", get(objectives))
        // rooms
        .route("/rooms/rooms/bulk", get(rooms_bulk))
        .route("/rooms/rooms", get(rooms_bulk))
        .route("/rooms/rooms/hot", get(rooms_hot))
        .route("/rooms/rooms/{id}", get(room))
        // matchmaking (presence, parties and joining friends: social.rs, party.rs)
        .route("/matchmaking/goto/room/{name}", post(goto_room))
        .route("/matchmaking/goto/none", post(ok))
}

/// A request that needs a signed-in caller: the account, or 401.
fn who(app: &crate::App, h: &HeaderMap) -> Result<Account, Response> {
    caller(app, h).ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response())
}

fn query_all(q: &Option<String>, key: &str) -> Vec<String> {
    q.as_deref()
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.replace('+', " ").replace("%20", " "))
        .collect()
}

fn account_json(a: &Account) -> Value {
    json!({
        "accountId": a.account_id, "username": a.username, "displayName": a.display_name,
        "profileImage": "", "bannerImage": "", "isJunior": false, "platforms": 1,
        "personalPronouns": 0, "identityFlags": 0, "createdAt": a.created_at,
    })
}

async fn ok() -> StatusCode {
    StatusCode::OK
}
async fn empty_list() -> Json<Value> {
    Json(json!([]))
}

async fn me(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    Ok(Json(account_json(&who(&app, &h)?)))
}

async fn account(State(app): State<Shared>, Path(id): Path<i64>) -> Result<Json<Value>, StatusCode> {
    app.store.get(id).map(|a| Json(account_json(&a))).ok_or(StatusCode::NOT_FOUND)
}

async fn accounts_bulk(State(app): State<Shared>, RawQuery(q): RawQuery) -> Json<Value> {
    let ids = query_all(&q, "id").into_iter().filter_map(|s| s.parse().ok());
    Json(Value::Array(ids.filter_map(|id| app.store.get(id)).map(|a| account_json(&a)).collect()))
}

async fn config(State(app): State<Shared>) -> Json<Value> {
    Json(json!({
        "MessageOfTheDay": "Welcome to Metallic-RC",
        "CdnBaseUri": crate::discovery::service_url(&app, "CDN"),
        "ShareBaseUrl": crate::discovery::service_url(&app, "WWW"),
        "LevelProgressionMaps": [], "DailyObjectives": [], "ConfigTable": [],
        "MatchmakingParams": {
            "PreferFullRoomsFrequency": 1.0, "PreferEmptyRoomsFrequency": 0.0
        },
        "PhotonConfig": { "CloudRegion": "us", "CrcCheckEnabled": false, "EnableServerTracingAfterDisconnect": false },
        "AutoMicMutingConfig": { "MicSpamVolumeThreshold": 1.125, "MicVolumeSampleInterval": 0.25, "MicVolumeSampleRollingWindowLength": 15.0, "MicSpamSamplePercentageForWarning": 0.8, "MicSpamSamplePercentageForForceMute": 0.8, "MicSpamWarningStateVolumeMultiplier": 0.25 },
    }))
}

async fn player_events() -> Json<Value> {
    Json(json!({ "Created": [], "Responses": [] }))
}

async fn objectives() -> Json<Value> {
    Json(json!({ "Objectives": [], "ObjectiveGroups": [] }))
}

// ---- rooms: the built-in rooms every player has; user-made rooms come later ----
struct RoomDef {
    id: i64,
    /// the room's name in matchmaking (also accepted by `goto`)
    name: &'static str,
    /// the client world to load for this room (`--world`): a decrypted 2026 scene name, or `dorm` for the bridged
    /// Dorm. Sent to the client as the sub-room's `UnitySceneId`.
    scene: &'static str,
    description: &'static str,
    max_players: i64,
    /// private (one instance per player, like the Dorm) vs shared
    private: bool,
}

/// The built-in environments, loaded from the decrypted 2026 scenes (the Dorm keeps its bridged look). The client
/// loads `scene` through `world::load`. Internal scenes (main_root, logout, additive, title screen, orientation
/// bootstraps) are not offered as rooms.
const ROOMS: &[RoomDef] = &[
    RoomDef { id: 1, name: "DormRoom", scene: "dorm", description: "Your private dorm room.", max_players: 4, private: true },
    RoomDef { id: 2, name: "RecCenter", scene: "reccenter", description: "The Rec Center, where everyone meets.", max_players: 40, private: false },
    RoomDef { id: 3, name: "Charades", scene: "charades", description: "Act it out in Charades.", max_players: 16, private: false },
    RoomDef { id: 4, name: "DiscGolf", scene: "discgolf", description: "Disc Golf.", max_players: 16, private: false },
    RoomDef { id: 5, name: "DiscGolfPropulsion", scene: "Discgolf_Propulsion", description: "Disc Golf: Propulsion.", max_players: 16, private: false },
    RoomDef { id: 6, name: "Dodgeball", scene: "dodgeball", description: "Dodgeball arena.", max_players: 16, private: false },
    RoomDef { id: 7, name: "Paddleball", scene: "paddleball", description: "Paddleball court.", max_players: 16, private: false },
    RoomDef { id: 8, name: "EventRoom", scene: "eventroom", description: "The Event Room.", max_players: 40, private: false },
    RoomDef { id: 9, name: "PaintballRiver", scene: "paintball", description: "Paintball: River.", max_players: 16, private: false },
    RoomDef { id: 10, name: "PaintballConstruction", scene: "paintball2_open", description: "Paintball: Construction.", max_players: 16, private: false },
    RoomDef { id: 11, name: "PaintballCastle", scene: "Paintball_Castle", description: "Paintball: Castle.", max_players: 16, private: false },
    RoomDef { id: 12, name: "PaintballClearCut", scene: "Paintball_ClearCut", description: "Paintball: Clear Cut.", max_players: 16, private: false },
    RoomDef { id: 13, name: "PaintballDam", scene: "Paintball_Dam", description: "Paintball: Dam.", max_players: 16, private: false },
    RoomDef { id: 14, name: "QuestFoyer", scene: "Quest_Foyer", description: "Quest: the Goblin King's Foyer.", max_players: 4, private: false },
    RoomDef { id: 15, name: "QuestLibrary", scene: "Quest_Library", description: "Quest: the Library.", max_players: 4, private: false },
    RoomDef { id: 16, name: "QuestBattlefield", scene: "Quest_Battlefield", description: "Quest: the Battlefield.", max_players: 4, private: false },
    RoomDef { id: 17, name: "QuestCafeteria", scene: "Quest_Cafeteria", description: "Quest: the Cafeteria.", max_players: 4, private: false },
    RoomDef { id: 18, name: "QuestScifiFoyer", scene: "Quest_Scifi_Foyer", description: "Quest: Sci-Fi Foyer.", max_players: 4, private: false },
    RoomDef { id: 19, name: "OrientationPracticeGym", scene: "Orientation_PracticeGym", description: "Orientation: Practice Gym.", max_players: 16, private: false },
];

fn room_json(r: &RoomDef) -> Value {
    json!({
        "RoomId": r.id, "Name": r.name, "Description": r.description, "CreatorAccountId": 1,
        "ImageName": "", "State": 0, "Accessibility": if r.private { 0 } else { 1 }, "IsRRO": true,
        "SupportsLevelVoting": false, "SupportsScreens": true, "SupportsWalkVR": true, "SupportsTeleportVR": true,
        "SupportsVRLow": true, "SupportsQuest2": true, "SupportsMobile": true, "SupportsJuniors": true, "MinLevel": 0,
        "CreatedAt": "2026-01-01T00:00:00Z", "Tags": [],
        "Stats": { "CheerCount": 0, "FavoriteCount": 0, "VisitorCount": 0, "VisitCount": 0 },
        // UnitySceneId: the decrypted 2026 scene the client loads for this room (`world::load` accepts this name).
        "SubRooms": [{ "SubRoomId": r.id, "RoomId": r.id, "Name": "Home", "DataBlob": "", "IsSandbox": false,
                       "MaxPlayers": r.max_players, "Accessibility": 1, "UnitySceneId": r.scene }],
    })
}

fn find_room(name_or_id: &str) -> Option<&'static RoomDef> {
    ROOMS.iter().find(|r| r.name.eq_ignore_ascii_case(name_or_id) || r.scene.eq_ignore_ascii_case(name_or_id) || r.id.to_string() == name_or_id)
}

async fn rooms_bulk(RawQuery(q): RawQuery) -> Json<Value> {
    let mut keys = query_all(&q, "name");
    keys.extend(query_all(&q, "id"));
    Json(Value::Array(keys.iter().filter_map(|k| find_room(k)).map(room_json).collect()))
}

/// Every room this server has, for the rooms browser: `{Results, TotalResults}` as RecNet's paged room lists send
/// them. `take` and `skip` page it. Player counts are not in it yet (they live in the realtime room server).
async fn rooms_hot(RawQuery(q): RawQuery) -> Json<Value> {
    let num = |k: &str| query_all(&q, k).first().and_then(|v| v.parse::<usize>().ok());
    let rooms: Vec<Value> = ROOMS.iter().skip(num("skip").unwrap_or(0)).take(num("take").unwrap_or(usize::MAX)).map(room_json).collect();
    Json(json!({ "Results": rooms, "TotalResults": ROOMS.len() }))
}

async fn room(Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    find_room(&id).map(|r| Json(room_json(r))).ok_or(StatusCode::NOT_FOUND)
}

// ---- matchmaking ----
/// Puts the caller in an instance of the room. The dorm is private: one instance per player. Photon is where players
/// actually meet; `photonRoomId` is the room name on the realtime server (not served by this program yet).
async fn goto_room(State(app): State<Shared>, h: HeaderMap, Path(name): Path<String>) -> Result<Json<Value>, Response> {
    let a = who(&app, &h)?;
    let Some(r) = find_room(&name) else {
        return Ok(Json(json!({ "errorCode": 1, "roomInstance": null })));
    };
    let instance_id = if r.private { 1_000_000 + a.account_id } else { r.id };
    let photon_room = if r.private { format!("{}-{}", r.scene, a.account_id) } else { format!("room-{}", r.id) };
    let instance = json!({
        "roomInstanceId": instance_id, "roomId": r.id, "subRoomId": r.id, "location": "", "dataBlob": "",
        "photonRegionId": "us", "photonRoomId": photon_room, "name": format!("^{}", r.name),
        "maxCapacity": r.max_players, "isFull": false, "isPrivate": r.private,
        "isInProgress": false, "roomInstanceType": 0,
    });
    println!("  {} -> {}", a.username, r.name);
    let instance = crate::party::placed(&app, &a, r.id, instance);
    Ok(Json(json!({ "errorCode": 0, "roomInstance": instance })))
}
