//! Friends, blocking, presence and notifications, plus the live (in-memory) state the social modules share.
//!
//! Relationships use the game's `api/relationships/v2/*` paths and its `Relationship` shape
//! `{PlayerID, RelationshipType (0 None, 1 FriendRequestSent, 2 FriendRequestReceived, 3 Friend), Muted, Ignored,
//! Favorited (0 None, 1 Local, 2 Remote, 3 Mutual), VoiceVolume}`. Blocking is the game's "Ignored": a blocker sees
//! Ignored = Local; the blocked player sees nothing (no relationship, the blocker offline). Blocked players cannot
//! send friend requests, party invites or join the blocker.
//!
//! Notifications are stored as the game's `Message` (`api/messages/v2/get`) and pushed over the hub as the
//! `Notification` target with `{Id, Msg}` (`NotificationDto`): `RelationshipChanged`, `MessageReceived`,
//! `SubscriptionUpdatePresence`, `SubscriptionUpdateProfile` and our own `PartyChanged`.
//! INFERRED: whether `Id` is the `PushNotificationId` name or its number (we send the name); the
//! `sendfriendrequest`/`acceptfriendrequest`/`declinefriendrequest`/`removefriend`/`ignore`/`unignore` path names and
//! verbs; the `MessageType` numbers (declaration order in the dump, values not seen).
//!
//! Presence: online = a notifications hub is open, or the last matchmaking login/heartbeat is under 90 s old.
//! Live state (presence, open hubs, parties) is not saved; it is rebuilt as players log in again.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use axum::{
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    auth::caller,
    store::{now, Account, Data, Message, RelKind, Relation},
    App, Shared,
};

/// `RecNet.Message.MessageType` numbers we use. INFERRED: declaration order in the dump.
pub mod message_type {
    pub const FRIEND_INVITE: i32 = 4;
    pub const PARTY_ACTIVITY_SWITCH_V2: i32 = 7;
    pub const PARTY_INVITE: i32 = 8;
    pub const FRIEND_REQUEST_ACCEPTED: i32 = 13;
}

const ONLINE_FOR: Duration = Duration::from_secs(90);
/// stored notifications per player; older ones are dropped
const KEEP_MESSAGES: usize = 200;

#[derive(Default)]
pub struct Presence {
    pub seen: Option<Instant>,
    /// the room instance matchmaking last put the player in (as `goto` answered it)
    pub instance: Option<Value>,
}

/// A party: the leader takes everyone along when going to a room.
#[derive(Clone)]
pub struct Party {
    pub id: i64,
    pub leader: i64,
    pub members: Vec<i64>,
    pub invited: Vec<i64>,
    /// where the leader last went
    pub instance: Option<Value>,
}

#[derive(Default)]
pub struct LiveState {
    pub presence: HashMap<i64, Presence>,
    /// open hub connections per account
    pub hubs: HashMap<i64, Vec<(u64, UnboundedSender<String>)>>,
    pub next_hub: u64,
    /// hub connection tokens handed out by an authenticated negotiate, waiting for their WebSocket
    pub negotiated: HashMap<String, i64>,
    pub parties: Vec<Party>,
    pub next_party: i64,
}

impl LiveState {
    pub fn online(&self, id: i64) -> bool {
        self.hubs.get(&id).is_some_and(|h| !h.is_empty())
            || self.presence.get(&id).and_then(|p| p.seen).is_some_and(|t| t.elapsed() < ONLINE_FOR)
    }

    pub fn party_of(&self, id: i64) -> Option<&Party> {
        self.parties.iter().find(|p| p.members.contains(&id))
    }
}

#[derive(Default)]
pub struct Live(pub std::sync::Mutex<LiveState>);

impl Live {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, LiveState> {
        self.0.lock().unwrap()
    }
}

