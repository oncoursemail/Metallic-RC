//! Prints the components of an addressable prefab whose class or script name contains a filter, as value trees.
//! `cargo run --profile fast -p metallic-rc-client --example world_dump -- <address> <filter> [limit]`
use metallic_rc_client::{room::{self, Library}, world};

fn main() {
    let mut args = std::env::args().skip(1);
    let (address, filter) = (args.next().expect("address"), args.next().unwrap_or_default());
    let limit = args.next().and_then(|n| n.parse().ok()).unwrap_or(5);
    let game = room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()).expect("no game folder");
    let dir = room::bundle_dir(&game);
    let mut lib = Library::new(&dir).unwrap();
    for (go, name, v) in world::dump_components(&mut lib, &dir, &address, &filter, limit).unwrap() {
        println!("== {go} / {name}\n{v:#?}");
    }
}
