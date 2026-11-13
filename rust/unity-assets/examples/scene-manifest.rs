//! Per-environment load-data manifest for every built-in scene, from a plaintext data dir
//! (the decrypted corpus). For each `levelN`: scene path + short name, root objects, the external
//! serialized files it depends on, per-class object counts, and a lighting summary. Emits JSON.
//!
//! `scene-manifest <data dir> [--pretty]`

use std::collections::BTreeMap;
use std::path::PathBuf;
use unity_assets::*;

fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("usage: scene-manifest <data dir> [--pretty]"));
    let pretty = args.any(|a| a == "--pretty");
    let nl = if pretty { "\n" } else { "" };
    let sp = if pretty { " " } else { "" };

    let ggm = SerializedFile::parse(std::fs::read(dir.join("globalgamemanagers")).expect("globalgamemanagers"))
        .expect("parse globalgamemanagers");
    let scenes = level::build_scenes(&ggm).expect("build settings");

    print!("[{nl}");
    for (i, path) in scenes.iter().enumerate() {
        let file = dir.join(format!("level{i}"));
        let sf = match std::fs::read(&file).map_err(Error::from).and_then(SerializedFile::parse) {
            Ok(sf) => sf,
            Err(e) => {
                print!("{sp}{sp}{{\"index\":{i},\"file\":\"level{i}\",\"error\":{}}}", jstr(&e.to_string()));
                if i + 1 < scenes.len() { print!(","); }
                print!("{nl}");
                continue;
            }
        };
        // roots
        let mut roots = Vec::new();
        for o in sf.objects.iter().filter(|o| o.class_id == classes::TRANSFORM || o.class_id == classes::RECT_TRANSFORM) {
            let Ok(t) = sf.transform(o) else { continue };
            if t.parent.is_null() {
                let name = sf.object(t.game_object.path_id).and_then(|g| sf.object_name(g)).unwrap_or_default();
                if !name.is_empty() { roots.push(name); }
            }
        }
        // class counts
        let mut classcount: BTreeMap<i32, usize> = BTreeMap::new();
        for o in &sf.objects {
            *classcount.entry(o.class_id).or_default() += 1;
        }
        let getc = |id: i32| classcount.get(&id).copied().unwrap_or(0);
        let short = level::scene_name(path);

        print!("{sp}{sp}{{{nl}");
        print!("{sp}{sp}{sp}{sp}\"index\":{i},{sp}\"file\":\"level{i}\",{sp}\"scene\":{},{sp}\"name\":{},{nl}", jstr(path), jstr(short));
        print!("{sp}{sp}{sp}{sp}\"objects\":{},{sp}\"externals\":[", sf.objects.len());
        for (k, e) in sf.externals.iter().enumerate() {
            if k > 0 { print!(","); }
            print!("{}", jstr(e.path.rsplit('/').next().unwrap_or(&e.path)));
        }
        print!("],{nl}");
        print!("{sp}{sp}{sp}{sp}\"lighting\":{{\"Light\":{},{sp}\"RenderSettings\":{},{sp}\"LightmapSettings\":{},{sp}\"ReflectionProbe\":{},{sp}\"LightProbes\":{}}},{nl}",
            getc(classes::LIGHT), getc(classes::RENDER_SETTINGS), getc(classes::LIGHTMAP_SETTINGS),
            getc(classes::REFLECTION_PROBE), getc(classes::LIGHT_PROBES));
        print!("{sp}{sp}{sp}{sp}\"counts\":{{\"GameObject\":{},{sp}\"MeshFilter\":{},{sp}\"MeshRenderer\":{},{sp}\"SkinnedMeshRenderer\":{},{sp}\"MeshCollider\":{},{sp}\"BoxCollider\":{},{sp}\"MonoBehaviour\":{}}},{nl}",
            getc(classes::GAME_OBJECT), getc(classes::MESH_FILTER), getc(classes::MESH_RENDERER),
            getc(classes::SKINNED_MESH_RENDERER), getc(classes::MESH_COLLIDER), getc(classes::BOX_COLLIDER),
            getc(classes::MONO_BEHAVIOUR));
        print!("{sp}{sp}{sp}{sp}\"roots\":[");
        for (k, r) in roots.iter().enumerate() {
            if k > 0 { print!(","); }
            print!("{}", jstr(r));
        }
        print!("]{nl}{sp}{sp}}}");
        if i + 1 < scenes.len() { print!(","); }
        print!("{nl}");
    }
    print!("]{nl}");
}