/// Sends a `Notification` to every hub `to` has open.
pub fn push(app: &App, to: i64, id: &str, msg: Value) {
    let frame = format!("{}\u{1e}", json!({ "type": 1, "target": "Notification", "arguments": [{ "Id": id, "Msg": msg }] }));
    if let Some(hubs) = app.live.lock().hubs.get(&to) {
        for (_, tx) in hubs {
            let _ = tx.send(frame.clone());
        }
    }
}

// ---- request helpers shared by the social modules ----

/// A request that needs a signed-in caller: the account, or 401.
pub fn who(app: &App, h: &HeaderMap) -> Result<Account, Response> {
    caller(app, h).ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response())
}

pub fn fail(status: StatusCode, why: &str) -> Response {
    (status, Json(json!({ "error": why }))).into_response()
}

/// Request fields from the query string, a form body or a JSON object body (later ones win).
pub struct Params(HashMap<String, Value>);

impl Params {
    pub fn new(q: &Option<String>, body: &[u8]) -> Params {
        let mut m = HashMap::new();
        let mut form = |s: &[u8]| {
            for (k, v) in serde_urlencoded::from_bytes::<Vec<(String, String)>>(s).unwrap_or_default() {
                m.insert(k.to_ascii_lowercase(), Value::String(v));
            }
        };
        form(q.as_deref().unwrap_or("").as_bytes());
        match serde_json::from_slice::<Value>(body) {
            Ok(Value::Object(o)) => {
                for (k, v) in o {
                    m.insert(k.to_ascii_lowercase(), v);
                }
            }
            _ => form(body),
        }
        Params(m)
    }

    /// The first of `keys` present (case-insensitive).
    pub fn get(&self, keys: &[&str]) -> Option<&Value> {
        keys.iter().find_map(|k| self.0.get(&k.to_ascii_lowercase()))
    }

    pub fn text(&self, keys: &[&str]) -> Option<String> {
        self.get(keys).and_then(|v| v.as_str().map(str::to_string))
    }

    pub fn id(&self, keys: &[&str]) -> Option<i64> {
        self.get(keys).and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
    }

    /// The other player a request names: `id`, `playerId` or `accountId`.
    pub fn player(&self) -> Option<i64> {
        self.id(&["id", "playerId", "accountId"])
    }
}

// ---- relationships ----

fn has(d: &Data, from: i64, to: i64, kind: RelKind) -> bool {
    d.relations.iter().any(|r| r.from == from && r.to == to && r.kind == kind)
}

pub fn friends(d: &Data, a: i64, b: i64) -> bool {
    has(d, a, b, RelKind::Friend)
}

/// Either one blocked the other.
pub fn blocked(d: &Data, a: i64, b: i64) -> bool {
    has(d, a, b, RelKind::Block) || has(d, b, a, RelKind::Block)
}

fn unlink(d: &mut Data, a: i64, b: i64, kinds: &[RelKind]) {
    d.relations.retain(|r| !(kinds.contains(&r.kind) && ((r.from == a && r.to == b) || (r.from == b && r.to == a))));
}

/// How `me` sees `other`, in the game's `Relationship` shape.
pub fn relationship(d: &Data, me: i64, other: i64) -> Value {
    let kind = if friends(d, me, other) {
        3
    } else if has(d, me, other, RelKind::Request) {
        1
    } else if has(d, other, me, RelKind::Request) {
        2
    } else {
        0
    };
    let ignored = if has(d, me, other, RelKind::Block) { 1 } else { 0 };
    json!({ "PlayerID": other, "RelationshipType": kind, "Muted": 0, "Ignored": ignored, "Favorited": 0, "VoiceVolume": null })
}

/// Everyone `me` has a relationship with that `me` may see (not players who blocked `me`).
fn related(d: &Data, me: i64) -> Vec<i64> {
    let mut ids: Vec<i64> = d
        .relations
        .iter()
        .filter_map(|r| if r.from == me { Some(r.to) } else if r.to == me && r.kind != RelKind::Block { Some(r.from) } else { None })
        .collect();
    ids.sort();
    ids.dedup();
    ids.retain(|&o| {
        let v = relationship(d, me, o);
        v["RelationshipType"] != 0 || v["Ignored"] != 0
    });
    ids
}

