//! Lists a player build's scenes (`levelN` -> scene path from BuildSettings) with their root objects.
//!
//! `unity-levels <RecRoom_Data dir> [level index ...] [--classes]`
//! Without indices every scene is listed; `--classes` adds each scene's object count per class; `--check` reads
//! every scene-level object (and every Mesh, Texture2D, Cubemap and Material of the scene's externals) with the
//! `level` readers and counts failures.

use std::collections::BTreeMap;
use std::path::PathBuf;
use unity_assets::*;

fn main() {
    let usage = "usage: unity-levels <data dir> [index ...] [--classes] [--check]";
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect(usage));
    let (mut only, mut show_classes, mut check) = (Vec::new(), false, false);
    for a in args {
        match a.as_str() {
            "--classes" => show_classes = true,
            "--check" => check = true,
            n => only.push(n.parse::<usize>().expect(usage)),
        }
    }
    let ggm = SerializedFile::parse(std::fs::read(dir.join("globalgamemanagers")).expect("globalgamemanagers")).expect("parse globalgamemanagers");
    let scenes = level::build_scenes(&ggm).expect("build settings");
    println!("Unity {}: {} scenes", ggm.unity_version, scenes.len());
    for (i, path) in scenes.iter().enumerate() {
        if !only.is_empty() && !only.contains(&i) {
            continue;
        }
        let file = dir.join(format!("level{i}"));
        let sf = match std::fs::read(&file).map_err(Error::from).and_then(SerializedFile::parse) {
            Ok(sf) => sf,
            Err(e) => {
                println!("level{i}\t{path}\t<{e}>");
                continue;
            }
        };
        let mut roots = Vec::new();
        for o in sf.objects.iter().filter(|o| o.class_id == classes::TRANSFORM || o.class_id == classes::RECT_TRANSFORM) {
            let Ok(t) = sf.transform(o) else { continue };
            if t.parent.is_null() {
                let name = sf.object(t.game_object.path_id).and_then(|g| sf.object_name(g)).unwrap_or_default();
                roots.push(name);
            }
        }
        println!("level{i}\t{path}\t{} objects\troots: {}", sf.objects.len(), roots.join(" | "));
        if check {
            check_file(&dir, &sf, true);
            for e in &sf.externals {
                let name = e.path.rsplit('/').next().unwrap_or(&e.path);
                if let Ok(x) = std::fs::read(dir.join(name)).map_err(Error::from).and_then(SerializedFile::parse) {
                    print!("  {name}: ");
                    check_file(&dir, &x, false);
                }
            }
        }
        if show_classes {
            let mut c: BTreeMap<i32, usize> = BTreeMap::new();
            for o in &sf.objects {
                *c.entry(o.class_id).or_default() += 1;
            }
            for (id, n) in c {
                println!("    {:>5} {}", n, classes::class_name(id).map_or_else(|| format!("Class{id}"), str::to_string));
            }
        }
    }
}

/// Reads every object the `level` readers know and prints counts per class, with the first error of each.
fn check_file(dir: &std::path::Path, sf: &SerializedFile, scene: bool) {
    let res = level::DirResources(dir.to_path_buf());
    let mut stats: BTreeMap<i32, (usize, usize, String)> = BTreeMap::new();
    for o in &sf.objects {
        let r: Option<std::result::Result<(), Error>> = match o.class_id {
            classes::LIGHT if scene => Some(level::light(sf, o).map(drop)),
            classes::RENDER_SETTINGS if scene => Some(level::render_settings(sf, o).map(drop)),
            classes::LIGHTMAP_SETTINGS if scene => Some(level::lightmap_settings(sf, o).map(drop)),
            classes::REFLECTION_PROBE if scene => Some(level::reflection_probe(sf, o).map(drop)),
            classes::BOX_COLLIDER if scene => Some(level::box_collider(sf, o).map(drop)),
            classes::MESH_RENDERER if scene => Some(sf.mesh_renderer(o).map(drop)),
            classes::LIGHT_PROBES => Some(level::light_probes(sf, o).map(drop)),
            classes::MESH => Some(level::mesh(sf, o, &res).map(drop)),
            classes::TEXTURE_2D | classes::CUBEMAP => Some(level::texture(sf, o, &res).map(drop)),
            classes::MATERIAL => Some(level::material(sf, o).map(drop)),
            _ => None,
        };
        let Some(r) = r else { continue };
        let e = stats.entry(o.class_id).or_default();
        match r {
            Ok(()) => e.0 += 1,
            Err(err) => {
                e.1 += 1;
                if e.2.is_empty() {
                    e.2 = format!("{} {}: {err}", o.path_id, sf.object_name(o).unwrap_or_default());
                }
            }
        }
    }
    let parts: Vec<String> = stats.iter().map(|(c, (ok, bad, first))| {
        let name = classes::class_name(*c).unwrap_or("?");
        if *bad > 0 { format!("{name} {ok} ok {bad} FAILED ({first})") } else { format!("{name} {ok} ok") }
    }).collect();
    println!("{}", parts.join(", "));
}
