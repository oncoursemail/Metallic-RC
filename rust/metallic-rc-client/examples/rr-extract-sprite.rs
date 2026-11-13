//! Extract the real default face-feature SPRITE textures from the install bundles to PNG (1:1, no guessing).
//! For each FB_ feature: read its DefaultSprites[0] PPtr, resolve the Sprite across bundles, decode the referenced
//! Texture2D, crop to the sprite's textureRect, and write a PNG. usage: rr-extract-sprite <bundle-dir> <out-dir>
use std::collections::HashMap;
use std::io::BufWriter;
use std::path::PathBuf;

use unity_assets::{Bundle, SerializedFile, Texture2D, Value};

/// Map every serialized-file CAB name -> the .bundle that holds it (reading only bundle heads).
fn cab_index(dir: &PathBuf) -> HashMap<String, PathBuf> {
    let mut m = HashMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("bundle") {
            continue;
        }
        if let Ok(b) = Bundle::open_head(&p) {
            for n in b.nodes.iter().filter(|n| n.is_serialized_file()) {
                m.entry(n.path.clone()).or_insert_with(|| p.clone());
            }
        }
    }
    m
}

/// Open a bundle and return its serialized files, caching by path.
fn open_bundle<'a>(cache: &'a mut HashMap<PathBuf, Vec<(String, SerializedFile)>>, path: &PathBuf) -> Option<&'a Vec<(String, SerializedFile)>> {
    if !cache.contains_key(path) {
        let sfs = Bundle::open(path).ok()?.serialized_files().ok()?;
        cache.insert(path.clone(), sfs);
    }
    cache.get(path)
}

/// The CAB a PPtr's file_id names, from the holding file's externals (file_id 0 = same file `self_cab`).
fn pptr_cab_from(externals: &[unity_assets::External], self_cab: &str, file_id: i32) -> String {
    if file_id == 0 {
        return self_cab.to_string();
    }
    externals.get(file_id as usize - 1).map(|e| e.path.rsplit('/').next().unwrap_or(&e.path).to_string()).unwrap_or_default()
}

