//! Unit tests on small inputs built here (no game data).

use crate::catalog::Catalog;
use crate::serialized::{common_string, COMMON_STRINGS};
use crate::*;
use std::sync::Arc;

/// Little byte writer for building test files.
#[derive(Default)]
struct W {
    b: Vec<u8>,
    be: bool,
}

impl W {
    fn new(be: bool) -> Self {
        W { b: Vec::new(), be }
    }
    fn u8(&mut self, v: u8) -> &mut Self {
        self.b.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.raw(&if self.be { v.to_be_bytes() } else { v.to_le_bytes() })
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.raw(&if self.be { v.to_be_bytes() } else { v.to_le_bytes() })
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.u32(v as u32)
    }
    fn i64(&mut self, v: i64) -> &mut Self {
        self.raw(&if self.be { v.to_be_bytes() } else { v.to_le_bytes() })
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.u32(v.to_bits())
    }
    fn raw(&mut self, v: &[u8]) -> &mut Self {
        self.b.extend_from_slice(v);
        self
    }
    fn cstr(&mut self, s: &str) -> &mut Self {
        self.raw(s.as_bytes()).u8(0)
    }
    fn align(&mut self, n: usize) -> &mut Self {
        while self.b.len() % n != 0 {
            self.b.push(0);
        }
        self
    }
    fn string(&mut self, s: &str) -> &mut Self {
        self.i32(s.len() as i32).raw(s.as_bytes()).align(4)
    }
}

#[test]
fn reader_endianness_strings_align() {
    let data = [0, 0, 1, 2, 2, 1, 0, 0, b'h', b'i', 0, 9];
    let mut r = Reader::new(&data, true);
    assert_eq!(r.u32().unwrap(), 0x0102);
    r.big_endian = false;
    assert_eq!(r.u32().unwrap(), 0x0102);
    assert_eq!(r.cstr().unwrap(), "hi");
    r.align(4);
    assert_eq!(r.pos(), 12);
    assert!(matches!(r.u8(), Err(Error::Eof { .. })));
}

#[test]
fn common_strings_offsets() {
    assert_eq!(common_string(0), Some("AABB"));
    assert_eq!(common_string(5), Some("AnimationClip"));
    // Offsets Unity writes for a few well-known names.
    assert_eq!(common_string(427), Some("m_Name"));
    assert_eq!(common_string(1169), Some("RenderingLayerMask"));
    assert_eq!(common_string(1), None);
    assert_eq!(COMMON_STRINGS.len(), 110);
}

/// Type tree node: (level, type, name, is_array, align).
type N<'a> = (u8, &'a str, &'a str, bool, bool);

fn tree_blob(w: &mut W, nodes: &[N]) {
    let mut strings = Vec::<u8>::new();
    let mut off = |s: &str| -> u32 {
        let mut at = 0u32;
        for c in COMMON_STRINGS {
            if *c == s {
                return at | 0x8000_0000;
            }
            at += c.len() as u32 + 1;
        }
        let o = strings.len() as u32;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        o
    };
    let raw: Vec<(N, u32, u32)> = nodes.iter().map(|n| (*n, off(n.1), off(n.2))).collect();
    w.i32(nodes.len() as i32).i32(strings.len() as i32);
    for (i, ((level, ty, _, arr, align), t, n)) in raw.into_iter().enumerate() {
        let size = match ty {
            "int" | "float" => 4,
            "SInt64" => 8,
            "bool" | "char" | "UInt8" => 1,
            _ => -1,
        };
        w.u16(1).u8(level).u8(arr as u8).u32(t).u32(n).i32(size).i32(i as i32);
        w.u32(if align { 0x4000 } else { 0 }).raw(&[0; 8]);
    }
    w.raw(&strings);
}