/// Stores a notification for `to` and returns it in the game's shape (to push).
pub fn notify(d: &mut Data, to: i64, from: i64, message_type: i32, kind: &str, data: &str, room_id: Option<i64>) -> Value {
    d.next_message_id = d.next_message_id.max(1);
    let m = Message {
        id: d.next_message_id,
        to,
        from,
        sent: now(),
        message_type,
        kind: kind.into(),
        data: data.into(),
        room_id,
        read: false,
    };
    d.next_message_id += 1;
    d.messages.push(m.clone());
    let mine: Vec<i64> = d.messages.iter().filter(|m| m.to == to).map(|m| m.id).collect();
    if mine.len() > KEEP_MESSAGES {
        let cut = mine[mine.len() - KEEP_MESSAGES];
        d.messages.retain(|m| m.to != to || m.id >= cut);
    }
    message_json(&m)
}

pub fn message_json(m: &Message) -> Value {
    json!({
        "Id": m.id, "FromPlayerId": m.from, "SentTime": m.sent, "Type": m.message_type, "Data": m.data,
        "RoomId": m.room_id, "PlayerEventId": null, "Kind": m.kind, "Read": m.read,
    })
}

#[derive(Clone, Copy)]
enum Act {
    Request,
    Accept,
    Decline,
    Remove,
    Block,
    Unblock,
}

macro_rules! acts {
    ($($name:ident => $act:expr),*) => {$(
        async fn $name(s: State<Shared>, h: HeaderMap, q: RawQuery, b: Bytes) -> Response {
            act(s, h, q, b, $act).await
        }
    )*};
}
acts!(send_request => Act::Request, accept => Act::Accept, decline => Act::Decline, remove => Act::Remove,
      block => Act::Block, unblock => Act::Unblock);

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/relationships/v2/get", get(list_relationships))
        .route("/api/relationships/v2/sendfriendrequest", post(send_request))
        .route("/api/relationships/v2/acceptfriendrequest", post(accept))
        .route("/api/relationships/v2/declinefriendrequest", post(decline))
        .route("/api/relationships/v2/removefriend", post(remove))
        .route("/api/relationships/v2/ignore", post(block))
        .route("/api/relationships/v2/unignore", post(unblock))
        .route("/api/social/v1/friends", get(friend_list))
        .route("/api/messages/v2/get", get(list_messages))
        .route("/api/messages/v2/delete", post(delete_message))
        .route("/api/messages/v2/markread", post(mark_read))
        .route("/matchmaking/player/login", post(mm_login))
        .route("/matchmaking/player/logout", post(mm_logout))
        .route("/matchmaking/player/heartbeat", post(heartbeat).get(heartbeat))
        .route("/matchmaking/player", get(presence_bulk))
}

async fn list_relationships(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    Ok(Json(app.store.read(|d| Value::Array(related(d, me).into_iter().map(|o| relationship(d, me, o)).collect()))))
}

