//! Metallic-RC client library: the network side and the room loader, shared by the game binary and its tests.
pub mod audio;
pub mod bowling;
pub mod cache;
pub mod charades;
pub mod avatar;
pub mod collide;
pub mod discgolf;
pub mod dodgeball;
pub mod drive;
pub mod games;
pub mod interact;
pub mod kiwii_net;
pub mod levels;
pub mod maker;
pub mod mirror;
pub mod net;
pub mod netobj;
pub mod paddleball;
pub mod particles;
pub mod perf;
pub mod physics;
pub mod platform;
pub mod player;
pub mod quality;
pub mod realtime;
pub mod render;
pub mod room;
pub mod settings;
pub mod shapes;
pub mod soccer;
pub mod social;
pub mod stuntrunner;
pub mod texprep;
pub mod ui;
pub mod voice;
pub mod world;

/// A float from an env var, for dialling values in without a rebuild (e.g. `METALLIC_TITLE_CAM_DIST`); falls back to `d`.
pub fn envf(key: &str, d: f32) -> f32 {
    std::env::var(key).ok().and_then(|s| s.parse().ok()).unwrap_or(d)
}
