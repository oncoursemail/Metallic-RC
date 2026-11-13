//! Lists every addressable GameObject that is not UI, with its size and what it holds, to find what rooms the
//! bundles can build. Writes TSV to stdout: address, bundle, size x/y/z (m), renderers, objects, bounds min, bounds max, components.
//! `cargo run --profile fast -p metallic-rc-client --example world_inventory [game dir] [address filter]`
use metallic_rc_client::{room::{self, Library}, world::{self, Addresses}};

fn main() {
    let mut args = std::env::args().skip(1);
    let game = args.next().filter(|a| !a.is_empty()).map(std::path::PathBuf::from)
        .or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()))
        .expect("no game folder");
    let filter = args.next().unwrap_or_default();
    let dir = room::bundle_dir(&game);
    let t = std::time::Instant::now();
    let addresses = Addresses::open(&dir).unwrap();
    let mut lib = Library::new(&dir).unwrap();
    eprintln!("catalog {} keys, {} serialized files, {:?}", addresses.by_key.len(), lib.bundle_count(), t.elapsed());
    let mut list = addresses.game_objects();
    list.retain(|(k, _)| k.contains(&filter));
    // one bundle at a time, so only its dependencies stay open
    list.sort_by(|a, b| (&a.1.bundle, a.0).cmp(&(&b.1.bundle, b.0)));
    let mut last = String::new();
    for (key, loc) in list {
        if loc.bundle != last {
            lib.forget();
            last = loc.bundle.clone();
        }
        match world::summarize(&mut lib, &dir, loc) {
            Ok(s) if s.ui => {}
            Ok(s) => {
                let size = s.size();
                let comps: Vec<String> = s.components.iter().map(|(n, c)| format!("{n}:{c}")).collect();
                let (lo, hi) = if s.renderers > 0 { (s.lo, s.hi) } else { Default::default() };
                println!("{key}\t{}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{:.2},{:.2},{:.2}\t{:.2},{:.2},{:.2}\t{}", loc.bundle, size.x, size.y, size.z, s.renderers, s.objects,
                    lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, comps.join(" "));
            }
            Err(e) => eprintln!("{key}: {e}"),
        }
    }
    eprintln!("done in {:?}", t.elapsed());
}