async fn act(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes, what: Act) -> Response {
    let me = match who(&app, &h) {
        Ok(a) => a.account_id,
        Err(r) => return r,
    };
    let Some(other) = Params::new(&q, &body).player() else {
        return fail(StatusCode::BAD_REQUEST, "no player id");
    };
    if other == me {
        return fail(StatusCode::BAD_REQUEST, "that is you");
    }
    if app.store.get(other).is_none() {
        return fail(StatusCode::NOT_FOUND, "no such player");
    }
    let since = now();
    let edge = |from, to, kind| Relation { from, to, kind, since: since.clone() };
    // (status, error, notification for `other`)
    let outcome: Result<Option<Value>, (StatusCode, &str)> = app.store.write(|d| match what {
        Act::Request | Act::Accept if blocked(d, me, other) => Err((StatusCode::FORBIDDEN, "blocked")),
        Act::Request | Act::Accept if friends(d, me, other) => Ok(None),
        Act::Request | Act::Accept if has(d, other, me, RelKind::Request) => {
            unlink(d, me, other, &[RelKind::Request]);
            d.relations.extend([edge(me, other, RelKind::Friend), edge(other, me, RelKind::Friend)]);
            Ok(Some(notify(d, other, me, message_type::FRIEND_REQUEST_ACCEPTED, "friendAccepted", "", None)))
        }
        Act::Accept => Err((StatusCode::NOT_FOUND, "no friend request from that player")),
        Act::Request if has(d, me, other, RelKind::Request) => Ok(None),
        Act::Request => {
            d.relations.push(edge(me, other, RelKind::Request));
            Ok(Some(notify(d, other, me, message_type::FRIEND_INVITE, "friendRequest", "", None)))
        }
        Act::Decline if has(d, other, me, RelKind::Request) => {
            unlink(d, me, other, &[RelKind::Request]);
            Ok(None)
        }
        Act::Decline => Err((StatusCode::NOT_FOUND, "no friend request from that player")),
        Act::Remove => {
            unlink(d, me, other, &[RelKind::Request, RelKind::Friend]);
            Ok(None)
        }
        Act::Block => {
            unlink(d, me, other, &[RelKind::Request, RelKind::Friend]);
            if !has(d, me, other, RelKind::Block) {
                d.relations.push(edge(me, other, RelKind::Block));
            }
            Ok(None)
        }
        Act::Unblock => {
            d.relations.retain(|r| !(r.from == me && r.to == other && r.kind == RelKind::Block));
            Ok(None)
        }
    });
    match outcome {
        Err((st, why)) => fail(st, why),
        Ok(message) => {
            let (mine, theirs) = app.store.read(|d| (relationship(d, me, other), relationship(d, other, me)));
            push(&app, me, "RelationshipChanged", mine.clone());
            push(&app, other, "RelationshipChanged", theirs);
            if let Some(m) = message {
                push(&app, other, "MessageReceived", m);
            }
            if matches!(what, Act::Block) {
                crate::party::on_block(&app, me, other);
            }
            Json(mine).into_response()
        }
    }
}

/// Ours, for the client's friends page: one row per relationship with names and where friends are.
async fn friend_list(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    let rows: Vec<(i64, Value, Option<Account>)> =
        app.store.read(|d| related(d, me).into_iter().map(|o| (o, relationship(d, me, o), d.accounts.iter().find(|a| a.account_id == o).cloned())).collect());
    let live = app.live.lock();
    let my_party = live.party_of(me).map(|p| p.id);
    let list = rows
        .into_iter()
        .filter_map(|(o, rel, acc)| {
            let acc = acc?;
            let is_friend = rel["RelationshipType"] == 3;
            let online = is_friend && live.online(o);
            let instance = if online { live.presence.get(&o).and_then(|p| p.instance.clone()) } else { None };
            let kind = match (rel["Ignored"] != 0, rel["RelationshipType"].as_i64()) {
                (true, _) => "blocked",
                (_, Some(3)) => "friend",
                (_, Some(1)) => "requestSent",
                _ => "requestReceived",
            };
            Some(json!({
                "accountId": o, "username": acc.username, "displayName": acc.display_name, "relationship": kind,
                "isOnline": online, "roomInstance": instance,
                "inMyParty": my_party.is_some() && live.party_of(o).map(|p| p.id) == my_party,
            }))
        })
        .collect();
    Ok(Json(Value::Array(list)))
}

// ---- notifications ----

async fn list_messages(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    Ok(Json(app.store.read(|d| Value::Array(d.messages.iter().filter(|m| m.to == me).map(message_json).collect()))))
}

