//! The client's side of RecNet start-up against metallic-rc-server: name server, login, account, config,
//! matchmaking, then the notifications hub. Blocking; the game runs it on its own thread.
use std::{fmt, net::TcpStream, time::Duration};

use serde_json::Value;
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn err(what: &str, e: impl fmt::Display) -> Error {
    Error(format!("{what}: {e}"))
}

/// Who logs in. The server picks the account by Steam id, else device id, else name.
#[derive(Clone, Debug)]
pub enum Login {
    Steam(String),
    Device(String),
    Name(String),
}

#[derive(Clone, Debug, Default)]
pub struct Account {
    pub id: i64,
    pub username: String,
    pub display_name: String,
}

/// Where the client stands: its room instance as matchmaking sent it.
#[derive(Clone, Debug, Default)]
pub struct Room {
    pub name: String,
    pub room_id: i64,
    pub instance_id: i64,
    pub photon_room: String,
    pub private: bool,
}

pub struct Session {
    agent: ureq::Agent,
    services: serde_json::Map<String, Value>,
    pub access_token: String,
    pub refresh_token: String,
    pub account: Account,
    pub motd: String,
    pub room: Room,
}

impl Session {
    /// The base address of a RecNet service ("Auth", "API", "Matchmaking", ...).
    pub fn service(&self, name: &str) -> Option<&str> {
        self.services.get(name)?.as_str()
    }

    fn url(&self, service: &str, path: &str) -> Result<String, Error> {
        Ok(format!("{}{path}", self.service(service).ok_or_else(|| Error(format!("no {service} service")))?))
    }

    fn get(&self, service: &str, path: &str) -> Result<Value, Error> {
        let url = self.url(service, path)?;
        let mut r = self.agent.get(&url).header("Authorization", format!("Bearer {}", self.access_token)).call().map_err(|e| err(&url, e))?;
        r.body_mut().read_json().map_err(|e| err(&url, e))
    }

    fn post(&self, service: &str, path: &str) -> Result<Value, Error> {
        let url = self.url(service, path)?;
        let mut r = self.agent.post(&url).header("Authorization", format!("Bearer {}", self.access_token)).send_empty().map_err(|e| err(&url, e))?;
        // some answers are a bare 200 with no body
        let text = r.body_mut().read_to_string().map_err(|e| err(&url, e))?;
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| err(&url, e))
    }

    /// Asks matchmaking for a room by name and returns the instance it puts this player in.
    pub fn goto(&mut self, room: &str) -> Result<&Room, Error> {
        let g = self.post("Matchmaking", &format!("/goto/room/{room}"))?;
        if g["errorCode"].as_i64() != Some(0) {
            return Err(Error(format!("matchmaking refused {room}: {g}")));
        }
        let i = &g["roomInstance"];
        self.room = Room {
            name: i["name"].as_str().unwrap_or("").trim_start_matches('^').to_string(),
            room_id: i["roomId"].as_i64().unwrap_or(0),
            instance_id: i["roomInstanceId"].as_i64().unwrap_or(0),
            photon_room: i["photonRoomId"].as_str().unwrap_or("").to_string(),
            private: i["isPrivate"].as_bool().unwrap_or(false),
        };
        Ok(&self.room)
    }

    /// Opens the notifications hub (SignalR JSON protocol over a WebSocket) and completes its handshake.
    pub fn open_hub(&self) -> Result<Hub, Error> {
        let base = self.url("Notifications", "/hub/v1")?;
        let mut r = self.agent.post(&format!("{base}/negotiate?negotiateVersion=1"))
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send_empty()
            .map_err(|e| err("hub negotiate", e))?;
        let n: Value = r.body_mut().read_json().map_err(|e| err("hub negotiate", e))?;
        let token = n["connectionToken"].as_str().ok_or_else(|| Error(format!("hub negotiate: no token in {n}")))?;
        let ws_url = format!("{}?id={token}", base.replacen("http", "ws", 1));
        let (mut ws, _) = tungstenite::connect(&ws_url).map_err(|e| err(&ws_url, e))?;
        ws.send(Message::text("{\"protocol\":\"json\",\"version\":1}\u{1e}")).map_err(|e| err("hub handshake", e))?;
        let reply = ws.read().map_err(|e| err("hub handshake", e))?;
        let text = reply.to_text().unwrap_or("");
        if text.trim_end_matches('\u{1e}') != "{}" {
            return Err(Error(format!("hub handshake refused: {text}")));
        }
        Ok(Hub { ws, next_id: 1 })
    }
}

