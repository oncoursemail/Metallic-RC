//! Storage and the social backend (profiles, friends, blocking, notifications, parties, inventory) against a real
//! server on a free local port. "Restart" = a second server opened on the same data folder after the first is gone.
use std::{net::SocketAddr, path::PathBuf, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("mrc-social-{}-{n}", std::process::id()))
}

/// A server on a data folder.
struct Server {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

async fn start(dir: &std::path::Path) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = metallic_rc_server::App::open(dir, &format!("http://{addr}")).unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, metallic_rc_server::router(app)).await.unwrap() });
    Server { addr, task }
}

/// Stops a server and waits until its task (and so its App and store) is gone.
async fn stop(s: Server) {
    s.task.abort();
    let _ = s.task.await;
}

/// A bare HTTP/1.1 request; `body` is sent as JSON when it starts with `{`, else as a form. Returns (status, body).
async fn http(addr: SocketAddr, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = body.unwrap_or("");
    let ct = if body.starts_with('{') { "application/json" } else { "application/x-www-form-urlencoded" };
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{auth}Content-Type: {ct}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let status = text[9..12].parse().unwrap();
    let (head, mut body) = text.split_once("\r\n\r\n").map(|(h, b)| (h.to_string(), b.to_string())).unwrap();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        let mut joined = String::new();
        let mut rest = body.as_str();
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            joined.push_str(&tail[..n]);
            rest = &tail[n + 2..];
        }
        body = joined;
    }
    (status, body)
}

fn js(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"))
}

/// Logs in by name; returns (access token, account id).
async fn login(addr: SocketAddr, name: &str) -> (String, i64) {
    let (st, body) = http(addr, "POST", "/auth/connect/token", None, Some(&format!("grant_type=steam&username={name}"))).await;
    assert_eq!(st, 200, "{body}");
    let t = js(&body)["access_token"].as_str().unwrap().to_string();
    let (_, me) = http(addr, "GET", "/accounts/account/me", Some(&t), None).await;
    (t, js(&me)["accountId"].as_i64().unwrap())
}

async fn get(addr: SocketAddr, path: &str, token: &str) -> Value {
    let (st, body) = http(addr, "GET", path, Some(token), None).await;
    assert_eq!(st, 200, "GET {path}: {body}");
    js(&body)
}

async fn post(addr: SocketAddr, path: &str, token: &str, body: Option<&str>) -> (u16, Value) {
    let (st, b) = http(addr, "POST", path, Some(token), body).await;
    (st, if b.trim().is_empty() { Value::Null } else { serde_json::from_str(&b).unwrap_or(Value::String(b)) })
}

// ---- 1. storage ----

#[tokio::test]
async fn store_survives_restart_and_concurrent_logins() {
    let dir = temp_dir();
    let s = start(&dir).await;
    let addr = s.addr;
    let logins: Vec<_> = (0..40).map(|i| tokio::spawn(async move { login(addr, &format!("P{i}")).await.1 })).collect();
    let mut ids = vec![];
    for l in logins {
        ids.push(l.await.unwrap());
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 40, "40 parallel logins made 40 accounts");
    stop(s).await;

    let file: Value = js(&std::fs::read_to_string(dir.join("accounts.json")).unwrap());
    assert_eq!(file["version"], 2);
    assert_eq!(file["accounts"].as_array().unwrap().len(), 40);
    let s = start(&dir).await;
    let (_, again) = login(s.addr, "P7").await;
    let p7_first = file["accounts"].as_array().unwrap().iter().find(|a| a["username"] == "P7").unwrap()["accountId"].as_i64().unwrap();
    assert_eq!(again, p7_first, "same name, same account after the restart");
}

