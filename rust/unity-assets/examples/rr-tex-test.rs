//! Test: can streamed textures of a decrypted scene be decoded by reading .resS from the game install?
//! usage: rr-tex-test <level file> <game RecRoom_Data dir> [out.png]
use std::collections::HashMap;
use std::path::PathBuf;
use unity_assets::*;

/// Resources that try several directories (the corpus, then the game install) for the .resS/.resource file.
struct MultiRes(Vec<PathBuf>);
impl objects::Resources for MultiRes {
    fn resource(&self, path: &str, offset: u64, size: u64) -> Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        for dir in &self.0 {
            if let Ok(mut f) = std::fs::File::open(dir.join(name)) {
                f.seek(SeekFrom::Start(offset))?;
                let mut v = vec![0; size as usize];
                if f.read_exact(&mut v).is_ok() {
                    return Ok(v);
                }
            }
        }
        Err(Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{name} not found"))))
    }
}

struct Files {
    dir: PathBuf,
    open: HashMap<String, Option<SerializedFile>>,
}
impl Files {
    fn get(&mut self, name: &str) -> Option<&SerializedFile> {
        self.open.entry(name.to_string()).or_insert_with(|| std::fs::read(self.dir.join(name)).ok().and_then(|d| SerializedFile::parse(d).ok())).as_ref()
    }
    fn resolve(&mut self, from: &str, p: PPtr) -> Option<(String, i64)> {
        if p.path_id == 0 { return None; }
        if p.file_id == 0 { return Some((from.to_string(), p.path_id)); }
        let sf = self.get(from)?;
        let ext = sf.externals.get(p.file_id as usize - 1)?;
        if ext.path.starts_with("Library/") || ext.path.contains("unity_builtin") { return None; }
        Some((ext.path.rsplit(['/', '\\']).next().unwrap_or(&ext.path).to_string(), p.path_id))
    }
}

fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("level file"));
    let game = PathBuf::from(std::env::args().nth(2).expect("game RecRoom_Data dir"));
    let out = std::env::args().nth(3);
    let dir = path.parent().unwrap().to_path_buf();
    let scene_name = path.file_name().unwrap().to_str().unwrap().to_string();
    let res = MultiRes(vec![dir.clone(), game]);
    let mut files = Files { dir: dir.clone(), open: HashMap::new() };
    files.open.insert(scene_name.clone(), Some(SerializedFile::parse(std::fs::read(&path).unwrap()).unwrap()));

    // gather textures referenced by materials
    let scene = SerializedFile::parse(std::fs::read(&path).unwrap()).unwrap();
    let mut mats = Vec::new();
    for o in scene.objects.iter().filter(|o| o.class_id == classes::MESH_RENDERER) {
        if let Ok(r) = scene.mesh_renderer(o) {
            for p in r.materials { if let Some(m) = files.resolve(&scene_name, p) { mats.push(m); } }
        }
    }
    mats.sort(); mats.dedup();
    let mut texs = Vec::new();
    for (f, id) in &mats {
        if let Some((sf, o)) = files.get(f).and_then(|sf| sf.object(*id).map(|o| (sf, o))) {
            if let Ok(m) = level::material(sf, o) {
                for t in m.textures.iter().filter(|t| t.texture.path_id != 0) {
                    if let Some(tx) = files.resolve(f, t.texture) { texs.push(tx); }
                }
            }
        }
    }
    texs.sort(); texs.dedup();
    println!("distinct textures referenced: {}", texs.len());
    let (mut ok, mut streamed_ok, mut inline, mut fail) = (0, 0, 0, 0);
    let mut first_png: Option<(String, u32, u32, Vec<u8>)> = None;
    for (f, id) in texs.iter().take(400) {
        let got = files.get(f).and_then(|sf| sf.object(*id).map(|o| (sf, o)));
        let Some((sf, o)) = got else { continue };
        match level::texture(sf, o, &res) {
            Ok(t) => {
                let streamed = t.data.len();
                match t.decode_rgba8() {
                    Ok(rgba) => {
                        ok += 1;
                        if streamed > 0 { streamed_ok += 1; } else { inline += 1; }
                        let skip = ["SDF", "Atlas", "Font", "Lightmap", "Normal", "_Nor", "Bump"].iter().any(|k| t.name.contains(k));
                        if first_png.is_none() && !skip && t.width >= 64 && t.height >= 64 {
                            first_png = Some((t.name.clone(), t.width as u32, t.height as u32, rgba));
                        }
                    }
                    Err(e) => { fail += 1; if fail <= 3 { println!("decode fail {}: {e}", t.name); } }
                }
            }
            Err(e) => { fail += 1; if fail <= 5 { println!("texture parse fail: {e}"); } }
        }
    }
    println!("decoded OK {ok} (streamed {streamed_ok}, inline {inline}), fail {fail}");
    if let (Some(path), Some((name, w, h, rgba))) = (out, first_png) {
        // write PNG
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
        println!("wrote {path} ({name}, {w}x{h})");
    }
}
