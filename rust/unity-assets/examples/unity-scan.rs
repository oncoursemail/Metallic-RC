//! Parses every bundle in a folder (container, serialized files, every object's value tree,
//! typed views) and prints counts with failures grouped by cause.
//!
//! `cargo run --release -p unity-assets --example unity-scan -- <bundle folder> [--no-decode] [--limit N]`

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use unity_assets::*;

#[derive(Default)]
struct Stats {
    bundles: usize,
    bundles_ok: usize,
    objects: usize,
    objects_ok: usize,
    errors: BTreeMap<String, (usize, String)>,
    typed: BTreeMap<String, (usize, usize)>,
    types: BTreeMap<String, usize>,
    flags: BTreeMap<String, usize>,
    scenes: Vec<String>,
    matches: Vec<String>,
}

impl Stats {
    fn err(&mut self, group: String, example: String) {
        let e = self.errors.entry(group).or_insert((0, example));
        e.0 += 1;
    }
}

/// Error text with numbers removed, so similar failures group together.
fn group(stage: &str, e: &Error) -> String {
    let s = e.to_string();
    let mut out = String::new();
    let mut digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !digit {
                out.push('#');
            }
            digit = true;
        } else {
            digit = false;
            out.push(c);
        }
    }
    format!("{stage}: {out}")
}