async fn delete_message(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes) -> Result<StatusCode, Response> {
    let me = who(&app, &h)?.account_id;
    let id = Params::new(&q, &body).id(&["id", "messageId"]).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "no message id"))?;
    let gone = app.store.write(|d| {
        let n = d.messages.len();
        d.messages.retain(|m| !(m.to == me && m.id == id));
        n != d.messages.len()
    });
    if gone {
        push(&app, me, "MessageDeleted", json!({ "Id": id }));
    }
    Ok(if gone { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

/// Ours: marks one message (`id`) or all of them read.
async fn mark_read(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes) -> Result<StatusCode, Response> {
    let me = who(&app, &h)?.account_id;
    let id = Params::new(&q, &body).id(&["id", "messageId"]);
    app.store.write(|d| d.messages.iter_mut().filter(|m| m.to == me && id.is_none_or(|i| m.id == i)).for_each(|m| m.read = true));
    Ok(StatusCode::OK)
}

// ---- presence ----

/// The game's `PlayerPresence` for `id` as `viewer` may see it: players who blocked the viewer look offline, and only
/// friends and party mates see the room instance.
pub fn presence_json(app: &App, viewer: i64, id: i64) -> Value {
    let (block, friend) = app.store.read(|d| (blocked(d, viewer, id), friends(d, viewer, id)));
    let live = app.live.lock();
    let online = !block && live.online(id);
    let mate = live.party_of(id).is_some_and(|p| p.members.contains(&viewer));
    let instance = if online && (viewer == id || friend || mate) { live.presence.get(&id).and_then(|p| p.instance.clone()) } else { None };
    json!({
        "PlayerId": id, "StatusVisibility": 0, "Platform": 0, "DeviceClass": 0, "VrMovementMode": 0,
        "RoomInstance": instance, "CorrelationId": null, "IsOnline": online, "AppVersion": "", "ClientJoinData": null,
    })
}

/// Tells `id`'s friends where `id` is now.
pub fn announce(app: &App, id: i64) {
    let friends: Vec<i64> = app.store.read(|d| d.relations.iter().filter(|r| r.from == id && r.kind == RelKind::Friend).map(|r| r.to).collect());
    for f in friends {
        push(app, f, "SubscriptionUpdatePresence", presence_json(app, f, id));
    }
}

/// Records that `id` is in `instance` (or nowhere) and tells its friends.
pub fn set_instance(app: &App, id: i64, instance: Option<Value>) {
    {
        let mut live = app.live.lock();
        let p = live.presence.entry(id).or_default();
        p.seen = Some(Instant::now());
        p.instance = instance;
    }
    announce(app, id);
}

async fn mm_login(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let a = who(&app, &h)?;
    println!("  {} is online", a.username);
    app.live.lock().presence.entry(a.account_id).or_default().seen = Some(Instant::now());
    announce(&app, a.account_id);
    Ok(Json(presence_json(&app, a.account_id, a.account_id)))
}

async fn mm_logout(State(app): State<Shared>, h: HeaderMap) -> Result<StatusCode, Response> {
    let a = who(&app, &h)?;
    crate::party::leave(&app, a.account_id);
    app.live.lock().presence.remove(&a.account_id);
    announce(&app, a.account_id);
    Ok(StatusCode::OK)
}

async fn heartbeat(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let a = who(&app, &h)?;
    app.live.lock().presence.entry(a.account_id).or_default().seen = Some(Instant::now());
    Ok(Json(presence_json(&app, a.account_id, a.account_id)))
}

/// `GET matchmaking/player?id=1&id=2`: presences, as the caller may see them.
async fn presence_bulk(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    let ids: Vec<i64> = serde_urlencoded::from_str::<Vec<(String, String)>>(q.as_deref().unwrap_or(""))
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("id"))
        .filter_map(|(_, v)| v.parse().ok())
        .collect();
    Ok(Json(Value::Array(ids.into_iter().filter(|&i| app.store.get(i).is_some()).map(|i| presence_json(&app, me, i)).collect())))
}
