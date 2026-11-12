//! Dump the raw Rec Room face sprites (eyes + mouths) to PNG so we can see what the game authored.
//! usage: cargo run --profile fast -p metallic-rc-client --example dump_face -- <out dir>
use std::{fs::File, io::BufWriter, path::PathBuf};

use unity_assets::{classes, Bundle, Texture2D};

const BUNDLE_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711\RecRoom_Data\StreamingAssets\aa\StandaloneWindows64";
const EYE_BUNDLE: &str = "84554899b3243253ba56b4d0262997b6.bundle";
const MOUTH_BUNDLE: &str = "8fcb685a7a49de356df5b83ada6b0c86.bundle";

fn save_rgba(path: &PathBuf, w: u32, h: u32, data: &[u8]) {
    let f = BufWriter::new(File::create(path).unwrap());
    let mut e = png::Encoder::new(f, w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(data).unwrap();
}

/// The client's face_decal, composited over a light-gray bg so the black ink is visible.
fn decal_on_gray(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let lum = (px[i * 4] as u32 + px[i * 4 + 1] as u32 + px[i * 4 + 2] as u32) / 3;
        let a = ((255 - lum.min(255)) * px[i * 4 + 3] as u32 / 255) as u8; // ink alpha
        // black ink over light gray (0xcc)
        let bg = 0xccu32;
        let v = (bg * (255 - a as u32) / 255) as u8;
        out[i * 4] = v;
        out[i * 4 + 1] = v;
        out[i * 4 + 2] = v;
        out[i * 4 + 3] = 255;
    }
    out
}

fn dump(bundle: &str, out: &PathBuf) {
    let path = PathBuf::from(BUNDLE_DIR).join(bundle);
    let b = Bundle::open(&path).expect("open bundle");
    let files = b.serialized_files().expect("serialized files");
    for (_, sf) in &files {
        for o in sf.objects.iter().filter(|o| o.class_id == classes::TEXTURE_2D) {
            let Some(name) = sf.object_name(o) else { continue };
            let Ok(v) = sf.read_value(o) else { continue };
            let Ok(t) = Texture2D::from_value(&v, &b) else { continue };
            let Ok(px) = t.decode_rgba8() else {
                println!("  {name}: decode failed");
                continue;
            };
            let (w, h) = (t.width as usize, t.height as usize);
            if px.len() < w * h * 4 {
                println!("  {name}: short buffer {}x{}", w, h);
                continue;
            }
            println!("  {name}: {w}x{h}");
            save_rgba(&out.join(format!("{name}.png")), w as u32, h as u32, &px);
            let g = decal_on_gray(w, h, &px);
            save_rgba(&out.join(format!("{name}__decal.png")), w as u32, h as u32, &g);
        }
    }
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "dump_face".into()));
    std::fs::create_dir_all(&out).unwrap();
    println!("eyes:");
    dump(EYE_BUNDLE, &out);
    println!("mouths:");
    dump(MOUTH_BUNDLE, &out);
    println!("done -> {}", out.display());
}