const TEST_TREE: &[N] = &[
    (0, "TestObj", "Base", false, false),
    (1, "string", "m_Name", false, false),
    (2, "Array", "Array", true, true),
    (3, "int", "size", false, false),
    (3, "char", "data", false, false),
    (1, "int", "m_Count", false, false),
    (1, "bool", "m_Flag", false, true),
    (1, "vector", "m_Values", false, false),
    (2, "Array", "Array", true, true),
    (3, "int", "size", false, false),
    (3, "float", "data", false, false),
    (1, "PPtr<Material>", "m_Mat", false, false),
    (2, "int", "m_FileID", false, false),
    (2, "SInt64", "m_PathID", false, false),
    (1, "map", "m_Map", false, false),
    (2, "Array", "Array", true, true),
    (3, "int", "size", false, false),
    (3, "pair", "data", false, false),
    (4, "string", "first", false, false),
    (5, "Array", "Array", true, true),
    (6, "int", "size", false, false),
    (6, "char", "data", false, false),
    (4, "int", "second", false, false),
    (1, "TypelessData", "m_Blob", false, true),
    (2, "int", "size", false, false),
    (2, "UInt8", "data", false, false),
];

fn test_object(extra: usize) -> Vec<u8> {
    let mut o = W::new(false);
    o.string("hello").i32(-7).u8(1).align(4);
    o.i32(2).f32(1.5).f32(-2.0);
    o.i32(1).i64(12345);
    o.i32(1).string("k").i32(42);
    o.i32(3).raw(&[1, 2, 3]).align(4);
    o.raw(&vec![0; extra]);
    o.b
}

/// A version 22 SerializedFile with one object of `TEST_TREE` (plus `extra` trailing bytes).
fn test_serialized_file(extra: usize) -> Vec<u8> {
    let obj = test_object(extra);
    let mut m = W::new(false);
    m.cstr("6000.0.27f1").i32(19).u8(1);
    m.i32(1); // types
    m.i32(999).u8(0).raw(&(-1i16).to_le_bytes()).raw(&[0; 16]);
    tree_blob(&mut m, TEST_TREE);
    m.i32(0); // type dependencies
    m.i32(1); // objects
    let header_len = 48;
    m.align(4); // alignment is relative to the file start; header_len is a multiple of 4
    m.i64(-42).i64(0).u32(obj.len() as u32).i32(0);
    m.i32(0); // script types
    m.i32(1).cstr("").raw(&[7; 16]).i32(0).cstr("archive:/CAB-other/CAB-other");
    m.i32(0); // ref types
    m.cstr("");
    let data_offset = (header_len + m.b.len()).div_ceil(16) * 16;
    let mut f = W::new(true);
    f.u32(0).u32(0).u32(22).u32(0).u8(0).raw(&[0; 3]);
    f.u32(m.b.len() as u32).i64((data_offset + obj.len()) as i64).i64(data_offset as i64).i64(0);
    assert_eq!(f.b.len(), header_len);
    f.raw(&m.b).align(16).raw(&obj);
    f.b
}

#[test]
fn serialized_file_and_value_tree() {
    let sf = SerializedFile::parse(test_serialized_file(0)).unwrap();
    assert_eq!(sf.version, 22);
    assert_eq!(sf.unity_version, "6000.0.27f1");
    assert!(sf.type_tree_enabled);
    assert_eq!(sf.objects.len(), 1);
    assert_eq!(sf.externals[0].path, "archive:/CAB-other/CAB-other");
    let o = &sf.objects[0];
    assert_eq!((o.path_id, o.class_id), (-42, 999));
    assert_eq!(sf.type_name(o), "TestObj");
    assert_eq!(sf.object_name(o).as_deref(), Some("hello"));
    let v = sf.read_value(o).unwrap();
    assert_eq!(v.get("m_Name").unwrap().as_str(), Some("hello"));
    assert_eq!(v.get("m_Count").unwrap().as_i64(), Some(-7));
    assert_eq!(v.get("m_Flag").unwrap().as_bool(), Some(true));
    assert_eq!(v.get("m_Values").unwrap().floats(), vec![1.5, -2.0]);
    assert_eq!(v.get("m_Mat").unwrap().as_pptr(), Some(PPtr { file_id: 1, path_id: 12345 }));
    let map = v.get("m_Map").unwrap().pairs();
    assert_eq!(map.len(), 1);
    assert_eq!((map[0].0.as_str(), map[0].1.as_i64()), (Some("k"), Some(42)));
    assert_eq!(v.get("m_Blob").unwrap().as_bytes(), Some(&[1u8, 2, 3][..]));
}

