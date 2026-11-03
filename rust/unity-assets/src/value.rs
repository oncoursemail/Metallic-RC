//! Generic object reader: walks a type tree and turns an object's bytes into a [`Value`] tree.

use crate::reader::Reader;
use crate::serialized::{SerializedType, TypeTree};
use crate::{format_err, Result};
use std::sync::Arc;

/// Reference to an object: `file_id` 0 is the same file, n > 0 is `externals[n - 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PPtr {
    pub file_id: i32,
    pub path_id: i64,
}

impl PPtr {
    pub fn is_null(&self) -> bool {
        self.path_id == 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    /// Arrays of 1-byte elements and `TypelessData`.
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    PPtr(PPtr),
    Struct { type_name: Arc<str>, fields: Vec<(Arc<str>, Value)> },
}

impl Value {
    /// Field of a struct by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Struct { fields, .. } => fields.iter().find(|(n, _)| &**n == name).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Follows a `/`-separated field path, e.g. `m_VertexData/m_VertexCount`.
    pub fn path(&self, path: &str) -> Option<&Value> {
        path.split('/').try_fold(self, |v, p| v.get(p))
    }

    pub fn type_name(&self) -> Option<&str> {
        match self {
            Value::Struct { type_name, .. } => Some(type_name),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Value::Int(i) => Some(i),
            Value::UInt(u) => Some(u as i64),
            Value::Bool(b) => Some(b as i64),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().map(|i| i as u64)
    }

    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Value::Float(f) => Some(f as f32),
            Value::Int(i) => Some(i as f32),
            Value::UInt(u) => Some(u as f32),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        self.as_i64().map(|i| i != 0)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            Value::String(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_pptr(&self) -> Option<PPtr> {
        match self {
            Value::PPtr(p) => Some(*p),
            _ => None,
        }
    }

    /// Key/value pairs of a `map`, or of a `vector<pair<..>>` (how Unity 6 stores material properties).
    pub fn pairs(&self) -> Vec<(&Value, &Value)> {
        match self {
            Value::Map(m) => m.iter().map(|(k, v)| (k, v)).collect(),
            Value::Array(a) => a.iter().filter_map(|p| Some((p.get("first")?, p.get("second")?))).collect(),
            _ => Vec::new(),
        }
    }

    /// `[x, y, z, w]` style struct fields as floats, in order.
    pub fn floats(&self) -> Vec<f32> {
        match self {
            Value::Struct { fields, .. } => fields.iter().filter_map(|(_, v)| v.as_f32()).collect(),
            Value::Array(a) => a.iter().filter_map(Value::as_f32).collect(),
            _ => Vec::new(),
        }
    }
}

/// Reads one object whose layout is `tree`. `ref_types` resolves SerializeReference data.
pub fn read(tree: &TypeTree, ref_types: &[SerializedType], r: &mut Reader) -> Result<Value> {
    if tree.nodes.is_empty() {
        return format_err("empty type tree");
    }
    Ctx { tree, ref_types, nested: false }.node(0, r)
}

struct Ctx<'a> {
    tree: &'a TypeTree,
    ref_types: &'a [SerializedType],
    /// Inside SerializeReference data, where the type's own trailing `references` registry is not stored.
    nested: bool,
}

impl Ctx<'_> {
    fn node(&self, i: usize, r: &mut Reader) -> Result<Value> {
        let t = self.tree;
        let n = &t.nodes[i];
        let mut align = n.aligns();
        let v = match &*n.type_name {
            "SInt8" => Value::Int(r.i8()? as i64),
            "UInt8" | "char" => Value::UInt(r.u8()? as u64),
            "SInt16" | "short" => Value::Int(r.i16()? as i64),
            "UInt16" | "unsigned short" => Value::UInt(r.u16()? as u64),
            "SInt32" | "int" => Value::Int(r.i32()? as i64),
            "UInt32" | "unsigned int" | "Type*" => Value::UInt(r.u32()? as u64),
            "SInt64" | "long long" => Value::Int(r.i64()?),
            "UInt64" | "unsigned long long" | "FileSize" => Value::UInt(r.u64()?),
            "float" => Value::Float(r.f32()? as f64),
            "double" => Value::Float(r.f64()?),
            "bool" => Value::Bool(r.bool()?),
            "string" => {
                let len = r.len_prefix(1)?;
                let s = String::from_utf8_lossy(r.bytes(len)?).into_owned();
                if let Some(arr) = t.children(i).next() {
                    align |= t.nodes[arr].aligns();
                }
                Value::String(s)
            }
            "TypelessData" => {
                let len = r.len_prefix(1)?;
                Value::Bytes(r.bytes(len)?.to_vec())
            }
            "map" => {
                let arr = t.children(i).next().ok_or_else(|| crate::Error::Format("map without Array".into()))?;
                align |= t.nodes[arr].aligns();
                let pair = t.children(arr).nth(1).ok_or_else(|| crate::Error::Format("map without pair".into()))?;
                let mut kv = t.children(pair);
                let (k, v) = (kv.next(), kv.next());
                let (Some(k), Some(v)) = (k, v) else { return format_err("map pair without first/second") };
                let count = r.len_prefix(1)?;
                let mut m = Vec::with_capacity(count.min(1 << 16));
                for _ in 0..count {
                    m.push((self.node(k, r)?, self.node(v, r)?));
                }
                Value::Map(m)
            }
            "ReferencedObject" => self.referenced_object(i, r)?,
            _ if n.is_array() => self.array(i, r)?,
            _ => match t.children(i).next() {
                Some(c) if t.nodes[c].is_array() => {
                    align |= t.nodes[c].aligns();
                    self.array(c, r)?
                }
                _ => self.structure(i, r)?,
            },
        };
        if align {
            r.align(4);
        }
        Ok(v)
    }

