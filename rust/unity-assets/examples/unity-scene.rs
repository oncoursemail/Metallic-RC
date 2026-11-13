//! Prints a built scene's hierarchy from a plain SerializedFile (`levelN`, with or without type trees).
//!
//! `unity-scene <level file> [--resources-dir DIR] [--json OUT]`
//! Externals (`sharedassetsN.assets`, `resources.assets`, ...) are opened from DIR (default: the level's
//! folder) when they are referenced; `unity default resources` / `unity_builtin_extra` are skipped.
//! Positions, rotations (quaternion x y z w) and scales are in Unity space.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use unity_assets::*;

struct Files {
    dir: PathBuf,
    externals: Vec<External>,
    open: HashMap<usize, Option<SerializedFile>>,
}

impl Files {
    /// The file a PPtr points into: `None` for file 0 (the caller uses the scene) or a missing file.
    fn get(&mut self, file_id: i32) -> Option<&SerializedFile> {
        let i = file_id as usize - 1;
        let dir = &self.dir;
        let ext = self.externals.get(i)?;
        self.open
            .entry(i)
            .or_insert_with(|| {
                let lower = ext.path.to_ascii_lowercase();
                if lower.contains("unity default resources") || lower.contains("unity_builtin_extra") {
                    return None;
                }
                let name = Path::new(&ext.path).file_name()?;
                let data = std::fs::read(dir.join(name)).ok()?;
                SerializedFile::parse(data).map_err(|e| eprintln!("{}: {e}", ext.path)).ok()
            })
            .as_ref()
    }
}

fn name_of(scene: &SerializedFile, files: &mut Files, p: PPtr) -> String {
    if p.is_null() {
        return String::new();
    }
    let ext = if p.file_id == 0 { "scene".to_string() } else { files.externals.get(p.file_id as usize - 1).map_or("?".into(), |e| e.path.clone()) };
    let sf = if p.file_id == 0 { Some(scene) } else { files.get(p.file_id) };
    sf.and_then(|f| f.object(p.path_id).and_then(|o| f.object_name(o)))
        .unwrap_or_else(|| format!("<{}:{}>", ext, p.path_id))
}

type Q = [f32; 4];
type V = [f32; 3];

fn qmul(a: Q, b: Q) -> Q {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn qrot(q: Q, v: V) -> V {
    let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
    [p[0], p[1], p[2]]
}

#[derive(Clone, Copy)]
struct World {
    pos: V,
    rot: Q,
    scale: V,
    active: bool,
}

struct Row {
    path: String,
    go: i64,
    active: bool,
    active_in_hierarchy: bool,
    local: Transform,
    world: World,
    renderer_enabled: Option<bool>,
    mesh: Option<String>,
    materials: Vec<String>,
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o += "\\\"",
            '\\' => o += "\\\\",
            c if (c as u32) < 0x20 => o += &format!("\\u{:04x}", c as u32),
            c => o.push(c),
        }
    }
    o + "\""
}

fn json_f(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|f| if f.is_finite() { format!("{f}") } else { "null".into() }).collect();
    format!("[{}]", parts.join(","))
}