#[test]
fn serialized_file_rejects_truncated_and_unconsumed_objects() {
    let mut f = test_serialized_file(0);
    let n = f.len();
    f.truncate(n - 4); // object table now points past the end
    assert!(SerializedFile::parse(f).is_err());
    let sf = SerializedFile::parse(test_serialized_file(4)).unwrap();
    assert!(matches!(sf.read_value(&sf.objects[0]), Err(Error::Format(_))));
}

#[test]
fn type_tree_children() {
    let nodes: Vec<TypeNode> = TEST_TREE
        .iter()
        .map(|&(level, t, n, arr, _)| TypeNode {
            version: 1,
            level,
            type_flags: arr as u8,
            type_name: Arc::from(t),
            name: Arc::from(n),
            byte_size: 0,
            index: 0,
            meta_flag: 0,
            ref_type_hash: 0,
        })
        .collect();
    let t = TypeTree::new(nodes);
    let top: Vec<&str> = t.children(0).map(|i| &*t.nodes[i].name).collect();
    assert_eq!(top, ["m_Name", "m_Count", "m_Flag", "m_Values", "m_Mat", "m_Map", "m_Blob"]);
    assert_eq!(t.ends[0], TEST_TREE.len());
}

/// Builds a UnityFS bundle holding `nodes`, split into blocks of `block` bytes compressed
/// round-robin with `kinds` (0 stored, 2 LZ4, 1 LZMA).
fn build_bundle(nodes: &[(&str, &[u8], u32)], block: usize, kinds: &[u32], info_at_end: bool, extra_flags: u32) -> Vec<u8> {
    let stream: Vec<u8> = nodes.iter().flat_map(|n| n.1.iter().copied()).collect();
    let mut blocks = Vec::new();
    for (i, chunk) in stream.chunks(block).enumerate() {
        let kind = kinds[i % kinds.len()];
        let c = match kind {
            0 => chunk.to_vec(),
            2 => lz4_flex::block::compress(chunk),
            1 => {
                let mut out = Vec::new();
                lzma_rs::lzma_compress(&mut &chunk[..], &mut out).unwrap();
                out.drain(5..13); // Unity omits the 8-byte size field
                out
            }
            _ => unreachable!(),
        };
        blocks.push((chunk.len() as u32, c, kind as u16));
    }
    let mut info = W::new(true);
    info.raw(&[0; 16]).i32(blocks.len() as i32);
    for (size, c, kind) in &blocks {
        info.u32(*size).u32(c.len() as u32).u16(*kind);
    }
    info.i32(nodes.len() as i32);
    let mut off = 0i64;
    for (name, data, flags) in nodes {
        info.i64(off).i64(data.len() as i64).u32(*flags).cstr(name);
        off += data.len() as i64;
    }
    let info_c = lz4_flex::block::compress(&info.b);
    let mut flags = 3 | 0x40 | 0x200 | extra_flags;
    if info_at_end {
        flags |= 0x80;
    }
    let mut f = W::new(true);
    f.cstr("UnityFS").u32(8).cstr("5.x.x").cstr("0.0.0").i64(0).u32(info_c.len() as u32).u32(info.b.len() as u32).u32(flags);
    f.align(16);
    if !info_at_end {
        f.raw(&info_c);
    }
    f.align(16);
    for (_, c, _) in &blocks {
        f.raw(c);
    }
    if info_at_end {
        f.raw(&info_c);
    }
    f.b
}

