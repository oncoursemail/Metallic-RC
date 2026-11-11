//! Audit: load every built-in corpus scene the way the client does, report stats or failure.
//! usage: rr-scene-audit [--game-dir DIR]
use std::path::PathBuf;
use metallic_rc_client::{levels, room};

fn main() {
    let mut a = std::env::args().skip(1);
    let mut game_dir: Option<PathBuf> = None;
    let mut only: Vec<String> = Vec::new();
    while let Some(x) = a.next() {
        if x == "--game-dir" {
            game_dir = a.next().map(PathBuf::from);
        } else {
            only.push(x);
        }
    }
    let game = game_dir
        .or_else(|| room::GAME_DIRS.iter().map(PathBuf::from).find(|p| p.exists()))
        .map(|g| g.join("RecRoom_Data"));
    let scenes = levels::scene_list();
    println!("scenes in corpus: {}", scenes.len());
    let (mut ok, mut empty, mut fail, mut skip) = (0, 0, 0, 0);
    for s in &scenes {
        if !only.is_empty() && !only.iter().any(|f| s.file.eq_ignore_ascii_case(f) || s.name.eq_ignore_ascii_case(f)) {
            continue;
        }
        let t = std::time::Instant::now();
        match levels::load_scene(game.as_deref(), &s.file) {
            Some(Ok(r)) => {
                let parts = r.parts.len();
                let verts: usize = r.parts.iter().map(|p| p.positions.len()).sum();
                if parts == 0 {
                    empty += 1;
                    println!("EMPTY {:<7} {:<30} (no drawn geometry)", s.file, s.name);
                } else {
                    ok += 1;
                    println!("OK    {:<7} {:<30} parts {:>5} verts {:>8} lights {:>4} colliders {:>5} spawns {:>3} ({:.1}s)",
                             s.file, s.name, parts, verts, r.lights.len(), r.colliders.len(), r.spawns.len(), t.elapsed().as_secs_f32());
                    if !only.is_empty() {
                        use metallic_rc_client::collide::Collider;
                        let (mut b, mut s, mut c, mut m, mut mt) = (0, 0, 0, 0, 0usize);
                        for col in &r.colliders {
                            match col {
                                Collider::Box(_) => b += 1,
                                Collider::Sphere { .. } => s += 1,
                                Collider::Capsule { .. } => c += 1,
                                Collider::Mesh(mesh) => { m += 1; mt += mesh.tris.len(); }
                            }
                        }
                        println!("      colliders: box {b}, sphere {s}, capsule {c}, mesh {m} ({mt} tris)");
                        let (lo, hi) = metallic_rc_client::world::bounds(&r);
                        println!("      bounds y {:.1}..{:.1}", lo.y, hi.y);
                        for (k, m) in r.spawns.iter().take(4).enumerate() {
                            let p = m.to_scale_rotation_translation().2;
                            println!("      spawn[{k}] ({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
                        }
                    }
                }
            }
            Some(Err(e)) => {
                fail += 1;
                println!("FAIL  {:<7} {:<30} {}", s.file, s.name, e);
            }
            None => {
                skip += 1;
                println!("SKIP  {:<7} {:<30} (not in corpus)", s.file, s.name);
            }
        }
    }
    println!("\nsummary: {ok} loaded with geometry, {empty} empty/internal, {fail} FAILED, {skip} skipped, of {}", scenes.len());
}