#[tokio::test]
async fn store_migrates_the_first_file_and_refuses_a_newer_one() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // the file as the first server wrote it: no version, accounts only
    std::fs::write(dir.join("accounts.json"), r#"{"next_id":2,"accounts":[{"accountId":1,"username":"Old","displayName":"Old","login":"name:Old","createdAt":"2026-11-01T00:00:00Z"}]}"#).unwrap();
    let s = start(&dir).await;
    let (t, id) = login(s.addr, "Old").await;
    assert_eq!(id, 1);
    assert_eq!(get(s.addr, "/api/relationships/v2/get", &t).await, json!([]));
    stop(s).await;
    let file = js(&std::fs::read_to_string(dir.join("accounts.json")).unwrap());
    assert_eq!(file["version"], 2);
    assert_eq!(file["accounts"][0]["username"], "Old");

    std::fs::write(dir.join("accounts.json"), r#"{"version":99,"next_id":1,"accounts":[]}"#).unwrap();
    let err = metallic_rc_server::App::open(&dir, "http://x").err().expect("a newer schema is refused");
    assert!(err.to_string().contains("schema version 99"), "{err}");
}

// ---- hub pushes ----

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the notifications hub as the token's account.
async fn hub(addr: SocketAddr, token: &str) -> Ws {
    let (st, neg) = http(addr, "POST", "/notifications/hub/v1/negotiate?negotiateVersion=1", Some(token), None).await;
    assert_eq!(st, 200);
    let id = js(&neg)["connectionToken"].as_str().unwrap().to_string();
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/notifications/hub/v1?id={id}")).await.unwrap();
    ws.send(Message::text("{\"protocol\":\"json\",\"version\":1}\u{1e}")).await.unwrap();
    assert_eq!(ws.next().await.unwrap().unwrap().into_text().unwrap().as_str(), "{}\u{1e}");
    ws
}

/// The next `Notification` push with this `Id` (others are skipped); its `Msg`.
async fn pushed(ws: &mut Ws, id: &str) -> Value {
    let wait = async {
        loop {
            let m = ws.next().await.expect("hub closed").unwrap();
            for frame in m.into_text().unwrap().split('\u{1e}').filter(|f| !f.is_empty()) {
                let v = js(frame);
                if v["target"] == "Notification" && v["arguments"][0]["Id"] == id {
                    return v["arguments"][0]["Msg"].clone();
                }
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait).await.unwrap_or_else(|_| panic!("no {id} push within 5 s"))
}

// ---- 2. profiles and 6. inventory ----

#[tokio::test]
async fn profiles_and_inventory_are_saved() {
    let dir = temp_dir();
    let s = start(&dir).await;
    let a = s.addr;
    let (ta, ida) = login(a, "Ann").await;
    let (tb, idb) = login(a, "Bob").await;

    let p = get(a, "/api/profile/v1/me", &ta).await;
    assert_eq!((p["accountId"].as_i64(), p["displayName"].as_str(), p["bio"].as_str()), (Some(ida), Some("Ann"), Some("")));
    assert!(p["look"].is_null());
    assert_eq!((p["level"].as_i64(), p["xp"].as_i64()), (Some(1), Some(0)));
    assert!(p["createdAt"].as_str().unwrap().ends_with('Z'));

    // the starter kit
    let inv = get(a, "/api/inventory/v1/me", &ta).await;
    let ids: Vec<_> = inv.as_array().unwrap().iter().map(|i| i["itemId"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids, metallic_rc_server::inventory::STARTER);
    assert_eq!(inv[0]["source"], "starter");

    let look = json!({"skin": "#F2C99B", "shirt": "#2b5fd9", "accent": "#3df5ff", "accessory": "halo"});
    let body = json!({"displayName": "  Ann the Bold ", "bio": "builds robots", "look": look}).to_string();
    let (st, p) = post(a, "/api/profile/v1/me", &ta, Some(&body)).await;
    assert_eq!(st, 200, "{p}");
    assert_eq!(p["displayName"], "Ann the Bold");
    assert_eq!(p["look"]["skin"], "#f2c99b", "colours are kept in lower case");
    // the newer zones, eye style and outfit are kept too; an outfit nobody owns is refused below
    let look = json!({"skin": "#f2c99b", "shirt": "#2b5fd9", "accent": "#3df5ff", "accessory": "goggles", "pants": "#3A3F4B", "visor": "#0f1117",
        "eyes": "happy", "outfit": "scarf"});
    let (st, p) = post(a, "/api/profile/v1/me", &ta, Some(&json!({"look": look}).to_string())).await;
    assert_eq!(st, 200, "{p}");
    assert_eq!((p["look"]["pants"].as_str(), p["look"]["eyes"].as_str(), p["look"]["outfit"].as_str()), (Some("#3a3f4b"), Some("happy"), Some("scarf")));
    // the old account call shows the new name too
    assert_eq!(get(a, "/accounts/account/me", &ta).await["displayName"], "Ann the Bold");

    for bad in [
        json!({"look": {"skin": "red", "shirt": "#000000", "accent": "#000000"}}),
        json!({"look": {"skin": "#000000", "shirt": "#000000", "accent": "#000000", "accessory": "crown"}}),
        json!({"look": {"skin": "#000000", "shirt": "#000000", "accent": "#000000", "outfit": "cape"}}),
        json!({"displayName": ""}),
        json!({"displayName": "x".repeat(33)}),
        json!({"bio": "b".repeat(513)}),
    ] {
        let (st, e) = post(a, "/api/profile/v1/me", &ta, Some(&bad.to_string())).await;
        assert_eq!(st, 400, "{bad} -> {e}");
    }
    assert_eq!(get(a, "/api/profile/v1/me", &ta).await["bio"], "builds robots", "a refused update changes nothing");

    // the game's calls: bio by id, bio and display name by form, progression bulk
    assert_eq!(get(a, &format!("/accounts/account/{ida}/bio"), &tb).await, json!({"accountId": ida, "bio": "builds robots"}));
    let (st, _) = http(a, "PUT", "/accounts/account/me/bio", Some(&tb), Some("bio=hello%20there")).await;
    assert_eq!(st, 200);
    let (st, _) = http(a, "PUT", "/accounts/account/me/displayName", Some(&tb), Some("displayName=Bobby")).await;
    assert_eq!(st, 200);
    let pb = get(a, &format!("/api/profile/v1/{idb}"), &ta).await;
    assert_eq!((pb["bio"].as_str(), pb["displayName"].as_str()), (Some("hello there"), Some("Bobby")));
    let prog = get(a, &format!("/api/players/v2/progression/bulk?id={ida}&id={idb}"), &ta).await;
    assert_eq!(prog[1], json!({"PlayerId": idb, "AccountId": idb, "Level": 1, "XP": 0}));
    stop(s).await;

    // another server on the same folder (a restart): everything is still there
    let s = start(&dir).await;
    let (ta, _) = login(s.addr, "Ann").await;
    let p = get(s.addr, &format!("/api/profile/v1/{ida}"), &ta).await;
    assert_eq!(p["look"], json!({"skin": "#f2c99b", "shirt": "#2b5fd9", "accent": "#3df5ff", "accessory": "goggles", "pants": "#3a3f4b",
        "visor": "#0f1117", "eyes": "happy", "outfit": "scarf"}));
    assert_eq!(p["bio"], "builds robots");
    assert_eq!(get(s.addr, "/api/inventory/v1/me", &ta).await.as_array().unwrap().len(), metallic_rc_server::inventory::STARTER.len());
}

// ---- 3. friends and blocking, 4. notifications ----

fn rel(list: &Value, id: i64) -> Option<Value> {
    list.as_array().unwrap().iter().find(|r| r["PlayerID"] == id).cloned()
}

#[tokio::test]
async fn friends_blocking_and_notifications() {
    let dir = temp_dir();
    let s = start(&dir).await;
    let a = s.addr;
    let (ta, ida) = login(a, "Ann").await;
    let (tb, idb) = login(a, "Bob").await;
    let (tc, idc) = login(a, "Cat").await;
    let mut hb = hub(a, &tb).await;

    // Ann asks Bob: Bob is told (relationship + stored message), both lists show the request
    let (st, r) = post(a, "/api/relationships/v2/sendfriendrequest", &ta, Some(&format!("id={idb}"))).await;
    assert_eq!(st, 200, "{r}");
    assert_eq!(r["RelationshipType"], 1);
    assert_eq!(pushed(&mut hb, "RelationshipChanged").await["RelationshipType"], 2);
    let m = pushed(&mut hb, "MessageReceived").await;
    assert_eq!((m["Kind"].as_str(), m["FromPlayerId"].as_i64()), (Some("friendRequest"), Some(ida)));
    assert_eq!(rel(&get(a, "/api/relationships/v2/get", &tb).await, ida).unwrap()["RelationshipType"], 2);
    let msgs = get(a, "/api/messages/v2/get", &tb).await;
    assert_eq!(msgs.as_array().unwrap().len(), 1);
    assert_eq!(msgs[0]["Read"], false);
    let mut ha = hub(a, &ta).await;

    // Bob accepts: friends both ways, Ann is told
    let (st, r) = post(a, &format!("/api/relationships/v2/acceptfriendrequest?id={ida}"), &tb, None).await;
    assert_eq!((st, r["RelationshipType"].as_i64()), (200, Some(3)));
    assert_eq!(pushed(&mut ha, "RelationshipChanged").await["RelationshipType"], 3);
    assert_eq!(pushed(&mut ha, "MessageReceived").await["Kind"], "friendAccepted");
    assert_eq!(rel(&get(a, "/api/relationships/v2/get", &ta).await, idb).unwrap()["RelationshipType"], 3);

    // the friends page: Bob is online (hub open) and, after matchmaking, in the Rec Center
    assert_eq!(post(a, "/matchmaking/player/login", &tb, None).await.0, 200);
    let (_, g) = post(a, "/matchmaking/goto/room/RecCenter", &tb, None).await;
    assert_eq!(g["errorCode"], 0);
    let presence = pushed(&mut ha, "SubscriptionUpdatePresence").await;
    assert_eq!((presence["PlayerId"].as_i64(), presence["IsOnline"].as_bool()), (Some(idb), Some(true)));
    let f = get(a, "/api/social/v1/friends", &ta).await;
    assert_eq!(f[0]["accountId"], idb);
    assert_eq!(f[0]["relationship"], "friend");
    assert_eq!(f[0]["isOnline"], true);
    assert_eq!(f[0]["roomInstance"]["roomId"], 2);
    let pres = get(a, &format!("/matchmaking/player?id={idb}&id={idc}"), &ta).await;
    assert_eq!((pres[0]["IsOnline"].as_bool(), pres[0]["RoomInstance"]["roomId"].as_i64()), (Some(true), Some(2)));
    assert_eq!(pres[1]["IsOnline"], false, "Cat never logged in");

    // notifications: mark read, delete
    let mid = msgs[0]["Id"].as_i64().unwrap();
    assert_eq!(post(a, "/api/messages/v2/markread", &tb, Some(&format!("id={mid}"))).await.0, 200);
    assert_eq!(get(a, "/api/messages/v2/get", &tb).await[0]["Read"], true);
    assert_eq!(post(a, "/api/messages/v2/delete", &tb, Some(&format!("{{\"id\":{mid}}}"))).await.0, 200);
    assert_eq!(pushed(&mut hb, "MessageDeleted").await["Id"], mid);
    assert_eq!(get(a, "/api/messages/v2/get", &tb).await, json!([]));

    // Cat asks Ann, Ann declines
    post(a, &format!("/api/relationships/v2/sendfriendrequest?id={ida}"), &tc, None).await;
    assert_eq!(post(a, &format!("/api/relationships/v2/declinefriendrequest?id={idc}"), &ta, None).await.0, 200);
    assert!(rel(&get(a, "/api/relationships/v2/get", &ta).await, idc).is_none());
    assert_eq!(post(a, &format!("/api/relationships/v2/declinefriendrequest?id={idc}"), &ta, None).await.0, 404);

    // Ann blocks Cat: Cat cannot ask again or see Ann online; Ann's list shows Ignored = Local
    let (st, r) = post(a, &format!("/api/relationships/v2/ignore?id={idc}"), &ta, None).await;
    assert_eq!((st, r["Ignored"].as_i64()), (200, Some(1)));
    assert_eq!(post(a, &format!("/api/relationships/v2/sendfriendrequest?id={ida}"), &tc, None).await.0, 403);
    assert!(rel(&get(a, "/api/relationships/v2/get", &tc).await, ida).is_none(), "the blocked side sees nothing");
    post(a, "/matchmaking/player/login", &ta, None).await;
    assert_eq!(get(a, &format!("/matchmaking/player?id={ida}"), &tc).await[0]["IsOnline"], false);
    let fa = get(a, "/api/social/v1/friends", &ta).await;
    assert_eq!(fa.as_array().unwrap().iter().find(|f| f["accountId"] == idc).unwrap()["relationship"], "blocked");
    assert_eq!(post(a, &format!("/api/relationships/v2/unignore?id={idc}"), &ta, None).await.0, 200);
    assert_eq!(post(a, &format!("/api/relationships/v2/sendfriendrequest?id={ida}"), &tc, None).await.0, 200);

    // blocking a friend ends the friendship; two requests make friends; removing works both ways
    post(a, &format!("/api/relationships/v2/ignore?id={idb}"), &ta, None).await;
    assert_eq!(rel(&get(a, "/api/relationships/v2/get", &tb).await, ida), None);
    post(a, &format!("/api/relationships/v2/unignore?id={idb}"), &ta, None).await;
    post(a, &format!("/api/relationships/v2/sendfriendrequest?id={idb}"), &ta, None).await;
    post(a, &format!("/api/relationships/v2/sendfriendrequest?id={ida}"), &tb, None).await;
    assert_eq!(rel(&get(a, "/api/relationships/v2/get", &ta).await, idb).unwrap()["RelationshipType"], 3);
    post(a, &format!("/api/relationships/v2/removefriend?id={ida}"), &tb, None).await;
    assert!(rel(&get(a, "/api/relationships/v2/get", &ta).await, idb).is_none());

    // bad requests
    assert_eq!(post(a, "/api/relationships/v2/sendfriendrequest", &ta, None).await.0, 400);
    assert_eq!(post(a, "/api/relationships/v2/sendfriendrequest?id=999", &ta, None).await.0, 404);
    assert_eq!(post(a, &format!("/api/relationships/v2/sendfriendrequest?id={ida}"), &ta, None).await.0, 400);
    assert_eq!(http(a, "GET", "/api/relationships/v2/get", None, None).await.0, 401);
    stop(s).await;

    // a restart keeps relationships and messages
    let s = start(&dir).await;
    let (tc, _) = login(s.addr, "Cat").await;
    assert_eq!(rel(&get(s.addr, "/api/relationships/v2/get", &tc).await, ida).unwrap()["RelationshipType"], 1);
    let (ta, _) = login(s.addr, "Ann").await;
    let msgs = get(s.addr, "/api/messages/v2/get", &ta).await;
    assert!(msgs.as_array().unwrap().iter().any(|m| m["Kind"] == "friendRequest" && m["FromPlayerId"] == idc));
}

// ---- 5. parties and joining friends ----

async fn befriend(a: SocketAddr, (t1, id1): (&str, i64), (t2, id2): (&str, i64)) {
    post(a, &format!("/api/relationships/v2/sendfriendrequest?id={id2}"), t1, None).await;
    let (st, r) = post(a, &format!("/api/relationships/v2/acceptfriendrequest?id={id1}"), t2, None).await;
    assert_eq!((st, r["RelationshipType"].as_i64()), (200, Some(3)));
}

#[tokio::test]
async fn parties_follow_the_leader_and_friends_join() {
    let s = start(&temp_dir()).await;
    let a = s.addr;
    let (ta, ida) = login(a, "Ann").await;
    let (tb, idb) = login(a, "Bob").await;
    let (tc, idc) = login(a, "Cat").await;
    let (td, idd) = login(a, "Dan").await;
    for t in [&ta, &tb, &tc, &td] {
        post(a, "/matchmaking/player/login", t, None).await;
    }
    let mut hb = hub(a, &tb).await;

    // Ann invites Bob (making the party); Bob gets a stored invite and joins
    let (st, p) = post(a, &format!("/api/party/v1/invite?id={idb}"), &ta, None).await;
    assert_eq!(st, 200, "{p}");
    assert_eq!((p["leaderId"].as_i64(), p["invited"][0].as_i64()), (Some(ida), Some(idb)));
    let inv = pushed(&mut hb, "MessageReceived").await;
    assert_eq!(inv["Kind"], "partyInvite");
    let party_id = inv["Data"].as_str().unwrap().to_string();
    assert_eq!(party_id, p["partyId"].to_string());
    assert_eq!(post(a, &format!("/api/party/v1/join?id={party_id}"), &tc, None).await.0, 403, "Cat was not invited");
    let (st, p) = post(a, &format!("/api/party/v1/join?id={party_id}"), &tb, None).await;
    assert_eq!((st, p["members"].clone()), (200, json!([ida, idb])));
    assert_eq!(pushed(&mut hb, "PartyChanged").await["members"], json!([ida, idb]));
    let left = get(a, "/api/messages/v2/get", &tb).await;
    assert!(left.as_array().unwrap().iter().all(|m| m["Kind"] != "partyInvite"), "the invite is used up");

    // the leader goes to the dorm (private); Bob is told to follow and lands in the same instance
    let (_, g) = post(a, "/matchmaking/goto/room/dormroom", &ta, None).await;
    let ann_dorm = g["roomInstance"].clone();
    assert_eq!(ann_dorm["isPrivate"], true);
    let follow = pushed(&mut hb, "MessageReceived").await;
    assert_eq!((follow["Kind"].as_str(), follow["FromPlayerId"].as_i64()), (Some("partyGoto"), Some(ida)));
    assert_eq!(js(follow["Data"].as_str().unwrap())["photonRoomId"], ann_dorm["photonRoomId"]);
    let (_, g) = post(a, "/matchmaking/goto/room/dormroom", &tb, None).await;
    assert_eq!(g["roomInstance"]["photonRoomId"], ann_dorm["photonRoomId"], "a member going to the party room joins the party instance");
    let (_, g) = post(a, &format!("/matchmaking/goto/player/{ida}"), &tb, None).await;
    assert_eq!(g["errorCode"], 0);
    assert_eq!(g["roomInstance"]["photonRoomId"], ann_dorm["photonRoomId"], "party mates join even a private dorm");
    assert_eq!(get(a, "/api/party/v1/me", &tb).await["roomInstance"]["photonRoomId"], ann_dorm["photonRoomId"]);

    // joining a friend: Dan is a friend of Ann but not in the party; Cat is a stranger, then blocked
    befriend(a, (&ta, ida), (&td, idd)).await;
    let (_, g) = post(a, &format!("/matchmaking/goto/player/{ida}"), &td, None).await;
    assert_eq!((g["errorCode"].as_i64(), g["error"].as_str()), (Some(26), Some("RoomInstanceIsPrivate")));
    let (_, g) = post(a, &format!("/matchmaking/goto/player/{ida}"), &tc, None).await;
    assert_eq!(g["errorCode"], 14, "not friends");
    post(a, "/matchmaking/goto/room/RecCenter", &ta, None).await;
    let (_, g) = post(a, &format!("/matchmaking/matchmake/v2/player/{ida}"), &td, None).await;
    assert_eq!((g["errorCode"].as_i64(), g["roomInstance"]["roomId"].as_i64()), (Some(0), Some(2)));
    let f = get(a, "/api/social/v1/friends", &td).await;
    assert_eq!(f[0]["roomInstance"]["roomId"], 2);
    befriend(a, (&tc, idc), (&ta, ida)).await;
    post(a, &format!("/api/relationships/v2/ignore?id={idc}"), &ta, None).await;
    let (_, g) = post(a, &format!("/matchmaking/goto/player/{ida}"), &tc, None).await;
    assert_eq!(g["errorCode"], 7, "blocked players cannot join");
    assert_eq!(post(a, &format!("/api/party/v1/invite?id={ida}"), &tc, None).await.0, 403);
    assert_eq!(post(a, &format!("/api/party/v1/invite?id={idc}"), &ta, None).await.0, 403);
    post(a, "/matchmaking/player/logout", &td, None).await;
    let (_, g) = post(a, &format!("/matchmaking/goto/player/{idd}"), &ta, None).await;
    assert_eq!(g["errorCode"], 2, "Dan logged out");

    // leaving: the leader leaves, Bob leads; Bob leaves, the party ends
    assert_eq!(post(a, "/api/party/v1/leave", &ta, None).await.0, 200);
    assert_eq!(get(a, "/api/party/v1/me", &tb).await["leaderId"], idb);
    assert!(get(a, "/api/party/v1/me", &ta).await.is_null());
    post(a, "/api/party/v1/leave", &tb, None).await;
    assert!(get(a, "/api/party/v1/me", &tb).await.is_null());
}
