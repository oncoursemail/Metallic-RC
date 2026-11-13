//! Inventory the REAL level6 dorm scene for the fidelity gap analysis: spawn, lights, materials, and the parts
//! (names + world centroids) so we can see the furniture/props and the lighting the real assets carry.
//! usage: rr-dorm-probe [scene]   (default "dorm")
use std::collections::BTreeMap;

use bevy::math::{Mat4, Vec3};
use metallic_rc_client::{levels, room};

fn centroid(world: Mat4, pos: &[[f32; 3]]) -> Vec3 {
    if pos.is_empty() {
        return Vec3::ZERO;
    }
    let s: Vec3 = pos.iter().fold(Vec3::ZERO, |a, p| a + world.transform_point3(Vec3::from(*p)));
    s / pos.len() as f32
}

fn main() {
    let scene = std::env::args().nth(1).unwrap_or_else(|| "dorm".into());
    let data = room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists());
    let r = levels::load_scene(data.as_deref(), &scene).expect("scene not in corpus").expect("load failed");
    println!("=== {scene}: {} parts, {} materials, {} lights, {} spawns ===", r.parts.len(), r.materials.len(), r.lights.len(), r.spawns.len());

    for (i, s) in r.spawns.iter().enumerate() {
        let t = s.w_axis.truncate();
        println!("spawn #{i}: pos=[{:.2} {:.2} {:.2}]", t.x, t.y, t.z);
    }
    println!("\nLIGHTS:");
    for l in &r.lights {
        let p = l.world.w_axis.truncate();
        let kind = ["spot", "directional", "point", "area"].get(l.kind as usize).copied().unwrap_or("?");
        println!("  {kind:<11} pos=[{:.1} {:.1} {:.1}] intensity={:.2} range={:.1} color=[{:.2} {:.2} {:.2}] shadows={}", p.x, p.y, p.z, l.intensity, l.range, l.color[0], l.color[1], l.color[2], l.shadows);
    }

    // materials: name -> shader + colour, to spot missing/plain materials
    println!("\nMATERIALS ({}):", r.materials.len());
    for m in &r.materials {
        println!("  {:<42} shader={:<28} color=[{:.2} {:.2} {:.2} {:.2}] tex={} emis={:.2} blend={:?}", m.name, m.shader, m.color[0], m.color[1], m.color[2], m.color[3], m.texture.is_some() as u8, m.emissive_power, m.blend);
    }

    // parts grouped by name prefix, with count + a representative centroid + bbox
    println!("\nPARTS by name:");
    let mut by: BTreeMap<String, (usize, Vec3, Vec3, Vec3)> = BTreeMap::new();
    for p in &r.parts {
        let c = centroid(p.world, &p.positions);
        let key = p.name.clone();
        let e = by.entry(key).or_insert((0, Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY), Vec3::ZERO));
        e.0 += 1;
        e.1 = e.1.min(c);
        e.2 = e.2.max(c);
        e.3 += c;
    }
    for (name, (n, lo, hi, sum)) in &by {
        let c = *sum / *n as f32;
        println!("  {name:<40} x{:<3} center=[{:.1} {:.1} {:.1}] span=[{:.1} {:.1} {:.1}]", n, c.x, c.y, c.z, hi.x - lo.x, hi.y - lo.y, hi.z - lo.z);
    }
    println!("\ntotal part names: {}", by.len());
    if !r.problems.is_empty() {
        println!("\nPROBLEMS ({}): {:?}", r.problems.len(), &r.problems[..r.problems.len().min(12)]);
    }
}
