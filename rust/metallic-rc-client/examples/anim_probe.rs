//! Spike: can we read the REAL authored avatar animation clips from the owned game's resources.assets, resolve their
//! bone bindings to our Jnt.* skeleton via the Avatar m_TOS, and sample them? Prints clips, avatars, and for one clip
//! its bone coverage + a sample. Not shipped in the client; a feasibility probe for authored-clip playback.
//! The live install's resources.assets is RR-encrypted; point this at the decompile session's DECRYPTED corpus:
//! `cargo run --profile fast -p metallic-rc-client --example anim_probe -- <corpus-resources.assets> [clip-name-substr]`
//! (with no path arg it tries the live resources.assets, which will fail to parse until decrypted).
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use unity_assets::{value, AnimationClip, Bundle, Reader, SerializedFile, TypeTree};

const GAME_DIRS: [&str; 2] = [
    r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711",
    r"C:\Program Files (x86)\Steam\steamapps\common\RecRoom",
];
// a bundle verified to ship an AnimationClip (74) type tree (corpus/resources clips ship none)
const TT_BUNDLE: &str = "0e46772fa7e018f83b8b8f20adcbbd96.bundle";

fn bundle_dir(game: &Path) -> PathBuf {
    game.join(r"RecRoom_Data\StreamingAssets\aa\StandaloneWindows64")
}

/// First type tree for `class_id` found across a bundle's serialized files.
fn tree_for(b: &Bundle, class_id: i32) -> Option<TypeTree> {
    let files = b.serialized_files().ok()?;
    files.into_iter().flat_map(|(_, sf)| sf.types).find(|t| t.class_id == class_id && t.tree.is_some()).and_then(|t| t.tree)
}

fn main() {
    // optional first arg: path to a readable (decrypted) resources.assets; second: clip-name filter
    let mut args = std::env::args().skip(1);
    let a1 = args.next();
    let game = GAME_DIRS.iter().map(Path::new).find(|p| p.exists()).expect("game dir");
    let (res_path, filter) = match a1 {
        Some(p) if Path::new(&p).exists() => (PathBuf::from(p), args.next()),
        other => (game.join(r"RecRoom_Data\resources.assets"), other),
    };
    println!("resources.assets: {}", res_path.display());

    // the AnimationClip (74) type tree from a bundle
    let ttb = Bundle::open(bundle_dir(game).join(TT_BUNDLE)).expect("tt bundle");
    let clip_tree = tree_for(&ttb, 74).expect("AnimationClip type tree in tt bundle");
    // the Avatar (90) type tree is not in the tt bundle; scan the bundle dir for one that ships it
    let avatar_tree = tree_for(&ttb, 90).map(|t| (t, TT_BUNDLE.to_string())).or_else(|| {
        let dir = bundle_dir(game);
        let mut entries: Vec<_> = std::fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "bundle").unwrap_or(false)).collect();
        entries.sort();
        for (i, p) in entries.iter().enumerate() {
            if i % 200 == 0 {
                eprintln!("  scanning for Avatar tree... {i}/{}", entries.len());
            }
            if let Ok(b) = Bundle::open(p) {
                if let Some(t) = tree_for(&b, 90) {
                    return Some((t, p.file_name().unwrap().to_string_lossy().into_owned()));
                }
            }
        }
        None
    });
    println!("AnimationClip tree: ok; Avatar tree: {}", avatar_tree.as_ref().map(|(_, n)| n.as_str()).unwrap_or("NOT FOUND"));
    let avatar_tree = avatar_tree.map(|(t, _)| t);

    // parse resources.assets and bucket objects
    let res = SerializedFile::parse(std::fs::read(&res_path).expect("read resources.assets")).expect("parse");
    let mut by_class: HashMap<i32, usize> = HashMap::new();
    for o in &res.objects {
        *by_class.entry(o.class_id).or_default() += 1;
    }
    println!("resources.assets objects: {} total; clips(74)={} avatars(90)={}", res.objects.len(),
        by_class.get(&74).copied().unwrap_or(0), by_class.get(&90).copied().unwrap_or(0));

    // list clip + avatar names
    let clip_objs: Vec<_> = res.objects.iter().filter(|o| o.class_id == 74).collect();
    let avatar_objs: Vec<_> = res.objects.iter().filter(|o| o.class_id == 90).collect();
    println!("\n=== avatars (class 90) ===");
    for o in &avatar_objs {
        println!("  {} {:?}", o.path_id, res.object_name(o).unwrap_or_default());
    }
    println!("\n=== clips (class 74), first 40 of {} ===", clip_objs.len());
    for o in clip_objs.iter().take(40) {
        println!("  {} {:?}", o.path_id, res.object_name(o).unwrap_or_default());
    }

    // decode one clip (filtered by name, else the first) and resolve its bone bindings via an avatar m_TOS
    let chosen = clip_objs.iter().find(|o| {
        filter.as_ref().map(|f| res.object_name(o).unwrap_or_default().to_lowercase().contains(&f.to_lowercase())).unwrap_or(true)
    });
    let Some(co) = chosen else {
        println!("\nno clip matched {filter:?}");
        return;
    };
    let name = res.object_name(co).unwrap_or_default();
    println!("\n=== decode clip {:?} (path_id {}) ===", name, co.path_id);
    let v = value::read(&clip_tree, &res.ref_types, &mut Reader::new(res.object_bytes(co), res.big_endian)).expect("read clip value");
    let clip = AnimationClip::from_value(&v).expect("from_value");
    println!("  name={} sample_rate={} stop_time={} bindings={} curves={}", clip.name, clip.sample_rate, clip.stop_time, clip.bindings.len(), clip.curve_count());

    // build an m_TOS (hash -> bone path) from the player-body avatar (the one whose paths carry Jnt.*)
    let tos = avatar_tree.as_ref().and_then(|at| {
        let mut best: HashMap<u32, String> = HashMap::new();
        for ao in &avatar_objs {
            let Ok(av) = value::read(at, &res.ref_types, &mut Reader::new(res.object_bytes(ao), res.big_endian)) else { continue };
            let map: HashMap<u32, String> = av.get("m_TOS").map(|t| t.pairs().iter()
                .filter_map(|(k, val)| Some((k.as_u64()? as u32, val.as_str()?.to_string()))).collect()).unwrap_or_default();
            let jnts = map.values().filter(|p| p.contains("Jnt.")).count();
            println!("  avatar {:?}: m_TOS {} entries, {} with Jnt.", res.object_name(ao).unwrap_or_default(), map.len(), jnts);
            if jnts > best.values().filter(|p| p.contains("Jnt.")).count() {
                best = map;
            }
        }
        (!best.is_empty()).then_some(best)
    });

    // resolve bindings -> bone path, count Jnt coverage, sample at t=0
    let mut jnt_bones = 0;
    println!("\n=== bindings (first 20) ===");
    for (i, bnd) in clip.bindings.iter().enumerate() {
        let path = tos.as_ref().and_then(|m| m.get(&bnd.path)).cloned();
        if path.as_deref().map(|p| p.contains("Jnt.")).unwrap_or(false) {
            jnt_bones += 1;
        }
        if i < 20 {
            let vals = clip.sample_binding(bnd, 0.0);
            let leaf = path.as_deref().map(|p| p.rsplit('/').next().unwrap_or(p).to_string());
            println!("  #{i} path={:#010x} {:?} -> {:?}  sample@0={:?}", bnd.path, bnd.attribute, leaf.or(path), vals.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>());
        }
    }
    println!("\nbone-path resolution: {}/{} bindings map to a Jnt.* bone", jnt_bones, clip.bindings.len());
}
