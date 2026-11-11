//! Dump a compressed mesh's `m_CompressedMesh` PackedBitVector structure.
//! `inspect-cmesh <bundle>`
use unity_assets::*;

fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect-cmesh <bundle>");
    let b = Bundle::open(&path).unwrap();
    for (_, sf) in b.serialized_files().unwrap() {
        for o in &sf.objects {
            if sf.type_name(o) != "Mesh" { continue; }
            let v = match sf.read_value(o) { Ok(v) => v, Err(_) => continue };
            let comp = v.get("m_MeshCompression").and_then(Value::as_i64).unwrap_or(0);
            if comp == 0 { continue; }
            let name = v.get("m_Name").and_then(Value::as_str).unwrap_or("");
            println!("MESH {name} path_id {} m_MeshCompression={comp}", o.path_id);
            println!("  vertexCount(m_VertexData)={:?}  indexFormat={:?}  submeshes={:?}",
                v.path("m_VertexData/m_VertexCount").and_then(Value::as_i64),
                v.get("m_IndexFormat").and_then(Value::as_i64),
                v.get("m_SubMeshes").and_then(Value::as_array).map(<[_]>::len));
            let cm = match v.get("m_CompressedMesh") { Some(c) => c, None => { println!("  NO m_CompressedMesh"); continue } };
            for fld in ["m_Vertices","m_UV","m_Normals","m_Tangents","m_Weights","m_NormalSigns",
                        "m_TangentSigns","m_FloatColors","m_BoneIndices","m_Triangles","m_Colors","m_UVInfo"] {
                if let Some(pbv) = cm.get(fld) {
                    let ni = pbv.get("m_NumItems").and_then(Value::as_i64);
                    let bs = pbv.get("m_BitSize").and_then(Value::as_i64);
                    let rng = pbv.get("m_Range").and_then(Value::as_f32);
                    let st = pbv.get("m_Start").and_then(Value::as_f32);
                    let dl = pbv.get("m_Data").and_then(Value::as_bytes).map(<[_]>::len);
                    println!("  {fld:14} NumItems={ni:?} BitSize={bs:?} Range={rng:?} Start={st:?} DataLen={dl:?}");
                } else {
                    println!("  {fld:14} (absent)");
                }
            }
            return;
        }
    }
    println!("no compressed mesh found in this bundle");
}
