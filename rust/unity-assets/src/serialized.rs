//! SerializedFile (the `CAB-*` files in bundles): header, type trees, object table, externals.
//! Supports format versions 17..=22+ (Unity 2017.x through Unity 6, which writes 22).

use crate::reader::Reader;
use crate::value::{self, Value};
use crate::{format_err, unsupported, Result};
use std::sync::Arc;

/// Unity's shared type-tree string table, addressed by byte offset with the top bit set.
/// Copied in order from the engine's `CommonString` buffer (Unity 6 adds `RenderingLayerMask`).
pub const COMMON_STRINGS: &[&str] = &[
    "AABB", "AnimationClip", "AnimationCurve", "AnimationState", "Array", "Base", "BitField", "bitset",
    "bool", "char", "ColorRGBA", "Component", "data", "deque", "double", "dynamic_array",
    "FastPropertyName", "first", "float", "Font", "GameObject", "Generic Mono", "GradientNEW", "GUID",
    "GUIStyle", "int", "list", "long long", "map", "Matrix4x4f", "MdFour", "MonoBehaviour", "MonoScript",
    "m_ByteSize", "m_Curve", "m_EditorClassIdentifier", "m_EditorHideFlags", "m_Enabled",
    "m_ExtensionPtr", "m_GameObject", "m_Index", "m_IsArray", "m_IsStatic", "m_MetaFlag", "m_Name",
    "m_ObjectHideFlags", "m_PrefabInternal", "m_PrefabParentObject", "m_Script", "m_StaticEditorFlags",
    "m_Type", "m_Version", "Object", "pair", "PPtr<Component>", "PPtr<GameObject>", "PPtr<Material>",
    "PPtr<MonoBehaviour>", "PPtr<MonoScript>", "PPtr<Object>", "PPtr<Prefab>", "PPtr<Sprite>",
    "PPtr<TextAsset>", "PPtr<Texture>", "PPtr<Texture2D>", "PPtr<Transform>", "Prefab", "Quaternionf",
    "Rectf", "RectInt", "RectOffset", "second", "set", "short", "size", "SInt16", "SInt32", "SInt64",
    "SInt8", "staticvector", "string", "TextAsset", "TextMesh", "Texture", "Texture2D", "Transform",
    "TypelessData", "UInt16", "UInt32", "UInt64", "UInt8", "unsigned int", "unsigned long long",
    "unsigned short", "vector", "Vector2f", "Vector3f", "Vector4f", "m_ScriptingClassIdentifier",
    "Gradient", "Type*", "int2_storage", "int3_storage", "BoundsInt", "m_CorrespondingSourceObject",
    "m_PrefabInstance", "m_PrefabAsset", "FileSize", "Hash128", "RenderingLayerMask",
];

/// Looks up a common string by its byte offset in the concatenated table.
pub fn common_string(offset: u32) -> Option<&'static str> {
    let mut at = 0u32;
    for s in COMMON_STRINGS {
        if at == offset {
            return Some(s);
        }
        at += s.len() as u32 + 1;
    }
    None
}

/// Meta flag: align the stream to 4 bytes after this field.
pub const META_ALIGN: u32 = 0x4000;

#[derive(Debug, Clone)]
pub struct TypeNode {
    pub version: u16,
    pub level: u8,
    /// Bit 0: this node is an `Array`. Bit 1: ref-type.
    pub type_flags: u8,
    pub type_name: Arc<str>,
    pub name: Arc<str>,
    pub byte_size: i32,
    pub index: i32,
    pub meta_flag: u32,
    pub ref_type_hash: u64,
}

impl TypeNode {
    pub fn is_array(&self) -> bool {
        self.type_flags & 1 != 0
    }
    pub fn aligns(&self) -> bool {
        self.meta_flag & META_ALIGN != 0
    }
}

/// Flat pre-order node list; `ends[i]` is one past the last descendant of node `i`.
#[derive(Debug, Clone, Default)]
pub struct TypeTree {
    pub nodes: Vec<TypeNode>,
    pub ends: Vec<usize>,
}

impl TypeTree {
    pub fn new(nodes: Vec<TypeNode>) -> Self {
        let mut ends = vec![nodes.len(); nodes.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            while let Some(&top) = stack.last() {
                if nodes[top].level >= n.level {
                    ends[top] = i;
                    stack.pop();
                } else {
                    break;
                }
            }
            stack.push(i);
        }
        TypeTree { nodes, ends }
    }

    /// Direct children of node `i`.
    pub fn children(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let mut c = i + 1;
        std::iter::from_fn(move || {
            if c >= self.ends[i] {
                return None;
            }
            let at = c;
            c = self.ends[c];
            Some(at)
        })
    }

