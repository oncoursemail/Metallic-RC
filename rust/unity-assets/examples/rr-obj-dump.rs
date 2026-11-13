//! Dump a single object's value by PathID from one bundle (bundles carry type trees). For face-sprite recon: see
//! how each FB_ feature references its sprite(s). usage: rr-obj-dump <bundle-file> <path_id>
use unity_assets::*;

fn summarize(v: &Value, depth: usize, out: &mut String) {
    if depth > 6 || out.len() > 20000 {
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
            for (i, e) in a.iter().take(4).enumerate() {
                out.push_str(&format!(" #{i}:"));
                summarize(e, depth + 1, out);
            }
            out.push(']');
        }
        Value::PPtr(p) => out.push_str(&format!("PPtr(f{},{})", p.file_id, p.path_id)),
        Value::Float(f) => out.push_str(&format!("{f:.3}")),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::UInt(u) => out.push_str(&u.to_string()),
        Value::Bool(b) => out.push_str(&b.to_string()),
        Value::String(s) => out.push_str(&format!("{s:?}")),
        _ => out.push('?'),
    }
}

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("bundle file"));
    let want: i64 = std::env::args().nth(2).expect("path_id").parse().expect("path_id i64");
    let b = Bundle::open(&path).expect("open bundle");
    let sfs = b.serialized_files().expect("sfs");
    for (cab, sf) in &sfs {
        if let Some(o) = sf.object(want) {
            let class = classes::class_name(o.class_id).map(String::from).unwrap_or_else(|| format!("class {}", o.class_id));
            let name = sf.object_name(o).unwrap_or_default();
            println!("[{cab}] path_id={want} class={class} m_Name={name:?}");
            match sf.read_value(o) {
                Ok(v) => {
                    let mut s = String::new();
                    summarize(&v, 0, &mut s);
                    println!("{s}");
                }
                Err(e) => println!("read_value err: {e}"),
            }
            // list externals so PPtr file_ids can be mapped to bundles
            if !sf.externals.is_empty() {
                println!("externals:");
                for (i, e) in sf.externals.iter().enumerate() {
                    println!("  file_id {} -> {}", i + 1, e.path);
                }
            }
            return;
        }
    }
    println!("path_id {want} not found in {}", path.display());
}
