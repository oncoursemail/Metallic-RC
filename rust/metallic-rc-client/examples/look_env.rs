//! Prints a room's ambient SH and cubemap statistics from the reference install (levels::lighting), and with an
//! output folder writes the cubemap faces there as PPM files (for looking at them; never commit them).
//! `cargo run --profile fast -p metallic-rc-client --example look_env -- [room] [out dir]`
use metallic_rc_client::levels;

fn main() {
    let mut args = std::env::args().skip(1);
    let room = args.next().unwrap_or_else(|| "dorm".into());
    let out = args.next();
    levels::set_ref_install(None);
    let t = std::time::Instant::now();
    let l = levels::lighting(&room).unwrap_or_else(|e| panic!("{e}"));
    println!("lighting of {} in {:.2} s", l.scene, t.elapsed().as_secs_f32());
    for (c, name) in ["r", "g", "b"].iter().enumerate() {
        println!("sh {name}: {:?}", l.ambient_sh[c]);
    }
    if let Some(s) = &l.skybox {
        println!("skybox {} {}: colors {:?} floats {:?}", s.name, l.skybox_shader, s.colors, s.floats);
    }
    for (name, cube) in [("sky", &l.skybox_cubemap), ("reflection", &l.reflection)] {
        let Some(cube) = cube else { continue };
        for (i, px) in cube.faces.iter().enumerate() {
            let (w, n) = (cube.size, px.len());
            let mean: Vec<f32> = (0..3).map(|c| px.iter().map(|p| p[c]).sum::<f32>() / n as f32).collect();
            let max = px.iter().flatten().fold(0f32, |a, &b| a.max(b));
            println!("{name} {} face {i}: {w}x{w} mean {mean:?} max {max}", cube.name);
            if let Some(dir) = &out {
                // linear to sRGB, clamped
                let s = |v: f32| (if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let mut f = format!("P6 {w} {w} 255\n").into_bytes();
                f.extend(px.iter().flat_map(|p| [s(p[0]), s(p[1]), s(p[2])]));
                std::fs::write(format!("{dir}/{name}_{i}.ppm"), f).unwrap();
            }
        }
    }
}
