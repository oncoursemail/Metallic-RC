//! Parties and following friends. Parties live in memory only (a restart ends them, as a disconnect would).
//!
//! - `GET api/party/v1/me` -> the caller's party or null; `POST api/party/v1/create`, `invite?id=<player>`,
//!   `join?id=<party>` (needs an invite), `leave`. Party JSON: `{partyId, leaderId, members, invited, roomInstance}`.
//!   Every change pushes our own `PartyChanged` notification (Msg = the party, or null to whoever left).
//! - When the leader goes to a room (any matchmaking goto), the party's instance becomes that one and each member gets
//!   a `MessageReceived` push of type `PartyActivitySwitchV2` whose `Data` is the instance JSON; a member who then goes
//!   to the same room (or joins the leader) is put in that instance, even a private dorm.
//! - `POST matchmaking/goto/player/{id}` (also the 2026 path `matchmaking/matchmake/v2/player/{id}`): join a friend
//!   or party mate where they are. Refused with the game's `MatchmakingErrorCode`s: AccountDoesNotExist 85,
//!   BlockedFromRoom 7 (either one blocked the other), InsufficientRelationship 14 (not friends or party mates),
//!   PlayerNotOnline 2, RoomInstanceIsPrivate 26 (a private instance, unless party mates).
//!   INFERRED: the party paths and shapes are ours; the game's own party service was not traced.
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
    social::{self, blocked, fail, friends, message_type, push, who, Params, Party},
    store::{now, Account},
    App, Shared,
};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/party/v1/me", get(mine))
        .route("/api/party/v1/create", post(create))
        .route("/api/party/v1/invite", post(invite))
        .route("/api/party/v1/join", post(join))
        .route("/api/party/v1/leave", post(leave_route))
        .route("/matchmaking/goto/player/{id}", post(join_player))
        .route("/matchmaking/matchmake/v2/player/{id}", post(join_player))
}

pub fn party_json(p: &Party) -> Value {
    json!({ "partyId": p.id, "leaderId": p.leader, "members": p.members, "invited": p.invited, "roomInstance": p.instance })
}

/// Pushes the party as it is now to its members (and `PartyChanged: null` to `gone`, who just left it).
fn changed(app: &App, party: Option<Party>, gone: Option<i64>) {
    if let Some(p) = &party {
        for &m in &p.members {
            push(app, m, "PartyChanged", party_json(p));
        }
    }
    if let Some(g) = gone {
        push(app, g, "PartyChanged", Value::Null);
    }
}

/// Takes `id` out of its party; the next member leads if the leader left, an empty party ends.
pub fn leave(app: &App, id: i64) {
    let after = {
        let mut live = app.live.lock();
        let Some(i) = live.parties.iter().position(|p| p.members.contains(&id)) else { return };
        let p = &mut live.parties[i];
        p.members.retain(|&m| m != id);
        if p.members.is_empty() {
            live.parties.remove(i);
            None
        } else {
            if p.leader == id {
                p.leader = p.members[0];
            }
            Some(p.clone())
        }
    };
    changed(app, after, Some(id));
}

/// After `me` blocked `other`: a shared party loses the blocker, and `other`'s invite to the blocker's party goes.
pub fn on_block(app: &App, me: i64, other: i64) {
    let shared = {
        let mut live = app.live.lock();
        for p in live.parties.iter_mut().filter(|p| p.members.contains(&me)) {
            p.invited.retain(|&i| i != other);
        }
        live.party_of(me).is_some_and(|p| p.members.contains(&other))
    };
    if shared {
        leave(app, me);
    }
}

/// `me` is now in `instance`: presence is updated and, if `me` leads a party, the members are told to follow.
pub fn arrive(app: &App, me: i64, instance: Value) {
    social::set_instance(app, me, Some(instance.clone()));
    let followers = {
        let mut live = app.live.lock();
        match live.parties.iter_mut().find(|p| p.leader == me) {
            Some(p) => {
                p.instance = Some(instance.clone());
                p.members.iter().copied().filter(|&m| m != me).collect()
            }
            None => vec![],
        }
    };
    let follow = json!({
        "Id": 0, "FromPlayerId": me, "SentTime": now(), "Type": message_type::PARTY_ACTIVITY_SWITCH_V2,
        "Data": instance.to_string(), "RoomId": instance["roomId"], "PlayerEventId": null, "Kind": "partyGoto", "Read": false,
    });
    for m in followers {
        push(app, m, "MessageReceived", follow.clone());
    }
}

/// Matchmaking's hook: `a` is going to `room_id` and would get `instance`. A party member going where the party is
/// gets the party's instance instead. Records where `a` ended up (and moves `a`'s party along) and returns it.
pub fn placed(app: &App, a: &Account, room_id: i64, instance: Value) -> Value {
    let instance = {
        let live = app.live.lock();
        match live.party_of(a.account_id) {
            Some(p) if p.leader != a.account_id => p.instance.clone().filter(|i| i["roomId"] == room_id).unwrap_or(instance),
            _ => instance,
        }
    };
    arrive(app, a.account_id, instance.clone());
    instance
}

