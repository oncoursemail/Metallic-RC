//! Accounts, kept in `<data-dir>/accounts.json` (rewritten atomically on every change), and the token signing key.
use std::{
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

#[derive(Default, Serialize, Deserialize)]
struct Data {
    next_id: i64,
    accounts: Vec<Account>,
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
        let data = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).map_err(std::io::Error::other)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data { next_id: 1, accounts: vec![] },
            Err(e) => return Err(e),
        };
        Ok(Store { path, data: Mutex::new(data) })
    }

    fn save(&self, d: &Data) {
        let tmp = self.path.with_extension("json.new");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(d).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
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
        self.save(&d);
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
