//! THROWAWAY: report reflection-probe / skybox / reflection cubemap decode for a 2026 corpus scene.
//! usage: rr-reflect-probe <scene> [--game-dir DIR]
use std::path::PathBuf;
use metallic_rc_client::levels;

fn main() {
    let mut a = std::env::args().skip(1);
    let scene = a.next().expect("scene");
    let mut game_dir: Option<PathBuf> = None;
    while let Some(x) = a.next() {
        if x == "--game-dir" {
            game_dir = a.next().map(PathBuf::from);
        }
    }
    let game = game_dir
        .or_else(|| metallic_rc_client::room::GAME_DIRS.iter().map(PathBuf::from).find(|p| p.exists()))
        .map(|g| g.join("RecRoom_Data"));
    let l = levels::scene_lighting(game.as_deref(), &scene).expect("scene_lighting failed");
    println!("scene {} ambient_mode {} intensity {}", l.scene, l.ambient_mode, l.ambient_intensity);
    println!("skybox_shader {:?}", l.skybox_shader);
    match &l.skybox_cubemap {
        Some(c) => println!("skybox_cubemap {:?} size {} faces {}", c.name, c.size, c.faces.len()),
        None => println!("skybox_cubemap: none"),
    }
    match &l.reflection {
        Some(c) => println!("custom reflection {:?} size {} faces {}", c.name, c.size, c.faces.len()),
        None => println!("custom reflection: none"),
    }
    println!("reflection probes: {}", l.probes.len());
    for (i, p) in l.probes.iter().enumerate() {
        match &p.cubemap {
            Some(c) => println!("  [{i}] {:?} size {} faces {} intensity {} boxproj {} importance {}", c.name, c.size, c.faces.len(), p.intensity, p.box_projection, p.importance),
            None => println!("  [{i}] cubemap: NONE (decode failed) intensity {} importance {}", p.intensity, p.importance),
        }
    }
    println!("light_probes (SH): {}", l.light_probes.len());
    let out = std::env::var("RR_OUT").unwrap_or_else(|_| ".".into());
    if let Some(c) = &l.skybox_cubemap {
        write_strip(&format!("{out}/{scene}_skybox.png"), c);
    }
    if let Some(c) = &l.reflection {
        write_strip(&format!("{out}/{scene}_reflection.png"), c);
    }
}

/// Write a cubemap as a horizontal 6-face strip (+X -X +Y -Y +Z -Z), RGB.
fn write_strip(path: &str, c: &metallic_rc_client::levels::Cubemap) {
    let n = c.size as usize;
    let (w, h) = (n * 6, n);
    let mut rgb = vec![0u8; w * h * 3];
    for (fi, (fw, fh, px)) in c.faces.iter().enumerate() {
        let (fw, fh) = (*fw as usize, *fh as usize);
        for y in 0..fh.min(n) {
            for x in 0..fw.min(n) {
                let s = (y * fw + x) * 4;
                let d = (y * w + fi * n + x) * 3;
                if let (Some(src), Some(dst)) = (px.get(s..s + 3), rgb.get_mut(d..d + 3)) {
                    dst.copy_from_slice(src);
                }
            }
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    println!("wrote {path} ({w}x{h})");
}
