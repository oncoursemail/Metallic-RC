//! Everything the server keeps: accounts, profiles, relationships, messages (notifications) and inventory, in one
//! file, `<data-dir>/accounts.json`, rewritten atomically (temp file, fsync, rename) under one lock on every change, so
//! it survives restarts and concurrent requests. Plus the token signing key.
//!
//! The file carries a schema `version`. Version 0 (no field) is the first file, accounts only; opening it fills the new
//! lists with empty defaults and writes it back as version 2. A file from a newer server (higher version) is refused,
//! so an old binary cannot drop data it does not know.
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rand::RngCore;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub account_id: i64,
    pub username: String,
    pub display_name: String,
    /// "steam:<id>", "device:<id>" or "name:<username>": how this account logs in
    pub login: String,
    pub created_at: String,
}

/// The schema this server writes.
pub const SCHEMA_VERSION: u32 = 2;

/// What a player shows others beyond the account: bio, the robot avatar's look, progression.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub bio: String,
    /// the client's avatar.json look, `{"skin":"#rrggbb","shirt":..,"accent":..,"accessory":"antenna"}`; None = never saved
    pub look: Option<serde_json::Value>,
    pub level: i32,
    pub xp: i32,
    /// what the game's `api/avatar/v2/set` posted, sent back as is by `api/avatar/v2`
    pub game_avatar: Option<serde_json::Value>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelKind {
    /// `from` asked `to` to be friends
    Request,
    /// one of the two edges a friendship is stored as
    Friend,
    /// `from` blocked `to` (the game calls it Ignored)
    Block,
}

/// One directed edge between two accounts.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub from: i64,
    pub to: i64,
    pub kind: RelKind,
    pub since: String,
}

/// A stored notification, shaped like the game's `RecNet.Message` plus `to`, our `kind` and a read flag.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub to: i64,
    pub from: i64,
    pub sent: String,
    /// `RecNet.Message.MessageType` number
    pub message_type: i32,
    /// ours: "friendRequest", "friendAccepted", "partyInvite"
    pub kind: String,
    pub data: String,
    pub room_id: Option<i64>,
    #[serde(default)]
    pub read: bool,
}

/// An item an account owns. Only the server grants items (no purchases).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub owner: i64,
    /// "<category>:<name>", e.g. "accessory:halo"
    pub item_id: String,
    pub category: String,
    pub granted_at: String,
    /// why it was granted, e.g. "starter"
    pub source: String,
}

/// The whole file. Fields are public so the social modules work on it inside `Store::read` / `Store::write`.
#[derive(Default, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub version: u32,
    pub next_id: i64,
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub profiles: BTreeMap<i64, Profile>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub next_message_id: i64,
    #[serde(default)]
    pub items: Vec<Item>,
}

pub struct Store {
    path: PathBuf,
    data: Mutex<Data>,
}

pub fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    // RFC 3339 in UTC without a date library
    let (d, r) = (s / 86400, s % 86400);
    let (y, m, day) = civil(d as i64);
    format!("{y:04}-{m:02}-{day:02}T{:02}:{:02}:{:02}Z", r / 3600, r % 3600 / 60, r % 60)
}

// days since 1970-01-01 -> (year, month, day); Howard Hinnant's algorithm
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

impl Store {
    pub fn open(dir: &Path) -> std::io::Result<Store> {
        let path = dir.join("accounts.json");
        let mut data: Data = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).map_err(std::io::Error::other)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data { version: SCHEMA_VERSION, next_id: 1, ..Data::default() },
            Err(e) => return Err(e),
        };
        if data.version > SCHEMA_VERSION {
            return Err(std::io::Error::other(format!(
                "{} is schema version {}, this server knows up to {SCHEMA_VERSION}: use a newer server",
                path.display(),
                data.version
            )));
        }
        let store = Store { path, data: Mutex::new(Data::default()) };
        if data.version < SCHEMA_VERSION {
            println!("store: migrating {} from schema version {} to {SCHEMA_VERSION}", store.path.display(), data.version);
            data.version = SCHEMA_VERSION;
            data.next_message_id = data.next_message_id.max(1);
            store.save(&data)?;
        }
        *store.data.lock().unwrap() = data;
        Ok(store)
    }

    fn save(&self, d: &Data) -> std::io::Result<()> {
        let tmp = self.path.with_extension("json.new");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(d).map_err(std::io::Error::other)?)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &self.path)
    }

    /// Reads the data under the lock.
    pub fn read<R>(&self, f: impl FnOnce(&Data) -> R) -> R {
        f(&self.data.lock().unwrap())
    }

    /// Changes the data under the lock and writes the file before the lock is let go. A failed write is logged; the
    /// change stays in memory and goes out with the next write.
    pub fn write<R>(&self, f: impl FnOnce(&mut Data) -> R) -> R {
        let mut d = self.data.lock().unwrap();
        let r = f(&mut d);
        if let Err(e) = self.save(&d) {
            eprintln!("store: writing {} failed: {e}", self.path.display());
        }
        r
    }

    pub fn get(&self, id: i64) -> Option<Account> {
        self.data.lock().unwrap().accounts.iter().find(|a| a.account_id == id).cloned()
    }

    pub fn by_name(&self, username: &str) -> Option<Account> {
        let d = self.data.lock().unwrap();
        d.accounts.iter().find(|a| a.username.eq_ignore_ascii_case(username)).cloned()
    }

    /// The account for this login key, created on first use with a free username based on `wanted_name`.
    pub fn login(&self, login: &str, wanted_name: &str) -> Account {
        let mut d = self.data.lock().unwrap();
        if let Some(a) = d.accounts.iter().find(|a| a.login == login) {
            return a.clone();
        }
        let base: String = wanted_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(20).collect();
        let base = if base.is_empty() { "Player".to_string() } else { base };
        let mut username = base.clone();
        let mut n = 1;
        while d.accounts.iter().any(|a| a.username.eq_ignore_ascii_case(&username)) {
            n += 1;
            username = format!("{base}{n}");
        }
        let a = Account {
            account_id: d.next_id,
            display_name: username.clone(),
            username,
            login: login.to_string(),
            created_at: now(),
        };
        d.next_id += 1;
        d.accounts.push(a.clone());
        if let Err(e) = self.save(&d) {
            eprintln!("store: writing {} failed: {e}", self.path.display());
        }
        a
    }
}

/// 32 random bytes in `<data-dir>/signing.key`, made on first start. Tokens signed with it survive restarts.
pub fn signing_key(dir: &Path) -> std::io::Result<Vec<u8>> {
    let path = dir.join("signing.key");
    match std::fs::read(&path) {
        Ok(k) if k.len() >= 32 => Ok(k),
        _ => {
            let mut k = vec![0u8; 32];
            rand::thread_rng().fill_bytes(&mut k);
            std::fs::write(&path, &k)?;
            Ok(k)
        }
    }
}