/// An open notifications hub. `invoke` sends a call and waits for its completion, skipping pings.
pub struct Hub {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    next_id: u64,
}

impl Hub {
    pub fn invoke(&mut self, target: &str, arguments: Value) -> Result<Value, Error> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let msg = serde_json::json!({ "type": 1, "invocationId": id, "target": target, "arguments": arguments });
        self.ws.send(Message::text(format!("{msg}\u{1e}"))).map_err(|e| err(target, e))?;
        loop {
            let m = self.ws.read().map_err(|e| err(target, e))?;
            for frame in m.to_text().unwrap_or("").split('\u{1e}').filter(|f| !f.is_empty()) {
                let v: Value = serde_json::from_str(frame).map_err(|e| err(target, e))?;
                if v["type"] == 3 && v["invocationId"] == id.as_str() {
                    return Ok(v["result"].clone());
                }
            }
        }
    }

    pub fn close(mut self) {
        let _ = self.ws.send(Message::text("{\"type\":7}\u{1e}"));
        let _ = self.ws.close(None);
    }
}

/// Start-up as the game does it: name server, login, account, config, matchmaking login, then the dorm.
pub fn connect(name_server: &str, login: &Login) -> Result<Session, Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let ns = format!("{}/?buildId=23191908", name_server.trim_end_matches('/'));
    let services = match agent.get(&ns).call().map_err(|e| err(&ns, e))?.body_mut().read_json::<Value>().map_err(|e| err(&ns, e))? {
        Value::Object(m) => m,
        other => return Err(Error(format!("name server sent {other}"))),
    };
    if !services.get("RecNetStatus").is_some_and(Value::is_null) {
        return Err(Error(format!("RecNet is down: {:?}", services.get("RecNetStatus"))));
    }

    let auth = services.get("Auth").and_then(Value::as_str).ok_or_else(|| Error("no Auth service".into()))?;
    let url = format!("{auth}/connect/token");
    let mut form = vec![("client_id", "recroom".to_string()), ("grant_type", "steam".to_string())];
    match login {
        Login::Steam(id) => form.push(("platform_id", id.clone())),
        Login::Device(id) => form.push(("device_id", id.clone())),
        Login::Name(n) => form.push(("username", n.clone())),
    }
    // a refused login is a 400 with Error / error_description in the body: read it rather than fail on the status
    let mut r = agent.post(&url).config().http_status_as_error(false).build()
        .send_form(form.iter().map(|(k, v)| (*k, v.as_str())))
        .map_err(|e| err(&url, e))?;
    let t: Value = r.body_mut().read_json().map_err(|e| err(&url, e))?;
    let access_token = t["access_token"].as_str()
        .ok_or_else(|| Error(format!("login refused: {} {}", t["Error"], t["error_description"])))?
        .to_string();

    let mut s = Session {
        agent,
        services,
        access_token,
        refresh_token: t["refresh_token"].as_str().unwrap_or("").to_string(),
        account: Account::default(),
        motd: String::new(),
        room: Room::default(),
    };
    let me = s.get("Accounts", "/account/me")?;
    s.account = Account {
        id: me["accountId"].as_i64().unwrap_or(0),
        username: me["username"].as_str().unwrap_or("").to_string(),
        display_name: me["displayName"].as_str().unwrap_or("").to_string(),
    };
    s.motd = s.get("API", "/config/v2")?["MessageOfTheDay"].as_str().unwrap_or("").to_string();
    s.post("Matchmaking", "/player/login")?;
    s.goto("dormroom")?;
    Ok(s)
}
