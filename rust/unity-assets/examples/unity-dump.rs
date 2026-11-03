//! Lists a bundle's objects and optionally exports one mesh (.obj) and one texture (.png).
//!
//! `unity-dump <bundle> [--out DIR] [--mesh PATHID|first] [--texture PATHID|first] [--container]`
//! With `--out` and no `--mesh`/`--texture`, the first of each is exported.

use std::io::Write;
use std::path::PathBuf;
use unity_assets::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: unity-dump <bundle> [--out DIR] [--mesh PATHID|first] [--texture PATHID|first] [--container]";
    let path = PathBuf::from(args.next().expect(usage));
    let (mut out, mut mesh, mut tex, mut container) = (None, None, None, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--mesh" => mesh = args.next(),
            "--texture" => tex = args.next(),
            "--container" => container = true,
            _ => panic!("{usage}"),
        }
    }
    if out.is_some() {
        mesh.get_or_insert("first".into());
        tex.get_or_insert("first".into());
    }
    let b = Bundle::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let h = &b.header;
    println!("{} format {} flags {:#x} blocks {:?}", h.signature, h.format, h.flags, b.block_compression());
    for n in &b.nodes {
        println!("node {:>10} flags {} {}", n.size, n.flags, n.path);
    }
    for (name, sf) in b.serialized_files().expect("serialized file") {
        println!("\n{name}: version {} type trees {} objects {} externals {}", sf.version, sf.type_tree_enabled, sf.objects.len(), sf.externals.len());
        for e in &sf.externals {
            println!("  external {}", e.path);
        }
        println!("  {:>20} {:>6} {:24} name", "path id", "class", "type");
        for o in &sf.objects {
            let tn = sf.type_name(o);
            let name = sf.object_name(o).unwrap_or_default();
            println!("  {:>20} {:>6} {:24} {}", o.path_id, o.class_id, tn, name);
            if container && tn == "AssetBundle" {
                if let Ok(a) = sf.read_value(o).and_then(|v| AssetBundleInfo::from_value(&v)) {
                    println!("    scene bundle: {}", a.is_scene);
                    for (p, ptr) in &a.container {
                        println!("    {p} -> {}:{}", ptr.file_id, ptr.path_id);
                    }
                }
            }
        }
        let Some(dir) = &out else { continue };
        std::fs::create_dir_all(dir).unwrap();
        let pick = |want: &Option<String>, class: i32| -> Option<&ObjectInfo> {
            match want.as_deref()? {
                "first" => sf.objects.iter().find(|o| o.class_id == class),
                id => sf.object(id.parse().ok()?),
            }
        };
        if let Some(o) = pick(&mesh, classes::MESH) {
            let m = sf.read_value(o).and_then(|v| Mesh::from_value(&v, &b));
            match m {
                Ok(m) => {
                    let file = dir.join(format!("{}_{}.obj", safe(&m.name), o.path_id));
                    write_obj(&m, &file).unwrap();
                    println!("\nmesh {} ({} vertices, {} submeshes) -> {}", m.name, m.vertex_count, m.submeshes.len(), file.display());
                }
                Err(e) => println!("\nmesh {}: {e}", o.path_id),
            }
        }
        if let Some(o) = pick(&tex, classes::TEXTURE_2D) {
            let t = sf.read_value(o).and_then(|v| Texture2D::from_value(&v, &b));
            match t.and_then(|t| t.decode_rgba8().map(|px| (t, px))) {
                Ok((t, px)) => {
                    let file = dir.join(format!("{}_{}.png", safe(&t.name), o.path_id));
                    let mut enc = png::Encoder::new(std::fs::File::create(&file).unwrap(), t.width as u32, t.height as u32);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    enc.write_header().unwrap().write_image_data(&px).unwrap();
                    let f = texture::format_name(t.format);
                    println!("texture {} {}x{} {f} mips {} -> {}", t.name, t.width, t.height, t.mip_count, file.display());
                }
                Err(e) => println!("texture {}: {e}", o.path_id),
            }
        }
    }
}

fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Wavefront OBJ; Unity is left-handed, so x is mirrored and triangle winding reversed.
fn write_obj(m: &Mesh, file: &std::path::Path) -> std::io::Result<()> {
    let mut w = std::io::BufWriter::new(std::fs::File::create(file)?);
    let (uv, nrm) = (m.uv(0), m.normals());
    for p in m.positions() {
        writeln!(w, "v {} {} {}", -p[0], p[1], p[2])?;
    }
    for t in &uv {
        writeln!(w, "vt {} {}", t[0], t[1])?;
    }
    for n in &nrm {
        writeln!(w, "vn {} {} {}", -n[0], n[1], n[2])?;
    }
    for (i, s) in m.submeshes.iter().enumerate() {
        if s.topology != 0 {
            continue;
        }
        writeln!(w, "g submesh_{i}")?;
        for t in m.submesh_indices(i).unwrap_or_default().chunks_exact(3) {
            let f = |x: u32| match (uv.is_empty(), nrm.is_empty()) {
                (true, true) => format!("{}", x + 1),
                (false, true) => format!("{0}/{0}", x + 1),
                (true, false) => format!("{0}//{0}", x + 1),
                (false, false) => format!("{0}/{0}/{0}", x + 1),
            };
            writeln!(w, "f {} {} {}", f(t[2]), f(t[1]), f(t[0]))?;
        }
    }
    Ok(())
}
