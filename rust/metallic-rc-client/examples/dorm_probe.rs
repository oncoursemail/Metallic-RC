//! Loads the Dorm prefabs from the installed game without opening a window and prints what was found.
//! `cargo run --release -p metallic-rc-client --example dorm_probe [game dir]`
use metallic_rc_client::{collide, room::{self, Library}, shapes::{self, ShapeKit}};

fn main() {
    let game = std::env::args().nth(1).map(std::path::PathBuf::from)
        .or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()))
        .expect("no game folder");
    let dir = room::bundle_dir(&game);
    let t = std::time::Instant::now();
    let mut lib = Library::new(&dir).unwrap();
    println!("indexed {} serialized files in {:?}", lib.bundle_count(), t.elapsed());
    let t = std::time::Instant::now();
    let mut kit = ShapeKit::new(shapes::old_build_dirs().iter().find(|d| d.exists()).map(|d| d.as_path()));
    println!("palette loaded in {:?}, Maker Pen textures: {}", t.elapsed(), kit.has_textures());
    let t = std::time::Instant::now();
    let r = room::load_prefabs(&mut lib, &mut kit, &dir, "43ea1176ca25c0333547813ec3298808.bundle", |n| n.starts_with("(Dorm_"), &[]).unwrap();
    println!("{} prefabs, {} parts, {} materials ({} textured), {} colliders in {:?}", r.roots.len(), r.parts.len(), r.materials.len(),
        r.materials.iter().filter(|m| m.texture.is_some()).count(), r.colliders.len(), t.elapsed());
    for m in &r.materials {
        println!("material {:56} {:32} smoothness {:.2} color {:?} texture {:?}", m.name, m.shader, m.smoothness,
            m.color.map(|c| (c * 100.0).round() / 100.0), m.texture.as_ref().map(|t| (t.0, t.1)));
    }
    for p in r.roots.iter().take(80) {
        print!("{p}  ");
    }
    println!();
    // world-space bounds of each prefab, to see which are placed and which sit at the origin
    for (i, name) in r.roots.iter().enumerate() {
        let (mut lo, mut hi) = (bevy::math::Vec3::splat(f32::MAX), bevy::math::Vec3::splat(f32::MIN));
        for part in r.parts.iter().filter(|p| p.root == i) {
            for &v in &part.positions {
                let w = part.world.transform_point3(v.into());
                lo = lo.min(w);
                hi = hi.max(w);
            }
        }
        if lo.x <= hi.x {
            println!("{name:34} x {:>6.2}..{:>6.2}  y {:>6.2}..{:>6.2}  z {:>6.2}..{:>6.2}", lo.x, hi.x, lo.y, hi.y, lo.z, hi.z);
        }
    }
    for part in r.parts.iter().take(0) {
        let (_, _, pos) = part.world.to_scale_rotation_translation();
        println!("{:40} at {:>7.2} {:>7.2} {:>7.2}  {} verts  mat {}", part.name, pos.x, pos.y, pos.z, part.positions.len(), r.materials[part.material].name);
    }
    // drop the player in the middle of the floor, then walk in four directions until something stops them
    let start = bevy::math::Vec3::new(1.0, 1.5, -2.0);
    let mut feet = start;
    for _ in 0..120 {
        feet = collide::move_capsule(&r.colliders, feet, bevy::math::Vec3::new(0.0, -0.05, 0.0)).0;
    }
    println!("landed at {feet}");
    for dir in [bevy::math::Vec3::X, -bevy::math::Vec3::X, bevy::math::Vec3::Z, -bevy::math::Vec3::Z] {
        let mut p = feet;
        for _ in 0..400 {
            p = collide::move_capsule(&r.colliders, p, dir * 0.05 - bevy::math::Vec3::Y * 0.05).0;
        }
        println!("walking {dir}: stopped at {p}");
    }
    for p in r.problems.iter().take(20) {
        println!("problem: {p}");
    }
}
