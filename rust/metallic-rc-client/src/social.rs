//! The client's social side: friends, profile, notifications, party and inventory, kept current from the server.
//!
//! Two layers:
//! - [`Client`]: blocking calls to metallic-rc-server (used by the worker thread, tests and scripts).
//! - [`SocialPlugin`]: after a login, [`attach`] starts a worker thread that fetches everything, keeps the
//!   notifications hub open (pushes trigger re-fetches), sends a matchmaking heartbeat every 30 s and runs the calls
//!   the game queues through the [`Social`] resource. Each frame the plugin copies what changed into the resources
//!   below and sends the messages below.
//!
//! For the UI pages (read these; they are replaced whole when something changes):
//! - [`Friends`]: every relationship (friends, requests both ways, players you blocked) with online state and room.
//! - [`MyProfile`]: your profile (display name, bio, look, created date, level, XP); None until fetched.
//! - [`Notifications`]: stored notifications, newest last (friend requests, accepted requests, party invites).
//! - [`MyParty`]: your party, None when not in one.
//! - [`Inventory`]: items you own (server-granted).
//! - [`SocialStatus`]: whether the worker is connected, and the last error (for a toast).
//!
//! Messages (Bevy `MessageReader`):
//! - [`SocialNotice`]: a new notification just arrived (show a toast / badge).
//! - [`FollowRoom`]: the server put this player in another room instance (joined a friend, or the party leader moved
//!   the party). The game should switch its realtime room to `room.photon_room`. Not wired into main.rs yet.
//! - [`SocialError`]: a call failed (the text says why: "blocked", "not invited", "RoomInstanceIsPrivate", ...).
//!
//! Calls (methods on [`Social`], they return at once; results arrive through the resources and messages):
//! `send_request`, `accept`, `decline`, `remove_friend`, `block`, `unblock`, `invite_to_party`, `join_party`,
//! `leave_party`, `join_friend`, `update_profile`, `mark_read`, `delete_notification`, `refresh`.
//!
//! The look: when the server has a saved look it replaces [`LocalLook`] after login (so a second PC shows the same
//! robot); a look changed in the look panel is saved to the server.
use std::{
    net::TcpStream,
    sync::{
        mpsc::{self, RecvTimeoutError, Sender},
        Arc, LazyLock, Mutex,
    },
    time::Duration,
};

use bevy::prelude::*;
use serde_json::{json, Value};
use tungstenite::{stream::MaybeTlsStream, Message as WsMessage, WebSocket};

use crate::{
    avatar::{LocalLook, Look},
    net::{Error, Room, Session},
};

// ---- data ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Friend,
    /// you asked them
    RequestSent,
    /// they asked you: `accept` or `decline`
    RequestReceived,
    /// you blocked them
    Blocked,
}