    /// Indented text dump, one node per line.
    pub fn dump(&self) -> String {
        let mut s = String::new();
        for n in &self.nodes {
            let ind = "  ".repeat(n.level as usize);
            let align = if n.aligns() { " (align)" } else { "" };
            s += &format!("{ind}{} {} size={}{align}\n", n.type_name, n.name, n.byte_size);
        }
        s
    }
}

#[derive(Debug, Clone)]
pub struct SerializedType {
    pub class_id: i32,
    pub is_stripped: bool,
    pub script_type_index: i16,
    pub script_id: Option<[u8; 16]>,
    pub old_type_hash: [u8; 16],
    /// `None` when the file was built without type trees.
    pub tree: Option<TypeTree>,
    pub type_dependencies: Vec<i32>,
    /// For ref types (SerializeReference): class name, namespace, assembly.
    pub ref_class: Option<(String, String, String)>,
}

#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub path_id: i64,
    /// Absolute offset of the object's bytes within the file.
    pub byte_start: u64,
    pub byte_size: u32,
    pub type_index: i32,
    pub class_id: i32,
}

#[derive(Debug, Clone)]
pub struct ScriptType {
    pub file_index: i32,
    pub path_id: i64,
}

/// Another serialized file referenced by `PPtr.file_id` (index + 1).
#[derive(Debug, Clone)]
pub struct External {
    pub guid: [u8; 16],
    pub kind: i32,
    pub path: String,
}

#[derive(Debug)]
pub struct SerializedFile {
    pub version: u32,
    pub big_endian: bool,
    pub file_size: u64,
    pub data_offset: u64,
    pub unity_version: String,
    pub target_platform: i32,
    pub type_tree_enabled: bool,
    pub types: Vec<SerializedType>,
    pub objects: Vec<ObjectInfo>,
    pub script_types: Vec<ScriptType>,
    pub externals: Vec<External>,
    pub ref_types: Vec<SerializedType>,
    pub user_info: String,
    data: Vec<u8>,
}

impl SerializedFile {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data, true);
        let mut metadata_size = r.u32()? as u64;
        let mut file_size = r.u32()? as u64;
        let version = r.u32()?;
        let mut data_offset = r.u32()? as u64;
        if !(17..=30).contains(&version) {
            return unsupported(format!("SerializedFile version {version}"));
        }
        let big_endian = r.u8()? != 0;
        r.bytes(3)?;
        if version >= 22 {
            metadata_size = r.u32()? as u64;
            file_size = r.i64()? as u64;
            data_offset = r.i64()? as u64;
            r.i64()?;
        }
        let _ = metadata_size;
        if data_offset > data.len() as u64 {
            return format_err(format!("data offset {data_offset} past file end {}", data.len()));
        }
        r.big_endian = big_endian;

        let unity_version = r.cstr()?;
        let target_platform = r.i32()?;
        let type_tree_enabled = r.bool()?;
        let n = r.len_prefix(1)?;
        let types = (0..n).map(|_| read_type(&mut r, version, type_tree_enabled, false)).collect::<Result<Vec<_>>>()?;

        let n = r.len_prefix(20)?;
        let mut objects = Vec::with_capacity(n);
        for _ in 0..n {
            r.align(4);
            let path_id = r.i64()?;
            let start = if version >= 22 { r.i64()? as u64 } else { r.u32()? as u64 };
            let byte_size = r.u32()?;
            let type_index = r.i32()?;
            let class_id = match types.get(type_index as usize) {
                Some(t) => t.class_id,
                None => return format_err(format!("object {path_id} has type index {type_index} of {}", types.len())),
            };
            let byte_start = data_offset + start;
            if byte_start + byte_size as u64 > data.len() as u64 {
                return format_err(format!("object {path_id} past file end"));
            }
            objects.push(ObjectInfo { path_id, byte_start, byte_size, type_index, class_id });
        }

        let n = r.len_prefix(12)?;
        let mut script_types = Vec::with_capacity(n);
        for _ in 0..n {
            let file_index = r.i32()?;
            r.align(4);
            script_types.push(ScriptType { file_index, path_id: r.i64()? });
        }

        let n = r.len_prefix(22)?;
        let mut externals = Vec::with_capacity(n);
        for _ in 0..n {
            r.cstr()?; // empty
            let guid = r.array::<16>()?;
            let kind = r.i32()?;
            externals.push(External { guid, kind, path: r.cstr()? });
        }

        let mut ref_types = Vec::new();
        if version >= 20 {
            let n = r.len_prefix(1)?;
            for _ in 0..n {
                ref_types.push(read_type(&mut r, version, type_tree_enabled, true)?);
            }
        }
        let user_info = r.cstr()?;

        Ok(SerializedFile {
            version,
            big_endian,
            file_size,
            data_offset,
            unity_version,
            target_platform,
            type_tree_enabled,
            types,
            objects,
            script_types,
            externals,
            ref_types,
            user_info,
            data,
        })
    }

    pub fn object(&self, path_id: i64) -> Option<&ObjectInfo> {
        self.objects.iter().find(|o| o.path_id == path_id)
    }

    pub fn object_bytes(&self, o: &ObjectInfo) -> &[u8] {
        &self.data[o.byte_start as usize..(o.byte_start + o.byte_size as u64) as usize]
    }

    pub fn type_of(&self, o: &ObjectInfo) -> &SerializedType {
        &self.types[o.type_index as usize]
    }

    /// Type name from the type tree root, or the built-in class name.
    pub fn type_name(&self, o: &ObjectInfo) -> String {
        match &self.type_of(o).tree {
            Some(t) if !t.nodes.is_empty() => t.nodes[0].type_name.to_string(),
            _ => crate::classes::class_name(o.class_id).map_or_else(|| format!("Class{}", o.class_id), str::to_string),
        }
    }

    /// Reads an object into a [`Value`] using its type tree. Fails if trees are stripped or the
    /// tree does not consume exactly the object's bytes.
    pub fn read_value(&self, o: &ObjectInfo) -> Result<Value> {
        let Some(tree) = &self.type_of(o).tree else {
            return unsupported(format!("no type tree for class {}", o.class_id));
        };
        let mut r = Reader::new(self.object_bytes(o), self.big_endian);
        let v = value::read(tree, &self.ref_types, &mut r)?;
        if r.pos() != r.len() {
            return format_err(format!(
                "{} {} read {} of {} bytes",
                self.type_name(o),
                o.path_id,
                r.pos(),
                r.len()
            ));
        }
        Ok(v)
    }

    /// Object name without a full read: most named objects start with `m_Name`
    /// (GameObject has it after the component list, so this reads the whole value for it).
    pub fn object_name(&self, o: &ObjectInfo) -> Option<String> {
        let tree = self.type_of(o).tree.as_ref()?;
        let first = tree.children(0).next()?;
        if &*tree.nodes[first].name == "m_Name" && &*tree.nodes[first].type_name == "string" {
            return Reader::new(self.object_bytes(o), self.big_endian).aligned_string().ok();
        }
        if tree.children(0).any(|c| &*tree.nodes[c].name == "m_Name") {
            return self.read_value(o).ok()?.get("m_Name")?.as_str().map(str::to_string);
        }
        None
    }
}