fn main() {
    let usage = "usage: unity-scene <level file> [--resources-dir DIR] [--json OUT]";
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect(usage));
    let (mut dir, mut json) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--resources-dir" => dir = args.next().map(PathBuf::from),
            "--json" => json = args.next().map(PathBuf::from),
            _ => panic!("{usage}"),
        }
    }
    let dir = dir.unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).to_path_buf());
    let scene = SerializedFile::parse(std::fs::read(&path).expect("read level")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    println!(
        "{}: Unity {} format {} type trees {} objects {}",
        path.display(),
        scene.unity_version,
        scene.version,
        scene.type_tree_enabled,
        scene.objects.len()
    );
    for (i, e) in scene.externals.iter().enumerate() {
        println!("  external {} {}", i + 1, e.path);
    }
    let mut files = Files { dir, externals: scene.externals.clone(), open: HashMap::new() };

    // transforms by path id, game objects with their transform
    let mut transforms: HashMap<i64, Transform> = HashMap::new();
    let mut gos: HashMap<i64, GameObject> = HashMap::new();
    let mut failed = 0;
    for o in &scene.objects {
        match o.class_id {
            classes::TRANSFORM | classes::RECT_TRANSFORM => match scene.transform(o) {
                Ok(t) => drop(transforms.insert(o.path_id, t)),
                Err(e) => {
                    failed += 1;
                    eprintln!("transform {}: {e}", o.path_id);
                }
            },
            classes::GAME_OBJECT => match scene.game_object(o) {
                Ok(g) => drop(gos.insert(o.path_id, g)),
                Err(e) => {
                    failed += 1;
                    eprintln!("game object {}: {e}", o.path_id);
                }
            },
            _ => {}
        }
    }
    println!("{} game objects, {} transforms, {failed} read errors", gos.len(), transforms.len());

    // world transforms, root first
    let mut world: HashMap<i64, World> = HashMap::new();
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    let mut roots: Vec<i64> = transforms.iter().filter(|(_, t)| t.parent.is_null()).map(|(&id, _)| id).collect();
    roots.sort_by_key(|id| scene.objects.iter().position(|o| o.path_id == *id));
    let mut stack: Vec<(i64, World, String)> = roots
        .iter()
        .rev()
        .map(|&id| (id, World { pos: [0.0; 3], rot: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], active: true }, String::new()))
        .collect();
    while let Some((id, parent, prefix)) = stack.pop() {
        let Some(t) = transforms.get(&id) else { continue };
        let go = gos.get(&t.game_object.path_id);
        let name = go.map_or("?".to_string(), |g| g.name.clone());
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let s = parent.scale;
        let lp = t.local_position;
        let off = qrot(parent.rot, [lp[0] * s[0], lp[1] * s[1], lp[2] * s[2]]);
        let w = World {
            pos: [parent.pos[0] + off[0], parent.pos[1] + off[1], parent.pos[2] + off[2]],
            rot: qmul(parent.rot, t.local_rotation),
            scale: [s[0] * t.local_scale[0], s[1] * t.local_scale[1], s[2] * t.local_scale[2]],
            active: parent.active && go.is_none_or(|g| g.active),
        };
        world.insert(id, w);
        names.insert(id, path.clone());
        order.push(id);
        for c in t.children.iter().rev() {
            stack.push((c.path_id, w, path.clone()));
        }
    }

    let mut rows = Vec::new();
    for id in order {
        let t = &transforms[&id];
        let Some(g) = gos.get(&t.game_object.path_id) else { continue };
        let (mut filter, mut renderer) = (None, None);
        for c in &g.components {
            if c.file_id != 0 {
                continue;
            }
            let Some(o) = scene.object(c.path_id) else { continue };
            match o.class_id {
                classes::MESH_FILTER => filter = scene.mesh_filter(o).ok(),
                classes::MESH_RENDERER => renderer = scene.mesh_renderer(o).map_err(|e| eprintln!("renderer {}: {e}", o.path_id)).ok(),
                _ => {}
            }
        }
        let mesh = filter.map(|f| name_of(&scene, &mut files, f.mesh));
        let materials = renderer.as_ref().map(|r| r.materials.iter().map(|&m| name_of(&scene, &mut files, m)).collect()).unwrap_or_default();
        rows.push(Row {
            path: names[&id].clone(),
            go: t.game_object.path_id,
            active: g.active,
            active_in_hierarchy: world[&id].active,
            local: t.clone(),
            world: world[&id],
            renderer_enabled: renderer.map(|r| r.enabled),
            mesh,
            materials,
        });
    }

    let f = |v: &[f32]| v.iter().map(|x| format!("{x:8.3}")).collect::<Vec<_>>().join("");
    for r in &rows {
        println!(
            "{}{} [{}]\n    local pos{} rot{} scale{}\n    world pos{} rot{} scale{}",
            r.path,
            if r.active_in_hierarchy { "" } else { "  (inactive)" },
            r.go,
            f(&r.local.local_position),
            f(&r.local.local_rotation),
            f(&r.local.local_scale),
            f(&r.world.pos),
            f(&r.world.rot),
            f(&r.world.scale)
        );
        if let Some(m) = &r.mesh {
            println!("    mesh {m}  materials {:?}{}", r.materials, if r.renderer_enabled == Some(false) { " (renderer off)" } else { "" });
        }
    }

    if let Some(out) = json {
        let mut s = String::from("[\n");
        for (i, r) in rows.iter().enumerate() {
            let mats: Vec<String> = r.materials.iter().map(|m| json_str(m)).collect();
            s += &format!(
                "  {{\"path\":{},\"game_object\":{},\"active\":{},\"active_in_hierarchy\":{},\"local_position\":{},\"local_rotation\":{},\"local_scale\":{},\"world_position\":{},\"world_rotation\":{},\"world_scale\":{},\"mesh\":{},\"renderer_enabled\":{},\"materials\":[{}]}}{}\n",
                json_str(&r.path),
                r.go,
                r.active,
                r.active_in_hierarchy,
                json_f(&r.local.local_position),
                json_f(&r.local.local_rotation),
                json_f(&r.local.local_scale),
                json_f(&r.world.pos),
                json_f(&r.world.rot),
                json_f(&r.world.scale),
                r.mesh.as_deref().map_or("null".into(), json_str),
                r.renderer_enabled.map_or("null".into(), |b| b.to_string()),
                mats.join(","),
                if i + 1 < rows.len() { "," } else { "" }
            );
        }
        s += "]\n";
        std::fs::write(&out, s).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
        println!("wrote {} rows to {}", rows.len(), out.display());
    }
}