#[derive(Clone, Debug)]
pub struct Friend {
    pub account_id: i64,
    pub username: String,
    pub display_name: String,
    pub relation: Relation,
    /// only for friends
    pub online: bool,
    /// where a friend is (None when offline); `room.private` = a private instance (only party mates can join)
    pub room: Option<Room>,
    pub in_my_party: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub account_id: i64,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    /// None until a look was saved on the server
    pub look: Option<Look>,
    /// RFC 3339
    pub created_at: String,
    pub level: i64,
    pub xp: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NoticeKind {
    FriendRequest,
    FriendAccepted,
    PartyInvite { party_id: i64 },
    Other(String),
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub id: i64,
    /// account id of the sender
    pub from: i64,
    pub kind: NoticeKind,
    /// RFC 3339
    pub sent: String,
    pub read: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Party {
    pub id: i64,
    pub leader: i64,
    pub members: Vec<i64>,
    pub invited: Vec<i64>,
    /// where the leader last went
    pub room: Option<Room>,
}

#[derive(Clone, Debug)]
pub struct Item {
    /// "<category>:<name>", e.g. "accessory:halo"
    pub item_id: String,
    pub category: String,
    pub granted_at: String,
    pub source: String,
}

/// Fields left None are not changed.
#[derive(Clone, Debug, Default)]
pub struct ProfileUpdate {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub look: Option<Look>,
}

fn room_of(i: &Value) -> Option<Room> {
    i.is_object().then(|| Room {
        name: i["name"].as_str().unwrap_or("").trim_start_matches('^').to_string(),
        room_id: i["roomId"].as_i64().unwrap_or(0),
        instance_id: i["roomInstanceId"].as_i64().unwrap_or(0),
        photon_room: i["photonRoomId"].as_str().unwrap_or("").to_string(),
        private: i["isPrivate"].as_bool().unwrap_or(false),
    })
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn profile_of(v: &Value) -> Profile {
    Profile {
        account_id: v["accountId"].as_i64().unwrap_or(0),
        username: text(&v["username"]),
        display_name: text(&v["displayName"]),
        bio: text(&v["bio"]),
        look: v["look"].is_object().then(|| Look::from_json(&v["look"].to_string())).flatten(),
        created_at: text(&v["createdAt"]),
        level: v["level"].as_i64().unwrap_or(1),
        xp: v["xp"].as_i64().unwrap_or(0),
    }
}

fn notification_of(v: &Value) -> Notification {
    let kind = match v["Kind"].as_str().unwrap_or("") {
        "friendRequest" => NoticeKind::FriendRequest,
        "friendAccepted" => NoticeKind::FriendAccepted,
        "partyInvite" => NoticeKind::PartyInvite { party_id: v["Data"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0) },
        k => NoticeKind::Other(k.to_string()),
    };
    Notification {
        id: v["Id"].as_i64().unwrap_or(0),
        from: v["FromPlayerId"].as_i64().unwrap_or(0),
        kind,
        sent: text(&v["SentTime"]),
        read: v["Read"].as_bool().unwrap_or(false),
    }
}

fn party_of(v: &Value) -> Option<Party> {
    let ids = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    v.is_object().then(|| Party {
        id: v["partyId"].as_i64().unwrap_or(0),
        leader: v["leaderId"].as_i64().unwrap_or(0),
        members: ids("members"),
        invited: ids("invited"),
        room: room_of(&v["roomInstance"]),
    })
}

// ---- blocking client ----

fn err(what: &str, e: impl std::fmt::Display) -> Error {
    Error(format!("{what}: {e}"))
}

/// Calls to the server's social endpoints for one logged-in account.
pub struct Client {
    agent: ureq::Agent,
    api: String,
    accounts: String,
    matchmaking: String,
    notifications: String,
    token: String,
    pub me: i64,
}

impl Client {
    pub fn new(session: &Session) -> Result<Client, Error> {
        let svc = |n: &str| session.service(n).map(str::to_string).ok_or_else(|| Error(format!("no {n} service")));
        Ok(Client {
            agent: ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).http_status_as_error(false).build().into(),
            api: svc("API")?,
            accounts: svc("Accounts")?,
            matchmaking: svc("Matchmaking")?,
            notifications: svc("Notifications")?,
            token: session.access_token.clone(),
            me: session.account.id,
        })
    }

    /// One call; a JSON body is sent as JSON. Errors carry the server's `error` text when it gives one.
    fn call(&self, method: &str, url: String, body: Option<Value>) -> Result<Value, Error> {
        let auth = format!("Bearer {}", self.token);
        let res = match (method, body) {
            ("GET", _) => self.agent.get(&url).header("Authorization", &auth).call(),
            (_, Some(b)) => self.agent.post(&url).header("Authorization", &auth).send_json(b),
            _ => self.agent.post(&url).header("Authorization", &auth).send_empty(),
        };
        let mut r = res.map_err(|e| err(&url, e))?;
        let status = r.status().as_u16();
        let text = r.body_mut().read_to_string().map_err(|e| err(&url, e))?;
        let v = if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::String(text)) };
        if status >= 400 {
            return Err(Error(v["error"].as_str().map_or_else(|| format!("{url}: HTTP {status}"), str::to_string)));
        }
        Ok(v)
    }

