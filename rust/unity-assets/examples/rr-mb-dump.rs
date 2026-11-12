//! Recon for the per-game config work: find every MonoBehaviour of a given script CLASS in a scene (resolving
//! m_Script across the scene's sibling files) and dump the first N words of its serialized body, each shown as
//! int / float / and (for the first word pair) a possible PPtr — so a gadget's leading fields are easy to eyeball
//! before writing a MonoFields reader. usage: rr-mb-dump <level-file> <ClassName> [words]
use std::collections::HashMap;
use unity_assets::*;

struct Files {
    dir: std::path::PathBuf,
    open: HashMap<String, Option<SerializedFile>>,
}
impl Files {
    fn get(&mut self, name: &str) -> Option<&SerializedFile> {
        self.open.entry(name.to_string()).or_insert_with(|| std::fs::read(self.dir.join(name)).ok().and_then(|d| SerializedFile::parse(d).ok())).as_ref()
    }
}

fn main() {
    let level = std::path::PathBuf::from(std::env::args().nth(1).expect("level file"));
    let want = std::env::args().nth(2).expect("class name");
    let words: usize = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(8);
    let dir = level.parent().unwrap().to_path_buf();
    let sf = SerializedFile::parse(std::fs::read(&level).unwrap()).unwrap();
    let mut files = Files { dir, open: HashMap::new() };
    let externals: Vec<String> = sf.externals.iter().map(|e| e.path.rsplit('/').next().unwrap_or(&e.path).to_string()).collect();

    let class_of = |o: &ObjectInfo, files: &mut Files| -> Option<String> {
        let p = level::mono_script_ref(&sf, o).ok()?;
        if p.file_id == 0 {
            return level::mono_script_name(&sf, sf.object(p.path_id)?).ok().map(|(c, _)| c);
        }
        let ext = externals.get(p.file_id as usize - 1)?;
        let f = files.get(ext)?;
        level::mono_script_name(f, f.object(p.path_id)?).ok().map(|(c, _)| c)
    };

    let objs: Vec<&ObjectInfo> = sf.objects.iter().filter(|o| o.class_id == classes::MONO_BEHAVIOUR).collect();
    let mut found = 0;
    for o in objs {
        if class_of(o, &mut files).as_deref() != Some(want.as_str()) {
            continue;
        }
        found += 1;
        // header: m_GameObject (PPtr 12), m_Enabled (u8 + align4), m_Script (PPtr 12), m_Name (aligned string)
        let b = sf.object_bytes(o);
        let mut r = Reader::new(b, sf.big_endian);
        let _ = (r.i32(), r.i64()); // m_GameObject
        let _ = r.u8();
        r.align(4); // m_Enabled
        let _ = (r.i32(), r.i64()); // m_Script
        let name = r.aligned_string().unwrap_or_default();
        let start = r.pos();
        println!("--- {want} #{found}  m_Name={name:?}  body@{start}, {} bytes ---", b.len() - start);
        for w in 0..words {
            let off = start + w * 4;
            if off + 4 > b.len() {
                break;
            }
            let i = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
            let f = f32::from_le_bytes(b[off..off + 4].try_into().unwrap());
            let fs = if f.is_finite() && f.abs() < 1e6 { format!("{f:.3}") } else { "-".into() };
            println!("  w{w:<2} @{off:<4} int={i:<12} float={fs}");
        }
    }
    println!("\n{want}: {found} found in {}", level.file_name().unwrap().to_string_lossy());
}
