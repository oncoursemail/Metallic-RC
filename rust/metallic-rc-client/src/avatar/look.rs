//! A player's look: shell (skin), shirt and accent colours and one accessory. It travels as a small JSON object
//! (`{"skin":"#rrggbb","shirt":..,"accent":..,"accessory":"antenna"}`), the same text that is saved to
//! `%LOCALAPPDATA%\Metallic-RC\avatar.json` and announced to the room.
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Accessory {
    #[default]
    None,
    Antenna,
    Headphones,
    Halo,
}

impl Accessory {
    pub const ALL: [Accessory; 4] = [Accessory::None, Accessory::Antenna, Accessory::Headphones, Accessory::Halo];

    pub fn name(self) -> &'static str {
        match self {
            Accessory::None => "none",
            Accessory::Antenna => "antenna",
            Accessory::Headphones => "headphones",
            Accessory::Halo => "halo",
        }
    }

    pub fn from_name(s: &str) -> Option<Accessory> {
        Accessory::ALL.into_iter().find(|a| a.name() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// the head shell and hands
    pub skin: [u8; 3],
    pub shirt: [u8; 3],
    /// eyes, mouth, badge, cuffs and the accessory's glow
    pub accent: [u8; 3],
    pub accessory: Accessory,
}

impl Default for Look {
    fn default() -> Self {
        Look { skin: SKINS[0], shirt: SHIRTS[0], accent: ACCENTS[0], accessory: Accessory::Antenna }
    }
}

/// The colours the look panel steps through.
pub const SKINS: [[u8; 3]; 8] = [
    [0xe8, 0xec, 0xf1], [0xf2, 0xc9, 0x9b], [0xc8, 0x8b, 0x5a], [0x7a, 0x4e, 0x33],
    [0x9a, 0xa7, 0xb8], [0xf5, 0xd0, 0x4a], [0x8f, 0xd6, 0xc3], [0xd9, 0x9a, 0xc9],
];
pub const SHIRTS: [[u8; 3]; 8] = [
    [0x2b, 0x5f, 0xd9], [0xe0, 0x4f, 0x3c], [0x2f, 0x9e, 0x63], [0x7b, 0x45, 0xc9],
    [0xf0, 0x9a, 0x1c], [0x23, 0x2a, 0x36], [0xf3, 0xf1, 0xea], [0x15, 0x9a, 0xa8],
];
pub const ACCENTS: [[u8; 3]; 8] = [
    [0x3d, 0xf5, 0xff], [0xff, 0x5c, 0xa8], [0x9d, 0xff, 0x4a], [0xff, 0xb3, 0x2e],
    [0xff, 0xff, 0xff], [0xb5, 0x7d, 0xff], [0xff, 0x45, 0x45], [0x4a, 0x9d, 0xff],
];

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Look {
    pub fn to_json(&self) -> String {
        json!({"skin": hex(self.skin), "shirt": hex(self.shirt), "accent": hex(self.accent), "accessory": self.accessory.name()}).to_string()
    }

    /// Fields that are missing or unreadable keep the default's.
    pub fn from_json(s: &str) -> Option<Look> {
        let v: Value = serde_json::from_str(s).ok()?;
        let v = v.as_object()?;
        let mut l = Look::default();
        let colour = |k: &str| v.get(k).and_then(Value::as_str).and_then(parse_hex);
        for (k, c) in [("skin", &mut l.skin), ("shirt", &mut l.shirt), ("accent", &mut l.accent)] {
            if let Some(x) = colour(k) {
                *c = x;
            }
        }
        if let Some(a) = v.get("accessory").and_then(Value::as_str).and_then(Accessory::from_name) {
            l.accessory = a;
        }
        Some(l)
    }

    /// `--look skin,shirt,accent[,accessory]`, colours as hex: `f2c99b,2b5fd9,3df5ff,halo`.
    pub fn parse_arg(s: &str) -> Result<Look, String> {
        let parts: Vec<&str> = s.split(',').map(str::trim).collect();
        if !(3..=4).contains(&parts.len()) {
            return Err("--look wants skin,shirt,accent[,accessory] (hex colours)".into());
        }
        let c = |i: usize| parse_hex(parts[i]).ok_or_else(|| format!("--look: {:?} is not a hex colour", parts[i]));
        let accessory = match parts.get(3) {
            Some(a) => Accessory::from_name(a).ok_or_else(|| format!("--look: accessory is one of none, antenna, headphones, halo, not {a:?}"))?,
            None => Accessory::None,
        };
        Ok(Look { skin: c(0)?, shirt: c(1)?, accent: c(2)?, accessory })
    }

    /// A look picked from the palettes by the player's name, so players without a saved look still differ.
    pub fn for_name(name: &str) -> Look {
        // FNV-1a: stable across runs and machines
        let h = name.bytes().fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193));
        let pick = |shift: u32| (h >> shift) as usize % 8;
        Look { skin: SKINS[pick(0)], shirt: SHIRTS[pick(3)], accent: ACCENTS[pick(6)], accessory: Accessory::ALL[(h >> 9) as usize % 4] }
    }

    pub fn path() -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Metallic-RC").join("avatar.json"))
    }

    pub fn load() -> Option<Look> {
        Look::from_json(&std::fs::read_to_string(Look::path()?).ok()?)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Look::path().ok_or_else(|| std::io::Error::other("LOCALAPPDATA is not set"))?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, self.to_json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trips_and_tolerates_gaps() {
        let l = Look { skin: [1, 2, 3], shirt: [0xab, 0xcd, 0xef], accent: [255, 0, 128], accessory: Accessory::Halo };
        let v: Value = serde_json::from_str(&l.to_json()).unwrap();
        assert_eq!(v, json!({"skin": "#010203", "shirt": "#abcdef", "accent": "#ff0080", "accessory": "halo"}));
        assert_eq!(Look::from_json(&l.to_json()), Some(l));
        // unknown fields are ignored, missing or bad ones keep the default
        let partial = Look::from_json(r##"{"shirt":"#000000","accent":"blue","accessory":"crown","v":2}"##).unwrap();
        assert_eq!(partial, Look { shirt: [0, 0, 0], ..Look::default() });
        assert_eq!(Look::from_json("not json"), None);
        assert_eq!(Look::from_json("[1]"), None);
    }

    #[test]
    fn command_line_and_name_looks() {
        assert_eq!(Look::parse_arg("f2c99b, #2b5fd9,3DF5FF,headphones"),
            Ok(Look { skin: [0xf2, 0xc9, 0x9b], shirt: [0x2b, 0x5f, 0xd9], accent: [0x3d, 0xf5, 0xff], accessory: Accessory::Headphones }));
        assert_eq!(Look::parse_arg("000000,ffffff,808080").unwrap().accessory, Accessory::None);
        assert!(Look::parse_arg("000000,ffffff").is_err());
        assert!(Look::parse_arg("000000,ffffff,80808g").is_err());
        assert!(Look::parse_arg("000000,ffffff,808080,cape").is_err());
        assert!(Look::parse_arg("ü00000,ffffff,808080").is_err());
        assert_eq!(Look::for_name("Ada"), Look::for_name("Ada"));
        assert_ne!(Look::for_name("Ada"), Look::for_name("Bo"));
    }
}
