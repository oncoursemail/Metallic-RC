//! Lists the game's shaders (name, properties, keywords, passes) and which of them the rooms use.
//! `cargo run --profile fast -p metallic-rc-client --example look_shaders -- [shaders|rooms] [filter]`
use std::collections::BTreeMap;

use metallic_rc_client::{room::{self, Library}, world};
use unity_assets::{classes, Bundle, Value};

/// The Addressables bundle with the AG shaders (rendering.md).
const SHADER_BUNDLE: &str = "b1a0b840467d50b0fbe1fbc1413be7b7.bundle";

fn names(v: Option<&Value>) -> Vec<String> {
    v.and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let what = args.next().unwrap_or_else(|| "shaders".into());
    let filter = args.next().unwrap_or_default();
    let game = room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()).expect("no game folder");
    let dir = room::bundle_dir(&game);
    if what == "rooms" {
        let mut lib = Library::new(&dir).unwrap();
        for w in ["dorm", "sandbox", "stuntrunner", "(Charades_InkSpace_MainHull_Sandbox)Variant"] {
            let r = world::load(&mut lib, &dir, Some(w)).unwrap();
            let mut by: BTreeMap<&str, (usize, Vec<&str>)> = BTreeMap::new();
            for p in &r.parts {
                let m = &r.materials[p.material];
                let e = by.entry(m.shader.as_str()).or_default();
                e.0 += 1;
                if !e.1.contains(&m.name.as_str()) {
                    e.1.push(&m.name);
                }
            }
            println!("== {w}: {} parts, {} materials, {} lights", r.parts.len(), r.materials.len(), r.lights.len());
            for (s, (n, mats)) in by {
                println!("  {n:4} {s:45} {}", mats.iter().take(6).cloned().collect::<Vec<_>>().join(", "));
            }
        }
        return;
    }
    let b = Bundle::open(dir.join(SHADER_BUNDLE)).unwrap();
    for (sf_name, sf) in b.serialized_files().unwrap() {
        for o in sf.objects.iter().filter(|o| o.class_id == classes::SHADER) {
            let Ok(v) = sf.read_value(o) else { continue };
            let pf = v.get("m_ParsedForm");
            let name = pf.and_then(|p| p.get("m_Name")).and_then(|n| n.as_str()).unwrap_or("?");
            if !name.contains(&filter) {
                continue;
            }
            println!("== {name} (CAB {}, path id {})", sf_name, o.path_id);
            let props: Vec<String> = pf.and_then(|p| p.path("m_PropInfo/m_Props")).and_then(|a| a.as_array()).unwrap_or(&[]).iter().map(|p| {
                let n = p.get("m_Name").and_then(|x| x.as_str()).unwrap_or("?");
                let d = p.get("m_DefValue[0]").and_then(|x| x.as_f32()).or_else(|| p.get("m_DefValue[0]").and_then(|x| x.as_i64()).map(|i| i as f32));
                let attrs = names(p.get("m_Attributes")).join(" ");
                format!("{n}{}{}", d.map(|d| format!("={d}")).unwrap_or_default(), if attrs.is_empty() { String::new() } else { format!(" [{attrs}]") })
            }).collect();
            println!("  props: {}", props.join(", "));
            let kw: Vec<String> = pf.and_then(|p| p.path("m_KeywordNames")).and_then(|a| a.as_array()).unwrap_or(&[]).iter().filter_map(|x| x.as_str().map(String::from)).collect();
            println!("  keywords: {}", kw.join(" "));
            for s in pf.and_then(|p| p.get("m_SubShaders")).and_then(|a| a.as_array()).unwrap_or(&[]) {
                for p in s.get("m_Passes").and_then(|a| a.as_array()).unwrap_or(&[]) {
                    let pname = p.path("m_State/m_Name").and_then(|x| x.as_str()).unwrap_or("");
                    let tags: Vec<String> = p.path("m_Tags/tags").map(|t| t.pairs().iter().map(|(k, v)| format!("{}={}", k.as_str().unwrap_or(""), v.as_str().unwrap_or(""))).collect()).unwrap_or_default();
                    println!("  pass {pname} {}", tags.join(" "));
                }
            }
        }
    }
}