async fn mine(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    Ok(Json(app.live.lock().party_of(me).map_or(Value::Null, party_json)))
}

/// The caller's party, made (with the caller leading, where the caller is now) if there is none.
fn ensure_party(app: &App, me: i64) -> Party {
    let mut live = app.live.lock();
    if let Some(p) = live.party_of(me) {
        return p.clone();
    }
    live.next_party += 1;
    let instance = live.presence.get(&me).and_then(|p| p.instance.clone());
    let p = Party { id: live.next_party, leader: me, members: vec![me], invited: vec![], instance };
    live.parties.push(p.clone());
    p
}

async fn create(State(app): State<Shared>, h: HeaderMap) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    let p = ensure_party(&app, me);
    changed(&app, Some(p.clone()), None);
    Ok(Json(party_json(&p)))
}

async fn invite(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    let other = Params::new(&q, &body).player().ok_or_else(|| fail(StatusCode::BAD_REQUEST, "no player id"))?;
    if other == me || app.store.get(other).is_none() {
        return Err(fail(StatusCode::NOT_FOUND, "no such player"));
    }
    if app.store.read(|d| blocked(d, me, other)) {
        return Err(fail(StatusCode::FORBIDDEN, "blocked"));
    }
    let p = ensure_party(&app, me);
    if p.members.contains(&other) {
        return Ok(Json(party_json(&p)));
    }
    let p = {
        let mut live = app.live.lock();
        let Some(p) = live.parties.iter_mut().find(|x| x.id == p.id) else { return Err(fail(StatusCode::CONFLICT, "party ended")) };
        if !p.invited.contains(&other) {
            p.invited.push(other);
        }
        p.clone()
    };
    let room = p.instance.as_ref().and_then(|i| i["roomId"].as_i64());
    let m = app.store.write(|d| social::notify(d, other, me, message_type::PARTY_INVITE, "partyInvite", &p.id.to_string(), room));
    push(&app, other, "MessageReceived", m);
    changed(&app, Some(p.clone()), None);
    Ok(Json(party_json(&p)))
}

async fn join(State(app): State<Shared>, h: HeaderMap, RawQuery(q): RawQuery, body: Bytes) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    let id = Params::new(&q, &body).id(&["id", "partyId"]).ok_or_else(|| fail(StatusCode::BAD_REQUEST, "no party id"))?;
    let Some(p) = app.live.lock().parties.iter().find(|p| p.id == id).cloned() else {
        return Err(fail(StatusCode::NOT_FOUND, "no such party"));
    };
    if p.members.contains(&me) {
        return Ok(Json(party_json(&p)));
    }
    if !p.invited.contains(&me) {
        return Err(fail(StatusCode::FORBIDDEN, "not invited"));
    }
    if app.store.read(|d| p.members.iter().any(|&m| blocked(d, me, m))) {
        return Err(fail(StatusCode::FORBIDDEN, "blocked"));
    }
    leave(&app, me);
    let p = {
        let mut live = app.live.lock();
        let Some(p) = live.parties.iter_mut().find(|x| x.id == id) else { return Err(fail(StatusCode::NOT_FOUND, "no such party")) };
        p.invited.retain(|&i| i != me);
        p.members.push(me);
        p.clone()
    };
    // the invite is used up
    app.store.write(|d| d.messages.retain(|m| !(m.to == me && m.kind == "partyInvite" && m.data == id.to_string())));
    changed(&app, Some(p.clone()), None);
    Ok(Json(party_json(&p)))
}

async fn leave_route(State(app): State<Shared>, h: HeaderMap) -> Result<StatusCode, Response> {
    leave(&app, who(&app, &h)?.account_id);
    Ok(StatusCode::OK)
}

fn refused(code: i32, name: &str) -> Json<Value> {
    Json(json!({ "errorCode": code, "error": name, "roomInstance": null }))
}

async fn join_player(State(app): State<Shared>, h: HeaderMap, Path(id): Path<i64>) -> Result<Json<Value>, Response> {
    let me = who(&app, &h)?.account_id;
    if app.store.get(id).is_none() {
        return Ok(refused(85, "AccountDoesNotExist"));
    }
    let (block, friend) = app.store.read(|d| (blocked(d, me, id), friends(d, me, id)));
    if block {
        return Ok(refused(7, "BlockedFromRoom"));
    }
    let (mate, instance) = {
        let live = app.live.lock();
        let mate = live.party_of(id).is_some_and(|p| p.members.contains(&me));
        (mate, if live.online(id) { live.presence.get(&id).and_then(|p| p.instance.clone()) } else { None })
    };
    if !friend && !mate {
        return Ok(refused(14, "InsufficientRelationship"));
    }
    let Some(instance) = instance else { return Ok(refused(2, "PlayerNotOnline")) };
    if instance["isPrivate"] == true && !mate {
        return Ok(refused(26, "RoomInstanceIsPrivate"));
    }
    arrive(&app, me, instance.clone());
    Ok(Json(json!({ "errorCode": 0, "roomInstance": instance })))
}
