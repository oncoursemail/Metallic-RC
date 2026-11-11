//! Prints what `levels` makes of a room: where each 2026 prefab goes, which old-scene pieces are drawn from the
//! reference install, and the scene's lighting.
//!
//! With `--load` it builds the whole room as the client does and prints the size of what it made instead.
//!
//! `cargo run --profile fast -p metallic-rc-client --example levels_report -- [room] [--load] [--ref-install DIR] [--game-dir DIR]`
use metallic_rc_client::{levels::{self, Piece}, room::{self, Library}, shapes};

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut room, mut game, mut reference, mut load) = ("dorm".to_string(), None, None, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ref-install" => reference = args.next(),
            "--game-dir" => game = args.next().map(std::path::PathBuf::from),
            "--load" => load = true,
            _ => room = a,
        }
    }
    levels::set_ref_install(reference.as_deref());
    let game = game.or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists())).expect("no game folder");
    let dir = room::bundle_dir(&game);
    let mut lib = Library::new(&dir).expect("bundles");
    if load {
        let mut kit = shapes::ShapeKit::new(shapes::old_build_dirs().iter().find(|d| d.exists()).map(|d| d.as_path()));
        let t = std::time::Instant::now();
        let r = levels::load(&mut lib, &mut kit, &dir, &room).expect("no such scene in the reference install").expect("load");
        let mb = |b: usize| b as f64 / 1048576.0;
        let verts: usize = r.parts.iter().map(|p| p.positions.len() * 32 + p.indices.len() * 4).sum();
        let tex = |t: &Option<shapes::Pixels>| t.as_ref().map_or(0, |t| t.2.len());
        let pixels: usize = r.materials.iter().map(|m| tex(&m.texture) + tex(&m.spec_map) + tex(&m.normal_map) + tex(&m.emissive_map)).sum();
        println!("{room}: {} parts ({:.0} MB of vertices), {} materials ({:.0} MB of pixels), {} lights, built in {:.1} s",
            r.parts.len(), mb(verts), r.materials.len(), mb(pixels), r.lights.len(), t.elapsed().as_secs_f32());
        for p in &r.problems {
            println!("problem: {p}");
        }
        return;
    }
    let placed = levels::layout(&mut lib, &dir, &room).expect("layout");
    let (mut prefabs, mut old, mut hidden) = (0, 0, 0);
    for p in &placed {
        let (s, r, t) = p.world.to_scale_rotation_translation();
        let what = match &p.piece {
            Piece::Prefab(n) => {
                prefabs += 1;
                format!("2026 {n}")
            }
            Piece::Hidden(n) => {
                hidden += 1;
                format!("hide {n}")
            }
            Piece::Mesh(i) => {
                old += 1;
                format!("2019 #{i}")
            }
        };
        println!("{what:40} {:8.3} {:8.3} {:8.3}  q {:6.3} {:6.3} {:6.3} {:6.3}  s {:5.2}  <- {}", t.x, t.y, t.z, r.x, r.y, r.z, r.w, s.x, p.from);
    }
    println!("{prefabs} prefabs placed, {hidden} left out, {old} old-scene pieces");
    match levels::lighting(&room) {
        Ok(l) => {
            println!("\nlighting of {}: ambient mode {} sky {:?} equator {:?} ground {:?} x{}", l.scene, l.ambient_mode, l.ambient_sky, l.ambient_equator, l.ambient_ground, l.ambient_intensity);
            println!("ambient SH dc {:?}", [l.ambient_sh[0][0], l.ambient_sh[1][0], l.ambient_sh[2][0]]);
            println!("fog {:?}", l.fog);
            println!("skybox {:?} shader {:?} cubemap {:?}", l.skybox.as_ref().map(|m| &m.name), l.skybox_shader, l.skybox_cubemap.as_ref().map(|c| (&c.name, c.size)));
            println!("reflection {:?} x{}", l.reflection.as_ref().map(|c| (&c.name, c.size)), l.reflection_intensity);
            for (i, x) in l.lights.iter().enumerate() {
                let (p, d) = (x.world.w_axis, -x.world.z_axis.truncate().normalize());
                println!("light {i}{} kind {} colour {:?} intensity {} range {} spot {} at ({:.2} {:.2} {:.2}) towards ({:.2} {:.2} {:.2}) shadows {}", if l.sun == Some(i) { " (sun)" } else { "" }, x.kind, x.color, x.intensity, x.range, x.spot_angle, p.x, p.y, p.z, d.x, d.y, d.z, x.shadows);
            }
            for p in &l.probes {
                println!("reflection probe at {:?} size {:?} box {} cubemap {:?}", p.world.w_axis.truncate(), p.size, p.box_projection, p.cubemap.as_ref().map(|c| (&c.name, c.size)));
            }
            println!("{} light probes, {} lightmaps, lightmap settings {:?}", l.light_probes.len(), l.lightmaps.len(), l.lightmap_settings.as_ref().map(|s| (s.baked_lightmaps, s.realtime_lightmaps, s.lightmaps.len())));
        }
        Err(e) => println!("lighting: {e}"),
    }
}