fn scan(path: &PathBuf, decode: bool, st: &Mutex<Stats>) {
    let file = path.file_name().unwrap().to_string_lossy().to_string();
    let b = match Bundle::open(path) {
        Ok(b) => b,
        Err(e) => {
            let mut s = st.lock().unwrap();
            s.bundles += 1;
            s.err(group("bundle", &e), file);
            return;
        }
    };
    let mut local = Stats::default();
    local.bundles = 1;
    *local.flags.entry(format!("flags={:#x} blocks={:?}", b.header.flags, b.block_compression())).or_default() += 1;
    let mut ok = true;
    match b.serialized_files() {
        Err(e) => {
            ok = false;
            local.err(group("serialized", &e), file.clone());
        }
        Ok(files) => {
            for n in &b.nodes {
                let l = n.path.to_lowercase();
                if l.contains("dorm") || l.contains("reccenter") || l.contains("rec_center") {
                    local.matches.push(format!("{file}: node {}", n.path));
                }
            }
            for (_, sf) in &files {
                for o in &sf.objects {
                    let tn = sf.type_name(o);
                    *local.types.entry(tn.clone()).or_default() += 1;
                    local.objects += 1;
                    let v = match sf.read_value(o) {
                        Ok(v) => v,
                        Err(e) => {
                            ok = false;
                            local.err(group(&format!("value {tn}"), &e), format!("{file} {}", o.path_id));
                            continue;
                        }
                    };
                    local.objects_ok += 1;
                    let typed: Option<Result<String>> = match tn.as_str() {
                        "GameObject" => Some(GameObject::from_value(&v).map(|g| g.name)),
                        "Transform" | "RectTransform" => Some(Transform::from_value(&v).map(|_| String::new())),
                        "MeshFilter" => Some(MeshFilter::from_value(&v).map(|_| String::new())),
                        "MeshRenderer" | "SkinnedMeshRenderer" => Some(MeshRenderer::from_value(&v).map(|_| String::new())),
                        "Material" => Some(Material::from_value(&v).map(|m| m.name)),
                        "Mesh" => Some(Mesh::from_value(&v, &b).and_then(|m| {
                            let n = m.positions().len();
                            if n != m.vertex_count {
                                return Err(Error::Format(format!("{n} positions of {}", m.vertex_count)));
                            }
                            for i in 0..m.submeshes.len() {
                                let idx = m.submesh_indices(i).ok_or(Error::Format("submesh range".into()))?;
                                if idx.iter().any(|&x| x as usize >= n) {
                                    return Err(Error::Format("index out of range".into()));
                                }
                            }
                            Ok(m.name)
                        })),
                        "Texture2D" => Some(Texture2D::from_value(&v, &b).and_then(|t| {
                            if decode {
                                t.decode_rgba8().map(|_| t.name)
                            } else {
                                Ok(t.name)
                            }
                        })),
                        "AssetBundle" => Some(AssetBundleInfo::from_value(&v).map(|a| {
                            for (p, _) in &a.container {
                                let l = p.to_lowercase();
                                if a.is_scene || l.ends_with(".unity") {
                                    local.scenes.push(format!("{file}\t{p}"));
                                }
                                if l.contains("dorm") || l.contains("reccenter") || l.contains("rec center") || l.contains("rec_center") {
                                    local.matches.push(format!("{file}: {p}"));
                                }
                            }
                            a.name
                        })),
                        _ => None,
                    };
                    if let Some(r) = typed {
                        let e = local.typed.entry(tn.clone()).or_default();
                        match r {
                            Ok(_) => e.0 += 1,
                            Err(err) => {
                                e.1 += 1;
                                local.err(group(&format!("typed {tn}"), &err), format!("{file} {}", o.path_id));
                            }
                        }
                    }
                }
            }
        }
    }
    let mut s = st.lock().unwrap();
    s.bundles += 1;
    s.bundles_ok += ok as usize;
    s.objects += local.objects;
    s.objects_ok += local.objects_ok;
    for (k, (n, ex)) in local.errors {
        s.errors.entry(k).or_insert((0, ex)).0 += n;
    }
    for (k, (a, b2)) in local.typed {
        let e = s.typed.entry(k).or_default();
        e.0 += a;
        e.1 += b2;
    }
    for (k, n) in local.types {
        *s.types.entry(k).or_default() += n;
    }
    for (k, n) in local.flags {
        *s.flags.entry(k).or_default() += n;
    }
    s.scenes.extend(local.scenes);
    s.matches.extend(local.matches);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("usage: unity-scan <bundle folder> [--no-decode] [--limit N]"));
    let (mut decode, mut limit) = (true, usize::MAX);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--no-decode" => decode = false,
            "--limit" => limit = args.next().unwrap().parse().unwrap(),
            _ => panic!("unknown argument {a}"),
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bundle"))
        .collect();
    files.sort();
    files.truncate(limit);
    let st = Mutex::new(Stats::default());
    let next = Mutex::new(0usize);
    let start = std::time::Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = {
                    let mut n = next.lock().unwrap();
                    *n += 1;
                    *n - 1
                };
                let Some(f) = files.get(i) else { break };
                scan(f, decode, &st);
            });
        }
    });
    let s = st.into_inner().unwrap();
    println!("bundles: {} parsed fully: {} ({:.1}s)", s.bundles, s.bundles_ok, start.elapsed().as_secs_f32());
    println!("objects: {} value trees read: {}", s.objects, s.objects_ok);
    println!("\nheader flags / block compression (1=LZMA 2=LZ4 3=LZ4HC):");
    for (k, n) in &s.flags {
        println!("  {n:6}  {k}");
    }
    println!("\ntyped extraction (ok, failed):");
    for (k, (a, b)) in &s.typed {
        println!("  {k:22} {a:8} {b:6}");
    }
    println!("\nerrors grouped (count, example):");
    for (k, (n, ex)) in &s.errors {
        println!("  {n:6}  {k}   [{ex}]");
    }
    let mut types: Vec<_> = s.types.iter().collect();
    types.sort_by(|a, b| b.1.cmp(a.1));
    println!("\ntop object types:");
    for (k, n) in types.iter().take(25) {
        println!("  {n:8}  {k}");
    }
    let mut scenes = s.scenes;
    scenes.sort();
    println!("\nscene entries: {}", scenes.len());
    for l in &scenes {
        println!("  {l}");
    }
    let mut m = s.matches;
    m.sort();
    println!("\ndorm / rec center matches: {}", m.len());
    for l in &m {
        println!("  {l}");
    }
}
