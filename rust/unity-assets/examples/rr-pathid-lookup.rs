//! Find objects by PathID across the install bundles and read their REAL name/class/value (bundles carry type
//! trees). Used to resolve the modern default face features and the OutfitManager colour vaults 1:1 — no guessing.
//! usage: rr-pathid-lookup <bundle-dir> [pathid,pathid,...]   (defaults to the 3 face-feature PathIDs)
use unity_assets::*;

fn summarize(v: &Value, depth: usize, out: &mut String) {
    if depth > 7 || out.len() > 24000 {
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
            for (i, e) in a.iter().take(60).enumerate() {
                out.push_str(&format!(" #{i}:"));
                summarize(e, depth + 1, out);
            }
            out.push(']');
        }
        Value::PPtr(p) => out.push_str(&format!("PPtr(f{},{})", p.file_id, p.path_id)),
        Value::Float(f) => out.push_str(&format!("{f:.4}")),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::UInt(u) => out.push_str(&u.to_string()),
        Value::Bool(b) => out.push_str(&b.to_string()),
        Value::String(s) => out.push_str(&format!("{s:?}")),
        _ => out.push('?'),
    }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("bundle dir"));
    let targets: Vec<i64> = match std::env::args().nth(2) {
        Some(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
        None => vec![2676774090902320730, 8425999176388334889, -8684038003556714004],
    };
    let mut found = 0;
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        if found >= targets.len() {
            break;
        }
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("bundle") {
            continue;
        }
        scanned += 1;
        let Ok(b) = Bundle::open(&p) else { continue };
        let Ok(sfs) = b.serialized_files() else { continue };
        for (cab, sf) in &sfs {
            for o in sf.objects.iter().filter(|o| targets.contains(&o.path_id)) {
                found += 1;
                let class = classes::class_name(o.class_id).map(String::from).unwrap_or_else(|| format!("class {}", o.class_id));
                let name = sf.object_name(o).unwrap_or_default();
                println!("=== path_id={} class={class} m_Name={name:?}  [bundle {} / {cab}] ===", o.path_id, p.file_name().unwrap().to_string_lossy());
                if let Ok(v) = sf.read_value(o) {
                    let mut s = String::new();
                    summarize(&v, 0, &mut s);
                    println!("{s}\n");
                }
            }
        }
    }
    eprintln!("scanned {scanned} bundles, found {found}/{}", targets.len());
}
