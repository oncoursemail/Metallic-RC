//! Face recon: find objects by PathID and the colour vaults across the install bundles, reading REAL names/colours
//! from the real assets (no guessing). usage: rr-pathid-lookup <bundle-dir>
use unity_assets::*;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("bundle dir"));
    // targets: Eyes[0], EyeBrows[5], Mouths[0]
    let targets: [i64; 3] = [2676774090902320730, 8425999176388334889, -8684038003556714004];
    let mut hits: Vec<(i64, String, String)> = Vec::new(); // (path_id, class, name)
    let mut vaults: Vec<(String, String)> = Vec::new(); // (m_Name, dumped fields)
    let mut face_sets = 0;
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
            for o in &sf.objects {
                if targets.contains(&o.path_id) {
                    let class = classes::class_name(o.class_id).map(String::from).unwrap_or_else(|| format!("class {}", o.class_id));
                    let name = sf.object_name(o).unwrap_or_default();
                    hits.push((o.path_id, class, format!("{name}  [bundle {}]", p.file_name().unwrap().to_string_lossy())));
                }
                if o.class_id == classes::MONO_BEHAVIOUR {
                    if let Some(nm) = sf.object_name(o) {
                        let n = nm.to_ascii_lowercase();
                        if n.contains("colorvault") || n.contains("colourvault") {
                            let dump = sf.read_value(o).ok().map(|v| format!("{v:?}")).unwrap_or_default();
                            let dump = dump.chars().take(600).collect::<String>();
                            vaults.push((nm.clone(), dump));
                        }
                        if nm.contains("PlayerFaceFeatureSet") {
                            face_sets += 1;
                            println!("FaceSet '{nm}' in {}", p.file_name().unwrap().to_string_lossy());
                        }
                    }
                }
            }
        }
    }
    println!("\n=== scanned {scanned} bundles ===");
    println!("PathID hits ({}):", hits.len());
    for (pid, class, name) in &hits {
        println!("  {pid}  {class}  {name}");
    }
    println!("FaceSets seen: {face_sets}");
    println!("ColorVaults ({}):", vaults.len());
    for (name, dump) in &vaults {
        println!("  {name}: {dump}");
    }
}
