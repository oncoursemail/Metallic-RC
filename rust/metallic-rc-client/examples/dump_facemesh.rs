//! Inspect the real FacialSpritesMesh (authored face-screen geometry): split it into connected islands (eye L, eye R,
//! mouth, ...) and print each island's position center/size + UV bounds, so we can place the eye/mouth sprites exactly.
//! usage: cargo run --profile fast -p metallic-rc-client --example dump_facemesh
use unity_assets::{Bundle, Mesh as UMesh};

const BUNDLE: &str = r"C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711\RecRoom_Data\StreamingAssets\aa\StandaloneWindows64\edf7f2d64a5698a6cec7ec8a1f821ab8.bundle";
const FACE_MESH_PID: i64 = -3450666020669679783; // FacialSpritesMesh_LOD0 (the Mesh, not the GameObject)

fn main() {
    let b = Bundle::open(std::path::Path::new(BUNDLE)).expect("open");
    let files = b.serialized_files().expect("files");
    let sf = files.iter().find_map(|(_, sf)| sf.object(FACE_MESH_PID).map(|_| sf)).expect("mesh in bundle");
    let v = sf.read_value(sf.object(FACE_MESH_PID).unwrap()).expect("read");
    let m = UMesh::from_value(&v, &b).expect("mesh");
    let flip = |p: [f32; 3]| [p[0], p[1], -p[2]];
    let pos: Vec<[f32; 3]> = m.positions().into_iter().map(flip).collect();
    let uv = m.uv(0);
    let n = pos.len();
    println!("verts={} submeshes={}", n, m.submeshes.len());

    // union-find over triangle edges to split into connected islands
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, x: usize) -> usize { if p[x] != x { let r = find(p, p[x]); p[x] = r; } p[x] }
    let mut idx = Vec::new();
    for i in 0..m.submeshes.len() { if let Some(s) = m.submesh_indices(i) { idx.extend(s); } }
    for t in idx.chunks_exact(3) {
        let (a, b_, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        for &(x, y) in &[(a, b_), (b_, c)] { let (rx, ry) = (find(&mut parent, x), find(&mut parent, y)); parent[rx] = ry; }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..n { let r = find(&mut parent, i); groups.entry(r).or_default().push(i); }
    println!("islands: {}", groups.len());
    let mut islands: Vec<Vec<usize>> = groups.into_values().collect();
    islands.sort_by(|a, b| b.len().cmp(&a.len()));
    for (gi, g) in islands.iter().enumerate() {
        let (mut pmin, mut pmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        let (mut umin, mut umax) = ([f32::MAX; 2], [f32::MIN; 2]);
        for &vi in g {
            let p = pos[vi];
            for k in 0..3 { pmin[k] = pmin[k].min(p[k]); pmax[k] = pmax[k].max(p[k]); }
            if let Some(&t) = uv.get(vi) { for k in 0..2 { umin[k] = umin[k].min(t[k]); umax[k] = umax[k].max(t[k]); } }
        }
        let c = [(pmin[0]+pmax[0])/2.0, (pmin[1]+pmax[1])/2.0, (pmin[2]+pmax[2])/2.0];
        println!("island {gi}: {} verts  center=({:.4},{:.4},{:.4}) size=({:.4},{:.4},{:.4})  uv u[{:.3},{:.3}] v[{:.3},{:.3}]",
            g.len(), c[0],c[1],c[2], pmax[0]-pmin[0], pmax[1]-pmin[1], pmax[2]-pmin[2], umin[0],umax[0], umin[1],umax[1]);
    }
}
