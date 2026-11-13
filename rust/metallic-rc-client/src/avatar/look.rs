//! A player's look: the colour zones (shell, shirt, pants, face screen, accent), the eye style, one head accessory and
//! one outfit piece. It travels as a small JSON object (`{"skin":"#rrggbb","shirt":..,"accent":..,"accessory":"antenna",
//! "pants":..,"visor":..,"eyes":"pill","outfit":"scarf"}`), the same text that is saved to
//! `%LOCALAPPDATA%\Metallic-RC\avatar.json` and announced to the room. Older looks (only the first four fields) still
//! load: what is missing keeps the default.
use serde_json::{json, Value};

/// An enum of named choices with its list (`ALL`, the first is the default) and names.
macro_rules! choices {
    ($(#[$m:meta])* $name:ident { $($v:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
        pub enum $name {
            #[default]
            $($v),+
        }

        impl $name {
            pub const ALL: &[$name] = &[$($name::$v),+];

            pub fn name(self) -> &'static str {
                match self {
                    $($name::$v => $s),+
                }
            }

            pub fn from_name(s: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|a| a.name() == s)
            }
        }
    };
}

choices!(
    /// What the robot wears on its head.
    Accessory {
        None = "none",
        Antenna = "antenna",
        Headphones = "headphones",
        Halo = "halo",
        Cap = "cap",
        Beanie = "beanie",
        Ears = "ears",
        Sprout = "sprout",
        Goggles = "goggles",
    }
);

choices!(
    /// What the robot wears on its body.
    Outfit {
        None = "none",
        Scarf = "scarf",
        Backpack = "backpack",
        Bowtie = "bowtie",
    }
);

choices!(
    /// The shape of the eyes on the face screen.
    Eyes {
        Pill = "pill",
        Round = "round",
        Happy = "happy",
        Wide = "wide",
    }
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// the head shell, shoulders, hands and boots
    pub skin: [u8; 3],
    pub shirt: [u8; 3],
    /// eyes, mouth, chest light, cuffs and the accessories' glow
    pub accent: [u8; 3],
    pub accessory: Accessory,
    /// hips and legs
    pub pants: [u8; 3],
    /// the face screen
    pub visor: [u8; 3],
    pub eyes: Eyes,
    pub outfit: Outfit,
}

