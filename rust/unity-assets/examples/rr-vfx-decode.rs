//! Decode ParticleSystems. Two modes:
//!   rr-vfx-decode bundle <bundle-file> [name-substr]        -- decode PS in a type-tree'd bundle
//!   rr-vfx-decode scene  <levelN-file> <bundle-for-tree>    -- inject the bundle's PS(198) tree and decode the
//!                                                              inline PS of a no-type-tree corpus scene (cipher route)
use unity_assets::*;

fn print_ps(name: &str, ps: &ParticleSystem) {
    println!(
        "{name:?}: len {:.3}s loop {} max {} rate {:.1} lifetime {:.3} speed {:.2} size {:.2} shape {} vel {} color_stops {}",
        ps.length, ps.looping, ps.max_particles, ps.rate_over_time.constant(), ps.start_lifetime.constant(),
        ps.start_speed.constant(), ps.start_size.constant(), ps.shape_type, ps.velocity_enabled,
        ps.color_over_lifetime.as_ref().map_or(0, |g| g.stops.len())
    );
}

fn main() {
    let mut a = std::env::args().skip(1);
    let mode = a.next().expect("mode: bundle|scene");
    if mode == "bundle" {
        let path = a.next().expect("bundle");
        let want = a.next();
        let b = Bundle::open(&path).expect("open bundle");
        for (fname, sf) in b.serialized_files().expect("serialized files") {
            for o in sf.objects.iter().filter(|o| o.class_id == classes::PARTICLE_SYSTEM) {
                let Ok(v) = sf.read_value(o) else { continue };
                let go_name = v.get("m_GameObject").and_then(|g| g.as_pptr()).and_then(|p| sf.object(p.path_id)).and_then(|go| sf.object_name(go)).unwrap_or_default();
                let _ = fname;
                if want.as_ref().is_some_and(|w| !go_name.contains(w.as_str())) {
                    continue;
                }
                print_ps(&go_name, &ParticleSystem::from_value(&v));
            }
        }
    } else {
        let scene_path = a.next().expect("scene file");
        let bundle_path = a.next().expect("bundle for the PS tree");
        // once: the PS(198) type tree from any bundle that has a ParticleSystem
        let b = Bundle::open(&bundle_path).expect("open bundle");
        let ps_tree = b.serialized_files().expect("sf").into_iter().flat_map(|(_, sf)| sf.types)
            .find(|t| t.class_id == classes::PARTICLE_SYSTEM && t.tree.is_some()).and_then(|t| t.tree).expect("no PS tree in bundle");
        let scene = SerializedFile::parse(std::fs::read(&scene_path).unwrap()).unwrap();
        let (mut ok, mut failed, mut room, mut emote) = (0, 0, 0, 0);
        for o in scene.objects.iter().filter(|o| o.class_id == classes::PARTICLE_SYSTEM) {
            let mut r = Reader::new(scene.object_bytes(o), scene.big_endian);
            match unity_assets::value::read(&ps_tree, &scene.ref_types, &mut r) {
                Ok(v) => {
                    ok += 1;
                    let go_name = v.get("m_GameObject").and_then(|g| g.as_pptr()).and_then(|p| scene.object(p.path_id)).and_then(|go| scene.object_name(go)).unwrap_or_default();
                    if go_name.starts_with("VFX_Emote") {
                        emote += 1;
                    } else {
                        room += 1;
                        if room <= 12 {
                            print_ps(&go_name, &ParticleSystem::from_value(&v));
                        }
                    }
                }
                Err(e) => {
                    if failed == 0 {
                        println!("  FAILED: {e}");
                    }
                    failed += 1;
                }
            }
        }
        println!("\ninline scene PS: {ok} decoded, {failed} failed; {room} room-ambiance, {emote} emote (filtered)");
    }
}