    fn structure(&self, i: usize, r: &mut Reader) -> Result<Value> {
        let t = self.tree;
        let mut fields = Vec::new();
        for c in t.children(i) {
            if self.nested && i == 0 && &*t.nodes[c].type_name == "ManagedReferencesRegistry" {
                continue;
            }
            fields.push((t.nodes[c].name.clone(), self.node(c, r)?));
        }
        let type_name = t.nodes[i].type_name.clone();
        if type_name.starts_with("PPtr<") {
            if let (Some(f), Some(p)) = (fields.first(), fields.get(1)) {
                if let (Some(file_id), Some(path_id)) = (f.1.as_i64(), p.1.as_i64()) {
                    return Ok(Value::PPtr(PPtr { file_id: file_id as i32, path_id }));
                }
            }
        }
        Ok(Value::Struct { type_name, fields })
    }

    /// `i` is the `Array` node: children are `size` and the element template `data`.
    fn array(&self, i: usize, r: &mut Reader) -> Result<Value> {
        let t = self.tree;
        let elem = t.children(i).nth(1).ok_or_else(|| crate::Error::Format("Array without data node".into()))?;
        let e = &t.nodes[elem];
        let leaf = t.ends[elem] == elem + 1;
        if leaf && matches!(&*e.type_name, "UInt8" | "SInt8" | "char" | "bool") {
            let count = r.len_prefix(1)?;
            return Ok(Value::Bytes(r.bytes(count)?.to_vec()));
        }
        let min = if leaf { e.byte_size.max(1) as usize } else { 0 };
        let count = r.len_prefix(min)?;
        let mut a = Vec::with_capacity(count.min(1 << 16));
        for _ in 0..count {
            a.push(self.node(elem, r)?);
        }
        Ok(Value::Array(a))
    }

    /// SerializeReference entry: `rid`, `type` (class, ns, asm), then `data` laid out by the
    /// matching ref type's own tree.
    fn referenced_object(&self, i: usize, r: &mut Reader) -> Result<Value> {
        let t = self.tree;
        let mut fields = Vec::new();
        let mut class: Option<(String, String, String)> = None;
        for c in t.children(i) {
            let name = t.nodes[c].name.clone();
            let v = if &*t.nodes[c].type_name == "ReferencedObjectData" {
                match &class {
                    Some((cls, ns, asm)) if !cls.is_empty() => {
                        let found = self.ref_types.iter().find(|rt| {
                            rt.ref_class.as_ref().is_some_and(|(c2, n2, a2)| c2 == cls && n2 == ns && a2 == asm)
                        });
                        match found.and_then(|rt| rt.tree.as_ref()) {
                            Some(tree) => Ctx { tree, ref_types: self.ref_types, nested: true }.node(0, r)?,
                            None => return format_err(format!("no ref type for {ns}.{cls} ({asm})")),
                        }
                    }
                    _ => Value::Null,
                }
            } else {
                self.node(c, r)?
            };
            if &*name == "type" {
                let s = |k| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                class = Some((s("class"), s("ns"), s("asm")));
            }
            fields.push((name, v));
        }
        Ok(Value::Struct { type_name: t.nodes[i].type_name.clone(), fields })
    }
}
