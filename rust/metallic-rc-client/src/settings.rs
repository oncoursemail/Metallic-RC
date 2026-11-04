//! The player's settings, kept in `%LOCALAPPDATA%\Metallic-RC\settings.json`: numbers by name (switches are 0 or 1),
//! and the names this PC has logged in as, for the account switcher. Unknown names are kept, so a setting the game's
//! pages show but the client does not use yet still round-trips.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// Defaults of the settings the client uses (the game's: sensitivity 50 of 1..100, volumes 0..100).
const DEFAULTS: &[(&str, f64)] = &[
    ("mouse_sensitivity", 50.0),
    ("look_horizontal", 50.0),
    ("look_vertical", 50.0),
    ("invert_y", 0.0),
    ("invert_x", 0.0),
    ("fov", 60.0),
    ("volume_master", 100.0),
    ("volume_music", 100.0),
    ("volume_sfx", 100.0),
    ("vsync", 1.0),
    ("fullscreen", 0.0),
    ("resolution", 0.0),
];

/// Window sizes the resolution setting steps through (index into this list).
pub const RESOLUTIONS: &[(u32, u32)] = &[(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    values: BTreeMap<String, f64>,
    /// most recent first
    pub accounts: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { values: DEFAULTS.iter().map(|(k, v)| (k.to_string(), *v)).collect(), accounts: vec![] }
    }
}

impl Settings {
    /// `%LOCALAPPDATA%\Metallic-RC\settings.json`
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Metallic-RC").join("settings.json"))
    }

    pub fn get(&self, key: &str) -> f64 {
        self.values.get(key).copied().unwrap_or(0.0)
    }

    pub fn on(&self, key: &str) -> bool {
        self.get(key) != 0.0
    }

    pub fn set(&mut self, key: &str, v: f64) {
        self.values.insert(key.to_string(), v);
    }

    /// Puts `name` first in the account list.
    pub fn remember(&mut self, name: &str) {
        self.accounts.retain(|a| a != name);
        self.accounts.insert(0, name.to_string());
        self.accounts.truncate(8);
    }

    pub fn to_json(&self) -> String {
        let values: Map<String, Value> = self.values.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        serde_json::to_string_pretty(&json!({ "settings": values, "accounts": self.accounts })).unwrap_or_default()
    }

    /// Settings from JSON over the defaults; anything unreadable is left at its default.
    pub fn from_json(text: &str) -> Settings {
        let mut s = Settings::default();
        let Ok(v) = serde_json::from_str::<Value>(text) else { return s };
        if let Some(m) = v.get("settings").and_then(Value::as_object) {
            for (k, x) in m {
                if let Some(x) = x.as_f64() {
                    s.values.insert(k.clone(), x);
                }
            }
        }
        if let Some(a) = v.get("accounts").and_then(Value::as_array) {
            s.accounts = a.iter().filter_map(Value::as_str).map(str::to_string).collect();
        }
        s
    }

    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path).map(|t| Settings::from_json(&t)).unwrap_or_default()
    }

    /// Writes the file (through a temporary file, so a crash never leaves half of it).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("metallic-rc-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.set("mouse_sensitivity", 73.0);
        s.set("invert_y", 1.0);
        s.set("auto_sprint", 1.0);
        s.remember("alice");
        s.remember("bob");
        s.remember("alice");
        s.save(&path).unwrap();
        let back = Settings::load(&path);
        assert_eq!(back, s);
        assert_eq!(back.accounts, ["alice", "bob"]);
        assert!(back.on("invert_y") && back.get("fov") == 60.0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        assert_eq!(Settings::load(Path::new("no/such/settings.json")), Settings::default());
        let s = Settings::from_json(r#"{"settings": {"fov": "wide", "vsync": 0}, "accounts": [1, "x"]}"#);
        assert_eq!((s.get("fov"), s.get("vsync"), s.accounts.clone()), (60.0, 0.0, vec!["x".to_string()]));
    }
}