    pub fn profile(&self, id: Option<i64>) -> Result<Profile, Error> {
        let who = id.map_or("me".to_string(), |i| i.to_string());
        Ok(profile_of(&self.call("GET", format!("{}/profile/v1/{who}", self.api), None)?))
    }

    pub fn update_profile(&self, u: &ProfileUpdate) -> Result<Profile, Error> {
        let mut b = json!({});
        if let Some(n) = &u.display_name {
            b["displayName"] = json!(n);
        }
        if let Some(t) = &u.bio {
            b["bio"] = json!(t);
        }
        if let Some(l) = &u.look {
            b["look"] = serde_json::from_str(&l.to_json()).unwrap_or(Value::Null);
        }
        Ok(profile_of(&self.call("POST", format!("{}/profile/v1/me", self.api), Some(b))?))
    }

    pub fn friends(&self) -> Result<Vec<Friend>, Error> {
        let v = self.call("GET", format!("{}/social/v1/friends", self.api), None)?;
        Ok(v.as_array()
            .into_iter()
            .flatten()
            .map(|f| Friend {
                account_id: f["accountId"].as_i64().unwrap_or(0),
                username: text(&f["username"]),
                display_name: text(&f["displayName"]),
                relation: match f["relationship"].as_str() {
                    Some("friend") => Relation::Friend,
                    Some("requestSent") => Relation::RequestSent,
                    Some("blocked") => Relation::Blocked,
                    _ => Relation::RequestReceived,
                },
                online: f["isOnline"].as_bool().unwrap_or(false),
                room: room_of(&f["roomInstance"]),
                in_my_party: f["inMyParty"].as_bool().unwrap_or(false),
            })
            .collect())
    }

    /// `what`: sendfriendrequest, acceptfriendrequest, declinefriendrequest, removefriend, ignore (block), unignore.
    pub fn relationship(&self, what: &str, id: i64) -> Result<(), Error> {
        self.call("POST", format!("{}/relationships/v2/{what}?id={id}", self.api), None).map(|_| ())
    }

    pub fn notifications(&self) -> Result<Vec<Notification>, Error> {
        let v = self.call("GET", format!("{}/messages/v2/get", self.api), None)?;
        Ok(v.as_array().into_iter().flatten().map(notification_of).collect())
    }

    pub fn mark_read(&self, id: Option<i64>) -> Result<(), Error> {
        let q = id.map_or(String::new(), |i| format!("?id={i}"));
        self.call("POST", format!("{}/messages/v2/markread{q}", self.api), None).map(|_| ())
    }

    pub fn delete_notification(&self, id: i64) -> Result<(), Error> {
        self.call("POST", format!("{}/messages/v2/delete?id={id}", self.api), None).map(|_| ())
    }

    pub fn party(&self) -> Result<Option<Party>, Error> {
        Ok(party_of(&self.call("GET", format!("{}/party/v1/me", self.api), None)?))
    }

    pub fn invite_to_party(&self, id: i64) -> Result<Option<Party>, Error> {
        Ok(party_of(&self.call("POST", format!("{}/party/v1/invite?id={id}", self.api), None)?))
    }

    pub fn join_party(&self, party_id: i64) -> Result<Option<Party>, Error> {
        Ok(party_of(&self.call("POST", format!("{}/party/v1/join?id={party_id}", self.api), None)?))
    }

    pub fn leave_party(&self) -> Result<(), Error> {
        self.call("POST", format!("{}/party/v1/leave", self.api), None).map(|_| ())
    }

    /// Goes where a friend (or party mate) is; the room instance to join, or why not (the matchmaking error name).
    pub fn join_friend(&self, id: i64) -> Result<Room, Error> {
        let g = self.call("POST", format!("{}/goto/player/{id}", self.matchmaking), None)?;
        if g["errorCode"].as_i64() != Some(0) {
            return Err(Error(g["error"].as_str().map_or_else(|| format!("matchmaking error {}", g["errorCode"]), str::to_string)));
        }
        room_of(&g["roomInstance"]).ok_or_else(|| Error(format!("no room instance in {g}")))
    }

