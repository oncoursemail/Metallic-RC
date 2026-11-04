//! Times loading worlds the way the client's loader thread does (a fresh Library each), for the load-time numbers.
//! `cargo run --profile fast -p metallic-rc-client --example world_time [world ...]`
use metallic_rc_client::{room::{self, Library}, world};

fn main() {
    let mut worlds: Vec<String> = std::env::args().skip(1).collect();
    if worlds.is_empty() {
        worlds = ["dorm", "sandbox", "(Charades_InkSpace_MainHull_Sandbox)Variant"].map(String::from).to_vec();
    }
    let game = room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()).expect("no game folder");
    let dir = room::bundle_dir(&game);
    for w in &worlds {
        let t = std::time::Instant::now();
        let mut lib = Library::new(&dir).unwrap();
        let index = t.elapsed();
        match world::load(&mut lib, &dir, Some(w)) {
            Ok(r) => {
                let sounds: Vec<String> = r.sounds.iter().map(|s| format!("{} ({})", s.clip.name, s.clip.codec)).collect();
                let unplayable: Vec<&String> = r.problems.iter().filter(|p| p.starts_with("audio")).collect();
                println!("{w}: index {:.2} s, load {:.2} s ({} roots, {} parts, {} materials, sounds {sounds:?}, unplayable {unplayable:?})",
                    index.as_secs_f32(), (t.elapsed() - index).as_secs_f32(), r.roots.len(), r.parts.len(), r.materials.len())
            }
            Err(e) => println!("{w}: {e}"),
        }
    }
}
