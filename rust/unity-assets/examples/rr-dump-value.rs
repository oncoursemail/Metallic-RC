//! Dump a MonoBehaviour's value by m_Name, and separately list MonoBehaviours whose script CLASS or name hints at a
//! colour vault/palette — to find the real default skin/hair colours. Bundles carry type trees so read_value works.
//! usage: rr-dump-value <bundle-dir> <m_Name-substring>
use unity_assets::*;

fn summarize(v: &Value, depth: usize, out: &mut String) {
    if depth > 6 || out.len() > 16000 {
        return;
    }
    match v {
        Value::Struct { type_name, fields } => {
            out.push_str(&format!("{type_name}{{ "));
            for (name, fv) in fields {
                out.push_str(&format!("{name}="));
                summarize(fv, depth + 1, out);
                out.push_str(", ");
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push_str(&format!("[{} items", a.len()));
            if let Some(first) = a.first() {
                out.push_str("; e.g. ");
                summarize(first, depth + 1, out);
            }
            out.push(']');
        }
        Value::PPtr(p) => out.push_str(&format!("PPtr({},{})", p.file_id, p.path_id)),
        Value::Float(f) => out.push_str(&format!("{f:.3}")),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::UInt(u) => out.push_str(&u.to_string()),
        Value::Bool(b) => out.push_str(&b.to_string()),
        Value::String(s) => out.push_str(&format!("{s:?}")),
        _ => out.push('?'),
    }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("bundle dir"));
    let want = std::env::args().nth(2).expect("m_Name substring");
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("bundle") {
            continue;
        }
        scanned += 1;
        let Ok(b) = Bundle::open(&p) else { continue };
        let Ok(sfs) = b.serialized_files() else { continue };
        for (_, sf) in &sfs {
            for o in sf.objects.iter().filter(|o| o.class_id == classes::MONO_BEHAVIOUR) {
                let Some(nm) = sf.object_name(o) else { continue };
                let low = nm.to_ascii_lowercase();
                let is_vaultish = low.contains("vault") || (low.contains("skin") && low.contains("color")) || (low.contains("hair") && low.contains("color")) || low.contains("skintone") || low.contains("outfitmanager");
                if nm.contains(&want) || is_vaultish {
                    if let Ok(v) = sf.read_value(o) {
                        let mut s = String::new();
                        summarize(&v, 0, &mut s);
                        println!("[{}] {nm}:\n  {}\n", p.file_name().unwrap().to_string_lossy(), s);
                    }
                }
            }
        }
    }
    eprintln!("scanned {scanned} bundles");
}
