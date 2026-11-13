//! List every object in an avatar bundle: class, name, and for meshes the submesh/vertex counts, so we can find the
//! real face/head mesh and the face feature transforms (instead of eyeballing decal positions).
//! usage: cargo run --profile fast -p metallic-rc-client --example dump_bundle -- <bundle path>
use unity_assets::{Bundle, Mesh as UMesh};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711\RecRoom_Data\StreamingAssets\aa\StandaloneWindows64\edf7f2d64a5698a6cec7ec8a1f821ab8.bundle".into()
    });
    let b = Bundle::open(std::path::Path::new(&path)).expect("open bundle");
    let files = b.serialized_files().expect("serialized files");
    for (_, sf) in &files {
        for o in &sf.objects {
            let name = sf.object_name(o).unwrap_or_default();
            // Unity class ids: GameObject=1, Transform=4, Material=21, MeshRenderer=23, MeshFilter=33, Mesh=43,
            // Texture2D=28, SkinnedMeshRenderer=137
            let kind = match o.class_id {
                1 => "GameObject",
                4 => "Transform",
                21 => "Material",
                23 => "MeshRenderer",
                33 => "MeshFilter",
                43 => "Mesh",
                28 => "Texture2D",
                137 => "SkinnedMeshRenderer",
                _ => continue,
            };
            if o.class_id == 43 {
                if let Some(m) = sf.read_value(o).ok().and_then(|v| UMesh::from_value(&v, &b).ok()) {
                    println!("Mesh  pid={} name={:?} verts={} submeshes={}", o.path_id, name, m.vertex_count, m.submeshes.len());
                    continue;
                }
            }
            println!("{kind:<20} pid={} name={:?}", o.path_id, name);
        }
    }
}