    pub fn inventory(&self) -> Result<Vec<Item>, Error> {
        let v = self.call("GET", format!("{}/inventory/v1/me", self.api), None)?;
        Ok(v.as_array()
            .into_iter()
            .flatten()
            .map(|i| Item { item_id: text(&i["itemId"]), category: text(&i["category"]), granted_at: text(&i["grantedAt"]), source: text(&i["source"]) })
            .collect())
    }

    pub fn bio(&self, id: i64) -> Result<String, Error> {
        Ok(text(&self.call("GET", format!("{}/account/{id}/bio", self.accounts), None)?["bio"]))
    }

    pub fn heartbeat(&self) -> Result<(), Error> {
        self.call("POST", format!("{}/player/heartbeat", self.matchmaking), None).map(|_| ())
    }

    /// Opens the notifications hub as this account; read pushes with [`Pushes::next`].
    pub fn pushes(&self) -> Result<Pushes, Error> {
        let base = format!("{}/hub/v1", self.notifications);
        let n = self.call("POST", format!("{base}/negotiate?negotiateVersion=1"), None)?;
        let token = n["connectionToken"].as_str().ok_or_else(|| Error(format!("hub negotiate: no token in {n}")))?;
        let url = format!("{}?id={token}", base.replacen("http", "ws", 1));
        let (mut ws, _) = tungstenite::connect(&url).map_err(|e| err(&url, e))?;
        ws.send(WsMessage::text("{\"protocol\":\"json\",\"version\":1}\u{1e}")).map_err(|e| err("hub handshake", e))?;
        let reply = ws.read().map_err(|e| err("hub handshake", e))?;
        if reply.to_text().unwrap_or("").trim_end_matches('\u{1e}') != "{}" {
            return Err(Error(format!("hub handshake refused: {reply}")));
        }
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
        }
        Ok(Pushes { ws, queue: Default::default() })
    }
}

/// An open notifications hub that only listens.
pub struct Pushes {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    queue: std::collections::VecDeque<(String, Value)>,
}

impl Pushes {
    /// The next push as `(Id, Msg)`; Ok(None) after about a second without one; Err when the hub closed.
    pub fn next(&mut self) -> Result<Option<(String, Value)>, Error> {
        if let Some(p) = self.queue.pop_front() {
            return Ok(Some(p));
        }
        let m = match self.ws.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(None),
            Err(e) => return Err(err("hub", e)),
        };
        for frame in m.to_text().unwrap_or("").split('\u{1e}').filter(|f| !f.is_empty()) {
            let v: Value = serde_json::from_str(frame).unwrap_or(Value::Null);
            if v["type"] == 1 && v["target"] == "Notification" {
                let n = &v["arguments"][0];
                let id = n["Id"].as_str().map_or_else(|| n["Id"].to_string(), str::to_string);
                self.queue.push_back((id, n["Msg"].clone()));
            } else if v["type"] == 6 {
                let _ = self.ws.send(WsMessage::text("{\"type\":6}\u{1e}"));
            } else if v["type"] == 7 {
                return Err(Error("hub closed by the server".into()));
            }
        }
        Ok(self.queue.pop_front())
    }
}

// ---- the worker and the Bevy side ----

