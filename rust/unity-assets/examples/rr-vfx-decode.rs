//! THROWAWAY: prove the scene->bundle VFX name-match bridge: open a bundle, find each ParticleSystem, resolve its
//! GameObject name, and decode its spec with ParticleSystem::from_value (type-tree'd bundles).
//! usage: rr-vfx-decode <bundle-file> [name-substr]
use unity_assets::*;

fn main() {
    let path = std::env::args().nth(1).expect("bundle");
    let want = std::env::args().nth(2);
    let b = Bundle::open(&path).expect("open bundle");
    for (fname, sf) in b.serialized_files().expect("serialized files") {
        for o in sf.objects.iter().filter(|o| o.class_id == classes::PARTICLE_SYSTEM) {
            // the PS component's GameObject name
            let v = match sf.read_value(o) {
                Ok(v) => v,
                Err(e) => {
                    println!("  PS read_value failed in {fname}: {e}");
                    continue;
                }
            };
            let go_name = v.get("m_GameObject").and_then(|g| g.as_pptr()).and_then(|p| sf.object(p.path_id)).and_then(|go| sf.object_name(go)).unwrap_or_default();
            if let Some(w) = &want {
                if !go_name.contains(w.as_str()) {
                    continue;
                }
            }
            let ps = ParticleSystem::from_value(&v);
            println!(
                "{go_name:?}: len {:.3}s loop {} max {} rate {:.1} lifetime {:.3} speed {:.2} size {:.2} shape {} vel_enabled {} color_stops {}",
                ps.length, ps.looping, ps.max_particles, ps.rate_over_time.constant(), ps.start_lifetime.constant(),
                ps.start_speed.constant(), ps.start_size.constant(), ps.shape_type, ps.velocity_enabled,
                ps.color_over_lifetime.as_ref().map_or(0, |g| g.stops.len())
            );
        }
    }
}
