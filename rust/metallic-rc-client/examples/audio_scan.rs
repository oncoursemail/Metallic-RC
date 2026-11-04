//! Counts the addressable AudioClips by Unity compression format and FSB5 codec, and decodes a few.
//! `cargo run --profile fast -p metallic-rc-client --example audio_scan [address filter] [limit]`
use std::collections::BTreeMap;

use metallic_rc_client::{audio, room::{self, Library}, world::Addresses};

fn main() {
    let mut args = std::env::args().skip(1);
    let filter = args.next().unwrap_or_default();
    let limit: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(usize::MAX);
    let game = room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()).expect("no game folder");
    let dir = room::bundle_dir(&game);
    let addresses = Addresses::open(&dir).unwrap();
    let mut lib = Library::new(&dir).unwrap();
    let mut clips: Vec<_> = addresses.by_key.iter().filter(|(k, l)| l.resource_type == "UnityEngine.AudioClip" && k.contains(&filter)).collect();
    clips.sort_by(|a, b| (&a.1.bundle, a.0).cmp(&(&b.1.bundle, b.0)));
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let t = std::time::Instant::now();
    for (key, loc) in clips.into_iter().take(limit) {
        lib.forget();
        match audio::load_clip(&mut lib, &dir, loc) {
            Ok(c) => {
                *counts.entry(format!("{} ok", c.codec)).or_default() += 1;
                if filter.len() > 0 {
                    println!("{key}: {} Hz x{} {:.2} s ({})", c.rate, c.channels, c.samples.len() as f32 / c.channels.max(1) as f32 / c.rate.max(1) as f32, c.codec);
                }
            }
            Err(e) => {
                *counts.entry(format!("error: {}", e.split(':').next().unwrap_or(&e))).or_default() += 1;
                if filter.len() > 0 {
                    println!("{key}: {e}");
                }
            }
        }
    }
    println!("{counts:#?} in {:?}", t.elapsed());
}