impl Default for Look {
    fn default() -> Self {
        // The Rec Room new-player default: a tan robot in the white jersey with the orange trim + chest logo.
        Look {
            skin: SKINS[1],    // tan
            shirt: SHIRTS[6],  // off-white jersey
            accent: ACCENTS[3], // orange
            accessory: Accessory::None,
            pants: PANTS[0],   // dark grey
            visor: VISORS[0],
            eyes: Eyes::Pill,
            outfit: Outfit::None,
        }
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
pub const PANTS: [[u8; 3]; 8] = [
    [0x3a, 0x3f, 0x4b], [0x26, 0x32, 0x4f], [0x3f, 0x6f, 0xb5], [0x5d, 0x6b, 0x3a],
    [0x5b, 0x3a, 0x6b], [0xc9, 0xb4, 0x8f], [0xe9, 0xe7, 0xe1], [0x1b, 0x1d, 0x22],
];
/// Dark tints, so the eyes stay bright on them.
pub const VISORS: [[u8; 3]; 8] = [
    [0x0f, 0x11, 0x17], [0x1a, 0x23, 0x33], [0x24, 0x1a, 0x2e], [0x10, 0x26, 0x1f],
    [0x2a, 0x14, 0x16], [0x1d, 0x1d, 0x1d], [0x0b, 0x1d, 0x2a], [0x2b, 0x24, 0x16],
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
        json!({"skin": hex(self.skin), "shirt": hex(self.shirt), "accent": hex(self.accent), "accessory": self.accessory.name(),
            "pants": hex(self.pants), "visor": hex(self.visor), "eyes": self.eyes.name(), "outfit": self.outfit.name()}).to_string()
    }

    /// Fields that are missing or unreadable keep the default's.
    pub fn from_json(s: &str) -> Option<Look> {
        let v: Value = serde_json::from_str(s).ok()?;
        let v = v.as_object()?;
        let mut l = Look::default();
        let colour = |k: &str| v.get(k).and_then(Value::as_str).and_then(parse_hex);
        for (k, c) in [("skin", &mut l.skin), ("shirt", &mut l.shirt), ("accent", &mut l.accent), ("pants", &mut l.pants), ("visor", &mut l.visor)] {
            if let Some(x) = colour(k) {
                *c = x;
            }
        }
        let name = |k: &str| v.get(k).and_then(Value::as_str);
        if let Some(a) = name("accessory").and_then(Accessory::from_name) {
            l.accessory = a;
        }
        if let Some(e) = name("eyes").and_then(Eyes::from_name) {
            l.eyes = e;
        }
        if let Some(o) = name("outfit").and_then(Outfit::from_name) {
            l.outfit = o;
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
            Some(a) => Accessory::from_name(a).ok_or_else(|| {
                let names: Vec<&str> = Accessory::ALL.iter().map(|a| a.name()).collect();
                format!("--look: accessory is one of {}, not {a:?}", names.join(", "))
            })?,
            None => Accessory::None,
        };
        Ok(Look { skin: c(0)?, shirt: c(1)?, accent: c(2)?, accessory, ..Look::default() })
    }

    /// A look picked from the palettes by the player's name, so players without a saved look still differ.
    pub fn for_name(name: &str) -> Look {
        // FNV-1a: stable across runs and machines
        let h = name.bytes().fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193));
        let pick = |shift: u32| (h >> shift) as usize % 8;
        let of = |shift: u32, n: usize| (h >> shift) as usize % n;
        Look {
            skin: SKINS[pick(0)],
            shirt: SHIRTS[pick(3)],
            accent: ACCENTS[pick(6)],
            accessory: Accessory::ALL[of(9, Accessory::ALL.len())],
            pants: PANTS[pick(13)],
            visor: VISORS[0],
            eyes: Eyes::ALL[of(16, Eyes::ALL.len())],
            outfit: Outfit::ALL[of(19, Outfit::ALL.len())],
        }
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
        let l = Look { skin: [1, 2, 3], shirt: [0xab, 0xcd, 0xef], accent: [255, 0, 128], accessory: Accessory::Halo, pants: [4, 5, 6],
            visor: [7, 8, 9], eyes: Eyes::Happy, outfit: Outfit::Scarf };
        let v: Value = serde_json::from_str(&l.to_json()).unwrap();
        assert_eq!(v, json!({"skin": "#010203", "shirt": "#abcdef", "accent": "#ff0080", "accessory": "halo", "pants": "#040506",
            "visor": "#070809", "eyes": "happy", "outfit": "scarf"}));
        assert_eq!(Look::from_json(&l.to_json()), Some(l));
        // unknown fields are ignored, missing or bad ones keep the default
        let partial = Look::from_json(r##"{"shirt":"#000000","accent":"blue","accessory":"crown","v":2}"##).unwrap();
        assert_eq!(partial, Look { shirt: [0, 0, 0], ..Look::default() });
        assert_eq!(Look::from_json("not json"), None);
        // a look saved before the new zones: its own four fields, the rest default, and it saves with all of them
        let old = Look::from_json(r##"{"skin":"#f2c99b","shirt":"#2b5fd9","accent":"#3df5ff","accessory":"headphones"}"##).unwrap();
        assert_eq!(old, Look { skin: [0xf2, 0xc9, 0x9b], shirt: [0x2b, 0x5f, 0xd9], accent: [0x3d, 0xf5, 0xff], accessory: Accessory::Headphones,
            ..Look::default() });
        assert_eq!(Look::from_json(&old.to_json()), Some(old));
        // a newer client's unknown choices fall back to the defaults
        let newer = Look::from_json(r##"{"eyes":"stars","outfit":"cape","accessory":"crown"}"##).unwrap();
        assert_eq!((newer.eyes, newer.outfit, newer.accessory), (Eyes::Pill, Outfit::None, Accessory::None));
        assert_eq!(Look::from_json("[1]"), None);
    }

    #[test]
    fn command_line_and_name_looks() {
        assert_eq!(Look::parse_arg("f2c99b, #2b5fd9,3DF5FF,headphones"),
            Ok(Look { skin: [0xf2, 0xc9, 0x9b], shirt: [0x2b, 0x5f, 0xd9], accent: [0x3d, 0xf5, 0xff], accessory: Accessory::Headphones,
                ..Look::default() }));
        assert_eq!(Look::parse_arg("000000,ffffff,808080,goggles").unwrap().accessory, Accessory::Goggles);
        assert_eq!(Look::parse_arg("000000,ffffff,808080").unwrap().accessory, Accessory::None);
        assert!(Look::parse_arg("000000,ffffff").is_err());
        assert!(Look::parse_arg("000000,ffffff,80808g").is_err());
        assert!(Look::parse_arg("000000,ffffff,808080,cape").is_err());
        assert!(Look::parse_arg("ü00000,ffffff,808080").is_err());
        assert_eq!(Look::for_name("Ada"), Look::for_name("Ada"));
        assert_ne!(Look::for_name("Ada"), Look::for_name("Bo"));
        // names spread over every choice
        let looks: Vec<Look> = (0..400).map(|i| Look::for_name(&format!("player{i}"))).collect();
        assert!(Accessory::ALL.iter().all(|a| looks.iter().any(|l| l.accessory == *a)));
        assert!(Eyes::ALL.iter().all(|e| looks.iter().any(|l| l.eyes == *e)));
        assert!(Outfit::ALL.iter().all(|o| looks.iter().any(|l| l.outfit == *o)));
    }
}