#[test]
fn bundle_blocks_and_nodes() {
    let sf = test_serialized_file(0);
    let res: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
    for at_end in [false, true] {
        let file = build_bundle(&[("CAB-test", &sf, 4), ("CAB-test.resS", &res, 0)], 333, &[0, 2, 1], at_end, 0);
        let b = Bundle::from_bytes(file).unwrap();
        assert_eq!(b.header.format, 8);
        assert_eq!(b.nodes.len(), 2);
        assert_eq!(b.block_compression().len(), 3);
        assert_eq!(b.read_node(&b.nodes[0]).unwrap(), sf);
        assert_eq!(b.read_node(&b.nodes[1]).unwrap(), res);
        let n = b.node("archive:/CAB-test/CAB-test.resS").unwrap();
        assert_eq!(b.read_node_range(n, 300, 400).unwrap(), &res[300..700]);
        assert!(b.read_node_range(n, 900, 200).is_err());
        let files = b.serialized_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.objects.len(), 1);
    }
}

#[test]
fn bundle_rejects_unknown_flags_and_signature() {
    let file = build_bundle(&[("a", b"xyz", 0)], 16, &[0], false, 0x400);
    assert!(matches!(Bundle::from_bytes(file), Err(Error::Unsupported(_))));
    assert!(matches!(Bundle::from_bytes(b"UnityWeb\0".to_vec()), Err(Error::Unsupported(_))));
}

fn s(v: &str) -> Value {
    Value::String(v.into())
}
fn i(v: i64) -> Value {
    Value::Int(v)
}
fn st(fields: Vec<(&str, Value)>) -> Value {
    Value::Struct { type_name: "T".into(), fields: fields.into_iter().map(|(k, v)| (Arc::from(k), v)).collect() }
}

struct MapRes(Vec<u8>);
impl Resources for MapRes {
    fn resource(&self, path: &str, offset: u64, size: u64) -> Result<Vec<u8>> {
        assert_eq!(path, "archive:/CAB-x/CAB-x.resS");
        Ok(self.0[offset as usize..(offset + size) as usize].to_vec())
    }
}

fn half(f: f32) -> [u8; 2] {
    // exact for the small values used here
    let b = f.to_bits();
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let h = ((b >> 16) & 0x8000) | if f == 0.0 { 0 } else { ((exp as u32) << 10) | ((b >> 13) & 0x3ff) };
    (h as u16).to_le_bytes()
}

#[test]
fn mesh_streams_channels_indices() {
    // stream 0: float3 position (3 vertices x 12 bytes = 36, padded to 48); stream 1: half2 uv0
    let mut vd = W::new(false);
    for p in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.5]] {
        for c in p {
            vd.f32(c);
        }
    }
    vd.align(16);
    for uv in [[0.0f32, 0.0], [1.0, 0.0], [0.5, 1.0]] {
        vd.raw(&half(uv[0])).raw(&half(uv[1]));
    }
    let chan = |stream, format, dim| st(vec![("stream", i(stream)), ("offset", i(0)), ("format", i(format)), ("dimension", i(dim))]);
    let mut channels = vec![chan(0, 0, 3)];
    channels.extend((1..4).map(|_| chan(0, 0, 0)));
    channels.push(chan(1, 1, 2));
    let idx: Vec<u8> = [2u16, 1, 0].iter().flat_map(|x| x.to_le_bytes()).collect();
    let mesh_value = |inline: bool| {
        st(vec![
            ("m_Name", s("tri")),
            (
                "m_SubMeshes",
                Value::Array(vec![st(vec![
                    ("firstByte", i(0)),
                    ("indexCount", i(3)),
                    ("topology", i(0)),
                    ("baseVertex", i(0)),
                    ("firstVertex", i(0)),
                    ("vertexCount", i(3)),
                ])]),
            ),
            ("m_MeshCompression", i(0)),
            ("m_IndexFormat", i(0)),
            ("m_IndexBuffer", Value::Bytes(idx.clone())),
            (
                "m_VertexData",
                st(vec![
                    ("m_VertexCount", i(3)),
                    ("m_Channels", Value::Array(channels.clone())),
                    ("m_DataSize", Value::Bytes(if inline { vd.b.clone() } else { Vec::new() })),
                ]),
            ),
            (
                "m_StreamData",
                st(vec![
                    ("offset", Value::UInt(if inline { 0 } else { 8 })),
                    ("size", Value::UInt(if inline { 0 } else { vd.b.len() as u64 })),
                    ("path", s(if inline { "" } else { "archive:/CAB-x/CAB-x.resS" })),
                ]),
            ),
        ])
    };
    let mut res = vec![0xAA; 8];
    res.extend_from_slice(&vd.b);
    for (inline, m) in [
        (true, Mesh::from_value(&mesh_value(true), &NoResources)),
        (false, Mesh::from_value(&mesh_value(false), &MapRes(res))),
    ] {
        let m = m.unwrap_or_else(|e| panic!("inline={inline}: {e}"));
        assert_eq!(m.streams(), vec![(0, 12), (48, 4)]);
        assert_eq!(m.positions(), vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.5]]);
        assert_eq!(m.uv(0), vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
        assert!(m.normals().is_empty());
        assert_eq!(m.submesh_indices(0).unwrap(), vec![2, 1, 0]);
    }
}

