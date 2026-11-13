//! Lists Addressables catalog keys -> bundle file, internal id and resource type.
//!
//! `unity-catalog <catalog.bin> [substring filter]`

use unity_assets::catalog::{bundle_file, Catalog};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: unity-catalog <catalog.bin> [filter]");
    let filter = args.next().map(|f| f.to_lowercase());
    let cat = Catalog::open(&path).expect("catalog");
    let entries = cat.entries().expect("entries");
    let locations: usize = entries.iter().map(|e| e.locations.len()).sum();
    println!("catalog v{}: {} keys, {} locations", cat.version, entries.len(), locations);
    for e in &entries {
        for l in &e.locations {
            let line = format!(
                "{}\t{}\t{}\t{}\t{}",
                e.key,
                bundle_file(l).unwrap_or("-"),
                l.internal_id,
                l.resource_type,
                l.provider.rsplit('.').next().unwrap_or("")
            );
            if filter.as_ref().is_none_or(|f| line.to_lowercase().contains(f)) {
                println!("{line}");
            }
        }
    }
}