fn read_type(r: &mut Reader, version: u32, trees: bool, is_ref: bool) -> Result<SerializedType> {
    let class_id = r.i32()?;
    let is_stripped = r.bool()?;
    let script_type_index = r.i16()?;
    let script_id = if (is_ref && script_type_index >= 0) || class_id == 114 { Some(r.array::<16>()?) } else { None };
    let old_type_hash = r.array::<16>()?;
    let mut t = SerializedType {
        class_id,
        is_stripped,
        script_type_index,
        script_id,
        old_type_hash,
        tree: None,
        type_dependencies: Vec::new(),
        ref_class: None,
    };
    if trees {
        t.tree = Some(read_tree(r, version)?);
        if version >= 21 {
            if is_ref {
                t.ref_class = Some((r.cstr()?, r.cstr()?, r.cstr()?));
            } else {
                let n = r.len_prefix(4)?;
                t.type_dependencies = (0..n).map(|_| r.i32()).collect::<Result<_>>()?;
            }
        }
    }
    Ok(t)
}

fn read_tree(r: &mut Reader, version: u32) -> Result<TypeTree> {
    let node_size = if version >= 19 { 32 } else { 24 };
    let count = r.len_prefix(node_size)?;
    let strings_len = r.len_prefix(0)?;
    let raw = r.bytes(count * node_size)?;
    let strings = r.bytes(strings_len)?;
    let name = |off: u32| -> Result<Arc<str>> {
        if off & 0x8000_0000 != 0 {
            return match common_string(off & 0x7fff_ffff) {
                Some(s) => Ok(s.into()),
                None => format_err(format!("unknown common string offset {}", off & 0x7fff_ffff)),
            };
        }
        let s = strings.get(off as usize..).ok_or_else(|| crate::Error::Format(format!("string offset {off}")))?;
        let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        Ok(String::from_utf8_lossy(&s[..end]).into())
    };
    let mut nr = Reader::new(raw, r.big_endian);
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let version = nr.u16()?;
        let level = nr.u8()?;
        let type_flags = nr.u8()?;
        let type_name = name(nr.u32()?)?;
        let field = name(nr.u32()?)?;
        let byte_size = nr.i32()?;
        let index = nr.i32()?;
        let meta_flag = nr.u32()?;
        let ref_type_hash = if node_size == 32 { nr.u64()? } else { 0 };
        nodes.push(TypeNode { version, level, type_flags, type_name, name: field, byte_size, index, meta_flag, ref_type_hash });
    }
    Ok(TypeTree::new(nodes))
}
