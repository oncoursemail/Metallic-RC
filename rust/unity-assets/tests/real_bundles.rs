//! Reads the owner's installed Rec Room bundles at test time (nothing is copied into the repo).
//! Skips when the install is missing. `RECROOM_AA` overrides the `StreamingAssets/aa` folder;
//! `UNITY_ASSETS_ALL=1` checks every bundle instead of a sample.

use std::path::PathBuf;
use unity_assets::catalog::{bundle_file, Catalog};
use unity_assets::*;

const DEFAULT_AA: &str = r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711\RecRoom_Data\StreamingAssets\aa";

fn aa_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("RECROOM_AA").map(PathBuf::from).unwrap_or_else(|| DEFAULT_AA.into());
    if dir.join("StandaloneWindows64").is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: {} not found", dir.display());
        None
    }
}

fn bundles(dir: &std::path::Path) -> Vec<(PathBuf, u64)> {
    let mut v: Vec<_> = std::fs::read_dir(dir.join("StandaloneWindows64"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "bundle"))
        .map(|e| (e.path(), e.metadata().unwrap().len()))
        .collect();
    v.sort();
    v
}

/// Parses one bundle fully; returns the number of objects read.
fn check_bundle(path: &std::path::Path) -> Result<usize> {
    let b = Bundle::open(path)?;
    let mut n = 0;
    for (_, sf) in b.serialized_files()? {
        for o in &sf.objects {
            let v = sf.read_value(o)?;
            n += 1;
            match sf.type_name(o).as_str() {
                "GameObject" => drop(GameObject::from_value(&v)?),
                "Transform" | "RectTransform" => drop(Transform::from_value(&v)?),
                "MeshFilter" => drop(MeshFilter::from_value(&v)?),
                "MeshRenderer" | "SkinnedMeshRenderer" => drop(MeshRenderer::from_value(&v)?),
                "Material" => drop(Material::from_value(&v)?),
                "AssetBundle" => drop(AssetBundleInfo::from_value(&v)?),
                "Mesh" => match Mesh::from_value(&v, &b) {
                    Ok(m) => assert_eq!(m.positions().len(), m.vertex_count),
                    Err(Error::Unsupported(_)) => {}
                    Err(e) => return Err(e),
                },
                "Texture2D" => drop(Texture2D::from_value(&v, &b)?),
                _ => {}
            }
        }
    }
    Ok(n)
}

#[test]
fn real_bundles_parse() {
    let Some(dir) = aa_dir() else { return };
    let all = bundles(&dir);
    assert!(!all.is_empty());
    let sample: Vec<_> = if std::env::var_os("UNITY_ASSETS_ALL").is_some() {
        all
    } else {
        // every 40th bundle under 20 MB keeps an unoptimised test run short
        all.into_iter().filter(|b| b.1 < 20 << 20).step_by(40).collect()
    };
    let mut failures = Vec::new();
    let mut objects = 0;
    for (p, _) in &sample {
        match check_bundle(p) {
            Ok(n) => objects += n,
            Err(e) => failures.push(format!("{}: {e}", p.display())),
        }
    }
    eprintln!("{} bundles, {objects} objects, {} failures", sample.len(), failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn real_catalog_points_at_existing_bundles() {
    let Some(dir) = aa_dir() else { return };
    let cat = Catalog::open(dir.join("catalog.bin")).unwrap();
    let entries = cat.entries().unwrap();
    assert!(entries.len() > 1000);
    let mut missing = 0;
    let mut bundled = 0;
    for l in entries.iter().flat_map(|e| &e.locations) {
        if let Some(f) = bundle_file(l) {
            bundled += 1;
            missing += !dir.join("StandaloneWindows64").join(f).is_file() as usize;
        }
    }
    eprintln!("{} keys, {bundled} locations with a bundle, {missing} missing", entries.len());
    assert_eq!(missing, 0);
}
