//! Diagnostic: exercise level::mesh/material/texture on a no-type-tree Unity 6 scene, report layout errors.
use std::collections::HashMap;
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
    fn resolve(&mut self, from: &str, p: PPtr) -> Option<(String, i64)> {
        if p.path_id == 0 {
            return None;
        }
        if p.file_id == 0 {
            return Some((from.to_string(), p.path_id));
        }
        let sf = self.get(from)?;
        let ext = sf.externals.get(p.file_id as usize - 1)?;
        if ext.path.starts_with("Library/") || ext.path.contains("unity_builtin") {
            return None;
        }
        let name = ext.path.rsplit(['/', '\\']).next().unwrap_or(&ext.path).to_string();
        Some((name, p.path_id))
    }
}

fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("level file"));
    let dir = path.parent().unwrap().to_path_buf();
    let scene_name = path.file_name().unwrap().to_str().unwrap().to_string();
    let scene = SerializedFile::parse(std::fs::read(&path).unwrap()).unwrap();
    println!("unity {} ver {} objects {}", scene.unity_version, scene.version, scene.objects.len());
    let mut files = Files { dir: dir.clone(), open: HashMap::new() };
    files.open.insert(scene_name.clone(), Some(SerializedFile::parse(std::fs::read(&path).unwrap()).unwrap()));
    let res = level::DirResources(dir.clone());

    let mut err_counts: HashMap<String, (usize, String)> = HashMap::new();
    let mut tally = |label: &str, r: std::result::Result<(), String>| {
        let key = match &r {
            Ok(()) => format!("{label}: OK"),
            Err(e) => format!("{label}: {}", e.split(" read ").next().unwrap_or(&e).chars().take(60).collect::<String>()),
        };
        let ent = err_counts.entry(key).or_insert((0, r.err().unwrap_or_default()));
        ent.0 += 1;
    };

    // Collect meshes and materials referenced by MeshRenderers (+ their MeshFilters).
    let (mut meshes, mut mats, mut texs) = (Vec::new(), Vec::new(), Vec::new());
    for o in scene.objects.iter().filter(|o| o.class_id == classes::MESH_FILTER) {
        if let Ok(mf) = scene.mesh_filter(o) {
            if let Some(m) = files.resolve(&scene_name, mf.mesh) {
                meshes.push(m);
            }
        }
    }
    for o in scene.objects.iter().filter(|o| o.class_id == classes::MESH_RENDERER) {
        if let Ok(r) = scene.mesh_renderer(o) {
            for p in r.materials {
                if let Some(m) = files.resolve(&scene_name, p) {
                    mats.push(m);
                }
            }
        }
    }
    meshes.sort(); meshes.dedup(); mats.sort(); mats.dedup();
    println!("distinct meshes {} materials {}", meshes.len(), mats.len());

    for (f, id) in meshes.iter().take(400) {
        let r = files.get(f).and_then(|sf| sf.object(*id).map(|o| (sf, o)));
        match r {
            Some((sf, o)) => tally("mesh", level::mesh(sf, o, &res).map(|_| ()).map_err(|e| e.to_string())),
            None => {}
        }
    }
    for (f, id) in mats.iter().take(400) {
        let got = files.get(f).and_then(|sf| sf.object(*id).map(|o| (sf, o)));
        if let Some((sf, o)) = got {
            match level::material(sf, o) {
                Ok(m) => {
                    tally("material", Ok(()));
                    for t in m.textures.iter().filter(|t| t.texture.path_id != 0) {
                        if let Some(tx) = files.resolve(f, t.texture) {
                            texs.push(tx);
                        }
                    }
                }
                Err(e) => tally("material", Err(e.to_string())),
            }
        }
    }
    texs.sort(); texs.dedup();
    for (f, id) in texs.iter().take(200) {
        let got = files.get(f).and_then(|sf| sf.object(*id).map(|o| (sf, o)));
        if let Some((sf, o)) = got {
            tally("texture", level::texture(sf, o, &res).map(|_| ()).map_err(|e| e.to_string()));
        }
    }
    // RenderSettings, LightmapSettings, Lights, Colliders in the scene
    for o in &scene.objects {
        match o.class_id {
            c if c == classes::RENDER_SETTINGS => tally("render_settings", level::render_settings(&scene, o).map(|_| ()).map_err(|e| e.to_string())),
            c if c == classes::LIGHTMAP_SETTINGS => tally("lightmap_settings", level::lightmap_settings(&scene, o).map(|_| ()).map_err(|e| e.to_string())),
            c if c == classes::LIGHT => tally("light", level::light(&scene, o).map(|_| ()).map_err(|e| e.to_string())),
            c if c == classes::BOX_COLLIDER => tally("box_collider", level::box_collider(&scene, o).map(|_| ()).map_err(|e| e.to_string())),
            c if c == classes::REFLECTION_PROBE => tally("reflection_probe", level::reflection_probe(&scene, o).map(|_| ()).map_err(|e| e.to_string())),
            _ => {}
        }
    }

    let mut keys: Vec<_> = err_counts.into_iter().collect();
    keys.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    println!("\n=== results (label: count) ===");
    for (k, (n, full)) in &keys {
        println!("{n:5}  {k}");
        if !full.is_empty() && !k.ends_with("OK") {
            println!("        full: {full}");
        }
    }
}