/// What the game asks the worker to do.
#[derive(Clone, Debug)]
enum Cmd {
    Relationship(&'static str, i64),
    Invite(i64),
    JoinParty(i64),
    LeaveParty,
    JoinFriend(i64),
    UpdateProfile(ProfileUpdate),
    MarkRead(Option<i64>),
    Delete(i64),
    Refresh,
    Push(String, Value),
}

#[derive(Clone, Debug)]
enum Event {
    Notice(Notification),
    Follow(Room),
    Error(String),
    /// the look saved on the server, after login
    Look(Look),
}

#[derive(Default)]
struct Snapshot {
    friends: Vec<Friend>,
    profile: Option<Profile>,
    notifications: Vec<Notification>,
    party: Option<Party>,
    inventory: Vec<Item>,
    connected: bool,
    last_error: Option<String>,
}

/// Shared between the worker and the plugin. `login` changes with every attach; a worker of an older login stops.
#[derive(Default)]
struct Shared {
    login: u64,
    version: u64,
    tx: Option<Sender<Cmd>>,
    data: Snapshot,
    events: Vec<Event>,
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(Default::default);

fn update(login: u64, f: impl FnOnce(&mut Shared)) -> bool {
    let mut s = SHARED.lock().unwrap();
    if s.login != login {
        return false;
    }
    f(&mut s);
    s.version += 1;
    true
}

/// Starts the social worker for a fresh login (call it from the login thread with the new session). A previous
/// login's worker stops.
pub fn attach(session: &Session) {
    let client = match Client::new(session) {
        Ok(c) => c,
        Err(e) => return eprintln!("social: {e}"),
    };
    let (tx, rx) = mpsc::channel();
    let login = {
        let mut s = SHARED.lock().unwrap();
        s.login += 1;
        s.tx = Some(tx.clone());
        s.data = Snapshot::default();
        s.version += 1;
        s.login
    };
    let client = Arc::new(client);
    // the hub: pushes become commands for the worker
    let (hub_client, hub_tx) = (client.clone(), tx);
    std::thread::spawn(move || {
        let mut pushes = match hub_client.pushes() {
            Ok(p) => p,
            Err(e) => return eprintln!("social: hub: {e}"),
        };
        while SHARED.lock().unwrap().login == login {
            match pushes.next() {
                Ok(Some((id, msg))) => {
                    if hub_tx.send(Cmd::Push(id, msg)).is_err() {
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => return eprintln!("social: hub: {e}"),
            }
        }
    });
    std::thread::spawn(move || worker(login, &client, rx));
}

/// Stops the worker (logging out or switching accounts without a new session yet).
pub fn detach() {
    let mut s = SHARED.lock().unwrap();
    s.login += 1;
    s.tx = None;
    s.data = Snapshot::default();
    s.version += 1;
}

fn worker(login: u64, c: &Client, rx: mpsc::Receiver<Cmd>) {
    let fail = |e: Error| {
        update(login, |s| {
            s.data.last_error = Some(e.0.clone());
            s.events.push(Event::Error(e.0));
        });
    };
    let all = |first: bool| -> Result<(), Error> {
        let (profile, friends, notifications, party, inventory) = (c.profile(None)?, c.friends()?, c.notifications()?, c.party()?, c.inventory()?);
        update(login, |s| {
            if first {
                if let Some(l) = profile.look {
                    s.events.push(Event::Look(l));
                }
            }
            s.data = Snapshot { friends, profile: Some(profile), notifications, party, inventory, connected: true, last_error: None };
        });
        Ok(())
    };
    if let Err(e) = all(true) {
        return fail(e);
    }
    loop {
        if SHARED.lock().unwrap().login != login {
            return;
        }
        let cmd = match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => {
                if let Err(e) = c.heartbeat() {
                    fail(e);
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Err(e) = run(login, c, cmd) {
            fail(e);
        }
    }
}

fn run(login: u64, c: &Client, cmd: Cmd) -> Result<(), Error> {
    let friends = || -> Result<(), Error> {
        let f = c.friends()?;
        update(login, |s| s.data.friends = f);
        Ok(())
    };
    let notifications = || -> Result<(), Error> {
        let n = c.notifications()?;
        update(login, |s| s.data.notifications = n);
        Ok(())
    };
    let party = || -> Result<(), Error> {
        let p = c.party()?;
        update(login, |s| s.data.party = p);
        Ok(())
    };
    let follow = |room: Room| {
        update(login, |s| s.events.push(Event::Follow(room)));
    };
    match cmd {
        Cmd::Relationship(what, id) => {
            c.relationship(what, id)?;
            friends()?;
            notifications()?;
        }
        Cmd::Invite(id) => {
            let p = c.invite_to_party(id)?;
            update(login, |s| s.data.party = p);
        }
        Cmd::JoinParty(id) => {
            let p = c.join_party(id)?;
            notifications()?;
            let leader = p.as_ref().filter(|p| p.room.is_some()).map(|p| p.leader);
            update(login, |s| s.data.party = p);
            // go where the party is
            if let Some(l) = leader.filter(|&l| l != c.me) {
                follow(c.join_friend(l)?);
            }
        }
        Cmd::LeaveParty => {
            c.leave_party()?;
            party()?;
        }
        Cmd::JoinFriend(id) => {
            follow(c.join_friend(id)?);
            friends()?;
        }
        Cmd::UpdateProfile(u) => {
            let p = c.update_profile(&u)?;
            update(login, |s| s.data.profile = Some(p));
        }
        Cmd::MarkRead(id) => {
            c.mark_read(id)?;
            notifications()?;
        }
        Cmd::Delete(id) => {
            c.delete_notification(id)?;
            notifications()?;
        }
        Cmd::Refresh => {
            let (p, f, n, pa, i) = (c.profile(None)?, c.friends()?, c.notifications()?, c.party()?, c.inventory()?);
            update(login, |s| (s.data.profile, s.data.friends, s.data.notifications, s.data.party, s.data.inventory) = (Some(p), f, n, pa, i));
        }
        Cmd::Push(id, msg) => match id.as_str() {
            "RelationshipChanged" | "SubscriptionUpdatePresence" | "SubscriptionUpdateProfile" => friends()?,
            "MessageDeleted" => notifications()?,
            "PartyChanged" => {
                let p = party_of(&msg);
                update(login, |s| s.data.party = p);
                friends()?;
            }
            "MessageReceived" if msg["Kind"] == "partyGoto" => {
                // the leader moved the party: follow
                if let Some(leader) = msg["FromPlayerId"].as_i64() {
                    follow(c.join_friend(leader)?);
                }
            }
            "MessageReceived" => {
                let n = notification_of(&msg);
                update(login, |s| s.events.push(Event::Notice(n)));
                notifications()?;
                friends()?;
            }
            _ => {}
        },
    }
    Ok(())
}

// Resources and messages for the game and the UI pages (see the module comment).

#[derive(Resource, Default, Clone, Debug)]
pub struct Friends(pub Vec<Friend>);

#[derive(Resource, Default, Clone, Debug)]
pub struct MyProfile(pub Option<Profile>);

#[derive(Resource, Default, Clone, Debug)]
pub struct Notifications(pub Vec<Notification>);

#[derive(Resource, Default, Clone, Debug)]
pub struct MyParty(pub Option<Party>);

#[derive(Resource, Default, Clone, Debug)]
pub struct Inventory(pub Vec<Item>);

#[derive(Resource, Default, Clone, Debug)]
pub struct SocialStatus {
    pub connected: bool,
    pub last_error: Option<String>,
}

#[derive(Message, Clone, Debug)]
pub struct SocialNotice(pub Notification);

#[derive(Message, Clone, Debug)]
pub struct FollowRoom(pub Room);

#[derive(Message, Clone, Debug)]
pub struct SocialError(pub String);

/// The calls the game and the UI make. Each one queues work for the worker and returns at once; without a login
/// they do nothing.
#[derive(Resource, Default)]
pub struct Social;

impl Social {
    fn send(&self, c: Cmd) {
        if let Some(tx) = &SHARED.lock().unwrap().tx {
            let _ = tx.send(c);
        }
    }
    pub fn send_request(&self, id: i64) {
        self.send(Cmd::Relationship("sendfriendrequest", id))
    }
    pub fn accept(&self, id: i64) {
        self.send(Cmd::Relationship("acceptfriendrequest", id))
    }
    pub fn decline(&self, id: i64) {
        self.send(Cmd::Relationship("declinefriendrequest", id))
    }
    pub fn remove_friend(&self, id: i64) {
        self.send(Cmd::Relationship("removefriend", id))
    }
    pub fn block(&self, id: i64) {
        self.send(Cmd::Relationship("ignore", id))
    }
    pub fn unblock(&self, id: i64) {
        self.send(Cmd::Relationship("unignore", id))
    }
    /// Invites a player to your party (making one, with you leading, if you have none).
    pub fn invite_to_party(&self, id: i64) {
        self.send(Cmd::Invite(id))
    }
    /// Joins a party you were invited to (`NoticeKind::PartyInvite { party_id }`), then goes where it is.
    pub fn join_party(&self, party_id: i64) {
        self.send(Cmd::JoinParty(party_id))
    }
    pub fn leave_party(&self) {
        self.send(Cmd::LeaveParty)
    }
    /// Goes to the friend's room instance; answered by `FollowRoom` or `SocialError`.
    pub fn join_friend(&self, id: i64) {
        self.send(Cmd::JoinFriend(id))
    }
    pub fn update_profile(&self, u: ProfileUpdate) {
        self.send(Cmd::UpdateProfile(u))
    }
    /// One notification, or all with None.
    pub fn mark_read(&self, id: Option<i64>) {
        self.send(Cmd::MarkRead(id))
    }
    pub fn delete_notification(&self, id: i64) {
        self.send(Cmd::Delete(id))
    }
    pub fn refresh(&self) {
        self.send(Cmd::Refresh)
    }
}

pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Social>()
            .init_resource::<Friends>()
            .init_resource::<MyProfile>()
            .init_resource::<Notifications>()
            .init_resource::<MyParty>()
            .init_resource::<Inventory>()
            .init_resource::<SocialStatus>()
            .add_message::<SocialNotice>()
            .add_message::<FollowRoom>()
            .add_message::<SocialError>()
            .add_systems(Update, (pull, save_look.after(pull)));
    }
}

/// Copies what the worker changed into the resources and sends its events as messages.
#[allow(clippy::too_many_arguments)]
fn pull(
    mut seen: Local<u64>,
    mut friends: ResMut<Friends>,
    mut profile: ResMut<MyProfile>,
    mut notifications: ResMut<Notifications>,
    mut party: ResMut<MyParty>,
    mut inventory: ResMut<Inventory>,
    mut status: ResMut<SocialStatus>,
    look: Option<ResMut<LocalLook>>,
    mut notices: MessageWriter<SocialNotice>,
    mut follows: MessageWriter<FollowRoom>,
    mut errors: MessageWriter<SocialError>,
) {
    let mut s = SHARED.lock().unwrap();
    if s.version == *seen {
        return;
    }
    *seen = s.version;
    let d = &s.data;
    friends.0 = d.friends.clone();
    profile.0 = d.profile.clone();
    notifications.0 = d.notifications.clone();
    party.0 = d.party.clone();
    inventory.0 = d.inventory.clone();
    *status = SocialStatus { connected: d.connected, last_error: d.last_error.clone() };
    let mut look = look;
    for e in s.events.drain(..) {
        match e {
            Event::Notice(n) => {
                notices.write(SocialNotice(n));
            }
            Event::Follow(r) => {
                follows.write(FollowRoom(r));
            }
            Event::Error(t) => {
                errors.write(SocialError(t));
            }
            Event::Look(l) => {
                if let Some(look) = look.as_mut() {
                    if look.0 != l {
                        look.0 = l;
                    }
                }
            }
        }
    }
}

/// Saves the look on the server: the local one when the server has none yet (first login of this account), and
/// afterwards every change made in the game (the look panel). The server's own look arriving matches the profile and
/// is not sent back.
fn save_look(look: Option<Res<LocalLook>>, profile: Res<MyProfile>, social: Res<Social>, mut sent: Local<Option<Look>>) {
    let (Some(look), Some(p)) = (look, &profile.0) else { return };
    if p.look == Some(look.0) || *sent == Some(look.0) || (p.look.is_some() && !look.is_changed()) {
        return;
    }
    *sent = Some(look.0);
    social.update_profile(ProfileUpdate { look: Some(look.0), ..default() });
}
