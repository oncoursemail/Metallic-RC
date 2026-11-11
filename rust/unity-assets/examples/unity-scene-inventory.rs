//! Inventory of a scene's objects and the MonoBehaviour script types placed in it (spawn points, NetObjects,
//! triggers, teleporters, gameplay logic) — to see what dynamic/interactive content a room carries.
//! usage: unity-scene-inventory <level-file>
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use unity_assets::*;

struct Files {
    dir: PathBuf,
    open: HashMap<String, Option<SerializedFile>>,
}
impl Files {
    fn get(&mut self, name: &str) -> Option<&SerializedFile> {
        self.open
            .entry(name.to_string())
            .or_insert_with(|| std::fs::read(self.dir.join(name)).ok().and_then(|d| SerializedFile::parse(d).ok()))
            .as_ref()
    }
}

fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("level file"));
    let dir = path.parent().unwrap().to_path_buf();
    let sf = SerializedFile::parse(std::fs::read(&path).unwrap()).unwrap();

    let mut classes: BTreeMap<String, usize> = BTreeMap::new();
    for o in &sf.objects {
        let k = classes::class_name(o.class_id).map(String::from).unwrap_or_else(|| format!("class {}", o.class_id));
        *classes.entry(k).or_default() += 1;
    }
    println!("== {} : {} objects, unity {} ==", path.file_name().unwrap().to_string_lossy(), sf.objects.len(), sf.unity_version);
    for (k, n) in &classes {
        println!("  {n:>6}  {k}");
    }

    let mut files = Files { dir, open: HashMap::new() };
    let mut scripts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unresolved = 0usize;
    let mb_total = sf.objects.iter().filter(|o| o.class_id == classes::MONO_BEHAVIOUR).count();
    for o in sf.objects.iter().filter(|o| o.class_id == classes::MONO_BEHAVIOUR) {
        let Ok(p) = level::mono_script_ref(&sf, o) else {
            unresolved += 1;
            continue;
        };
        let script = if p.file_id == 0 {
            sf.object(p.path_id).and_then(|so| level::mono_script_name(&sf, so).ok())
        } else if let Some(ext) = sf.externals.get(p.file_id as usize - 1) {
            let name = ext.path.rsplit(['/', '\\']).next().unwrap_or(&ext.path).to_string();
            files.get(&name).and_then(|x| x.object(p.path_id).and_then(|so| level::mono_script_name(x, so).ok()))
        } else {
            None
        };
        match script {
            Some((cn, ns)) => *scripts.entry(if ns.is_empty() { cn } else { format!("{ns}.{cn}") }).or_default() += 1,
            None => unresolved += 1,
        }
    }
    // LODGroup parse check: every group should parse, and we tally LOD0 vs lower-LOD renderers.
    let (mut lg_ok, mut lg_err, mut lod0, mut lower) = (0usize, 0usize, 0usize, 0usize);
    for o in sf.objects.iter().filter(|o| o.class_id == classes::LOD_GROUP) {
        match level::lod_group(&sf, o) {
            Ok(g) => {
                lg_ok += 1;
                for (i, lv) in g.lods.iter().enumerate() {
                    if i == 0 {
                        lod0 += lv.len();
                    } else {
                        lower += lv.len();
                    }
                }
            }
            Err(e) => {
                if lg_err == 0 {
                    println!("  LODGroup parse error: {e}");
                }
                lg_err += 1;
            }
        }
    }
    println!("\n== LODGroups: {lg_ok} parsed, {lg_err} failed; {lod0} LOD0 renderers, {lower} lower-LOD renderers ==");

    println!("\n== MonoBehaviour scripts ({mb_total} total, {unresolved} unresolved) ==");
    let mut v: Vec<_> = scripts.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, n) in v {
        println!("  {n:>5}  {k}");
    }
}