fn f(v: &Value, path: &str) -> Option<f32> {
    v.path(path).and_then(|x| x.as_f32())
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("bundle dir"));
    let out = PathBuf::from(std::env::args().nth(2).expect("out dir"));
    std::fs::create_dir_all(&out).unwrap();
    eprintln!("indexing bundles...");
    let index = cab_index(&dir);
    eprintln!("{} CABs indexed", index.len());
    let mut cache: HashMap<PathBuf, Vec<(String, SerializedFile)>> = HashMap::new();

    // (feature bundle, feature path_id, output name)
    let feats: [(&str, i64, &str); 3] = [
        ("f124b399caea956fb2ae85b8413c07d2.bundle", 2676774090902320730, "FB_Eyes_Default"),
        ("f124b399caea956fb2ae85b8413c07d2.bundle", -8684038003556714004, "FB_Mouth_Default"),
        ("6a075cffd3d231f124d5aa410b778755.bundle", 8425999176388334889, "FB_Eyebrows_Medium_Flat"),
    ];

    for (fbundle, fpid, name) in feats {
        let fpath = dir.join(fbundle);
        let Some(fsfs) = open_bundle(&mut cache, &fpath) else {
            eprintln!("{name}: cannot open {fbundle}");
            continue;
        };
        // find the feature + read DefaultSprites[0]
        let Some((fcab, fv, _)) = fsfs.iter().find_map(|(cab, sf)| {
            let o = sf.object(fpid)?;
            let v = sf.read_value(o).ok()?;
            Some((cab.clone(), v, sf.externals.clone()))
        }) else {
            eprintln!("{name}: feature {fpid} not found");
            continue;
        };
        let Some(sp) = fv.path("DefaultSprites").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_pptr) else {
            eprintln!("{name}: no DefaultSprites PPtr");
            continue;
        };
        // resolve the sprite's CAB (file_id 0 = the feature's own file) and open the bundle holding it
        let fsf_externals = fsfs.iter().find(|(c, _)| *c == fcab).map(|(_, sf)| sf.externals.clone()).unwrap_or_default();
        let sprite_cab = pptr_cab_from(&fsf_externals, &fcab, sp.file_id);
        let Some(sbundle_path) = index.get(&sprite_cab).cloned() else {
            eprintln!("{name}: sprite CAB {sprite_cab} not in any bundle");
            continue;
        };
        let Some(ssfs) = open_bundle(&mut cache, &sbundle_path) else { continue };
        let Some((scab, sprite_v, sprite_ext)) = ssfs.iter().find_map(|(cab, sf)| {
            let o = sf.object(sp.path_id)?;
            Some((cab.clone(), sf.read_value(o).ok()?, sf.externals.clone()))
        }) else {
            eprintln!("{name}: sprite {} not in {sprite_cab}", sp.path_id);
            continue;
        };
        // sprite -> texture PPtr + textureRect
        let Some(tex_pptr) = sprite_v.path("m_RD/texture").and_then(Value::as_pptr) else {
            eprintln!("{name}: sprite has no m_RD.texture");
            continue;
        };
        let (tx, ty, tw, th) = (
            f(&sprite_v, "m_RD/textureRect/x").unwrap_or(0.0),
            f(&sprite_v, "m_RD/textureRect/y").unwrap_or(0.0),
            f(&sprite_v, "m_RD/textureRect/width").unwrap_or(0.0),
            f(&sprite_v, "m_RD/textureRect/height").unwrap_or(0.0),
        );
        // resolve the texture's CAB + decode it
        let tex_cab = pptr_cab_from(&sprite_ext, &scab, tex_pptr.file_id);
        let tex_bundle_path = index.get(&tex_cab).cloned().or_else(|| if tex_cab == scab { Some(sbundle_path.clone()) } else { None });
        let Some(tex_bundle_path) = tex_bundle_path else {
            eprintln!("{name}: texture CAB {tex_cab} not found");
            continue;
        };
        let tex_bundle = Bundle::open(&tex_bundle_path).ok();
        let Some(tex_bundle) = tex_bundle else { continue };
        let Some(tsfs) = tex_bundle.serialized_files().ok() else { continue };
        let Some((tw_px, th_px, rgba)) = tsfs.iter().find(|(c, _)| *c == tex_cab).and_then(|(_, sf)| {
            let o = sf.object(tex_pptr.path_id)?;
            let v = sf.read_value(o).ok()?;
            let tex = Texture2D::from_value(&v, &tex_bundle).ok()?;
            let rgba = tex.decode_rgba8().ok()?;
            Some((tex.width, tex.height, rgba))
        }) else {
            eprintln!("{name}: texture {} decode failed", tex_pptr.path_id);
            continue;
        };
        // crop to the sprite's textureRect (atlas pixels, bottom-left origin) out of top-down rgba
        let (cw, ch) = (tw.round().max(1.0) as usize, th.round().max(1.0) as usize);
        let (ox, oy) = (tx.round().max(0.0) as usize, ty.round().max(0.0) as usize);
        let mut crop = vec![0u8; cw * ch * 4];
        for row in 0..ch {
            // sprite row 0 (bottom) -> texture row (th_px - oy - 1); we write top-down so flip
            let src_y = th_px.saturating_sub(oy + (ch - row));
            for col in 0..cw {
                let sx = ox + col;
                if sx < tw_px && src_y < th_px {
                    let si = (src_y * tw_px + sx) * 4;
                    let di = (row * cw + col) * 4;
                    if si + 4 <= rgba.len() {
                        crop[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
                    }
                }
            }
        }
        let path = out.join(format!("{name}.png"));
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(BufWriter::new(file), cw as u32, ch as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&crop).unwrap();
        println!("{name}: wrote {cw}x{ch} PNG (texture {tw_px}x{th_px}, rect {tx},{ty},{tw},{th}) -> {}", path.display());
    }
}