#[test]
fn half_floats() {
    assert_eq!(f16_to_f32(0x3c00), 1.0);
    assert_eq!(f16_to_f32(0xc000), -2.0);
    assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
    assert!(f16_to_f32(0x7c00).is_infinite());
}

#[test]
fn texture_decoding() {
    // RGBA32 2x2, stored bottom row first: output must be flipped.
    let px = [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    let out = texture::decode_rgba8(4, 2, 2, &px).unwrap();
    assert_eq!(&out[..8], &[3, 3, 3, 3, 4, 4, 4, 4]);
    // RGB565 pure green
    let out = texture::decode_rgba8(7, 1, 1, &0x07e0u16.to_le_bytes()).unwrap();
    assert_eq!(out, [0, 255, 0, 255]);
    // BC1 block: colour0 red, colour1 blue, all indices 0 -> red
    let out = texture::decode_rgba8(10, 4, 4, &[0x00, 0xf8, 0x1f, 0x00, 0, 0, 0, 0]).unwrap();
    assert!(out.chunks(4).all(|p| p[0] > 240 && p[1] == 0 && p[2] == 0 && p[3] == 255), "{out:?}");
    assert!(texture::decode_rgba8(10, 8, 8, &[0; 8]).is_err());
    assert!(matches!(texture::decode_rgba8(1000, 1, 1, &[0; 4]), Err(Error::Unsupported(_))));
}

#[test]
fn typed_material_and_transform() {
    let pptr = |p| Value::PPtr(PPtr { file_id: 0, path_id: p });
    let v2 = |x, y| st(vec![("x", Value::Float(x)), ("y", Value::Float(y))]);
    let pair = |k: &str, v| st(vec![("first", s(k)), ("second", v)]);
    let mat = st(vec![
        ("m_Name", s("M")),
        ("m_Shader", pptr(5)),
        ("m_ValidKeywords", Value::Array(vec![s("_EMISSION")])),
        (
            "m_SavedProperties",
            st(vec![
                ("m_TexEnvs", Value::Array(vec![pair("_MainTex", st(vec![("m_Texture", pptr(9)), ("m_Scale", v2(1.0, 2.0)), ("m_Offset", v2(0.0, 0.5))]))])),
                ("m_Ints", Value::Array(vec![])),
                ("m_Floats", Value::Array(vec![pair("_Glossiness", Value::Float(0.25))])),
                ("m_Colors", Value::Array(vec![pair("_Color", st(vec![("r", Value::Float(1.0)), ("g", Value::Float(0.5)), ("b", Value::Float(0.0)), ("a", Value::Float(1.0))]))])),
            ]),
        ),
    ]);
    let m = Material::from_value(&mat).unwrap();
    assert_eq!(m.shader.path_id, 5);
    assert_eq!(m.keywords, ["_EMISSION"]);
    assert_eq!(m.textures[0].name, "_MainTex");
    assert_eq!((m.textures[0].texture.path_id, m.textures[0].scale, m.textures[0].offset), (9, [1.0, 2.0], [0.0, 0.5]));
    assert_eq!(m.floats, vec![("_Glossiness".to_string(), 0.25)]);
    assert_eq!(m.colors, vec![("_Color".to_string(), [1.0, 0.5, 0.0, 1.0])]);

    let v3 = |x, y, z| st(vec![("x", Value::Float(x)), ("y", Value::Float(y)), ("z", Value::Float(z))]);
    let tr = st(vec![
        ("m_GameObject", pptr(1)),
        ("m_LocalRotation", st(vec![("x", Value::Float(0.0)), ("y", Value::Float(0.0)), ("z", Value::Float(0.0)), ("w", Value::Float(1.0))])),
        ("m_LocalPosition", v3(1.0, 2.0, 3.0)),
        ("m_LocalScale", v3(1.0, 1.0, 1.0)),
        ("m_Children", Value::Array(vec![pptr(7), pptr(8)])),
        ("m_Father", pptr(0)),
    ]);
    let t = Transform::from_value(&tr).unwrap();
    assert_eq!((t.local_position, t.local_rotation[3], t.children.len()), ([1.0, 2.0, 3.0], 1.0, 2));
    assert!(t.parent.is_null());
}

#[test]
fn catalog_keys_locations_dynamic_strings() {
    const NONE: u32 = u32::MAX;
    let mut w = W::new(false);
    w.raw(&[0; 32]);
    let blob = |w: &mut W, b: &[u8]| -> u32 {
        w.u32(b.len() as u32);
        let id = w.b.len() as u32;
        w.raw(b);
        id
    };
    let words = |w: &mut W, v: &[u32]| -> u32 {
        let id = w.b.len() as u32;
        for x in v {
            w.u32(*x);
        }
        id
    };
    // "System.String" as a dynamic string: parts stored last first.
    let sys = blob(&mut w, b"System");
    let string = blob(&mut w, b"String");
    let tail = words(&mut w, &[sys, NONE]);
    let class = words(&mut w, &[string, tail]) | 0x4000_0000;
    let asm = blob(&mut w, b"mscorlib");
    let str_type = words(&mut w, &[asm, class]);
    let go_class = blob(&mut w, b"UnityEngine.GameObject");
    let go_type = words(&mut w, &[asm, go_class]);
    let prov = blob(&mut w, b"P.AssetBundleProvider");
    let bundle = blob(&mut w, b"abc.bundle");
    let bundle_path = blob(&mut w, b"{RuntimePath}\\abc.bundle");
    let loc_bundle = words(&mut w, &[bundle, bundle_path, prov, NONE, 0, NONE, NONE]);
    let deps = blob(&mut w, &loc_bundle.to_le_bytes());
    let addr = blob(&mut w, b"Assets/x.prefab");
    let iid = blob(&mut w, b"1f");
    let loc_asset = words(&mut w, &[addr, iid, prov, deps, 0, NONE, go_type]);
    let set_asset = blob(&mut w, &loc_asset.to_le_bytes());
    let set_bundle = blob(&mut w, &loc_bundle.to_le_bytes());
    let key_str = words(&mut w, &[addr, '/' as u32]);
    let key_obj = words(&mut w, &[str_type, key_str]);
    let key2_str = words(&mut w, &[bundle, '/' as u32]);
    let key2_obj = words(&mut w, &[str_type, key2_str]);
    let keys: Vec<u8> = [key_obj, set_asset, key2_obj, set_bundle].iter().flat_map(|x| x.to_le_bytes()).collect();
    let keys_id = blob(&mut w, &keys);
    w.b[..12].copy_from_slice(&[catalog::MAGIC.to_le_bytes(), 2u32.to_le_bytes(), keys_id.to_le_bytes()].concat());

    let cat = Catalog::parse(w.b).unwrap();
    assert_eq!(cat.string(class, '.').unwrap(), "System.String");
    let e = cat.entries().unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].key, "Assets/x.prefab");
    let l = &e[0].locations[0];
    assert_eq!((l.internal_id.as_str(), l.resource_type.as_str()), ("1f", "UnityEngine.GameObject"));
    assert_eq!(l.dependencies, ["abc.bundle"]);
    assert_eq!(catalog::bundle_file(l), Some("abc.bundle"));
    assert_eq!(catalog::bundle_file(&e[1].locations[0]), Some("abc.bundle"));
    assert!(Catalog::parse(vec![0; 32]).is_err());
}
