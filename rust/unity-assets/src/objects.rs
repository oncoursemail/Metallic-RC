//! Typed views of common objects, built from [`Value`] trees (so they need type trees).

use crate::value::{PPtr, Value};
use crate::{format_err, unsupported, Bundle, Error, Result};

/// Source of out-of-line data referenced by `StreamingInfo` (`.resS` / `.resource` nodes).
pub trait Resources {
    fn resource(&self, path: &str, offset: u64, size: u64) -> Result<Vec<u8>>;
}

impl Resources for Bundle {
    fn resource(&self, path: &str, offset: u64, size: u64) -> Result<Vec<u8>> {
        let node = self.node(path).ok_or_else(|| Error::Format(format!("resource {path:?} not in bundle")))?;
        self.read_node_range(node, offset, size)
    }
}

/// For objects known to have no streamed data.
pub struct NoResources;

impl Resources for NoResources {
    fn resource(&self, path: &str, _: u64, _: u64) -> Result<Vec<u8>> {
        unsupported(format!("no resource source for {path:?}"))
    }
}

fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value> {
    v.path(name).ok_or_else(|| Error::Format(format!("missing field {name}")))
}

fn int(v: &Value, name: &str) -> Result<i64> {
    field(v, name)?.as_i64().ok_or_else(|| Error::Format(format!("{name} is not an integer")))
}

fn pptr(v: &Value, name: &str) -> Result<PPtr> {
    field(v, name)?.as_pptr().ok_or_else(|| Error::Format(format!("{name} is not a PPtr")))
}

fn string(v: &Value, name: &str) -> String {
    v.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn floats<const N: usize>(v: &Value, name: &str) -> Result<[f32; N]> {
    let f = field(v, name)?.floats();
    f.get(..N).and_then(|s| s.try_into().ok()).ok_or_else(|| Error::Format(format!("{name} needs {N} floats")))
}

fn pptrs(v: &Value, name: &str) -> Vec<PPtr> {
    v.get(name).and_then(Value::as_array).unwrap_or_default().iter().filter_map(Value::as_pptr).collect()
}

/// Reads `StreamingInfo` data if the object has any; `None` when the data is inline.
fn streamed(v: &Value, res: &dyn Resources) -> Result<Option<Vec<u8>>> {
    let Some(s) = v.get("m_StreamData") else { return Ok(None) };
    let path = s.get("path").and_then(Value::as_str).unwrap_or("");
    let size = s.get("size").and_then(Value::as_u64).unwrap_or(0);
    if path.is_empty() || size == 0 {
        return Ok(None);
    }
    let offset = s.get("offset").and_then(Value::as_u64).unwrap_or(0);
    res.resource(path, offset, size).map(Some)
}

#[derive(Debug, Clone)]
pub struct GameObject {
    pub name: String,
    pub components: Vec<PPtr>,
    pub layer: u32,
    pub tag: u16,
    pub active: bool,
}

impl GameObject {
    pub fn from_value(v: &Value) -> Result<Self> {
        let comps = field(v, "m_Component")?.as_array().unwrap_or_default();
        Ok(GameObject {
            name: string(v, "m_Name"),
            components: comps.iter().filter_map(|c| c.get("component").and_then(Value::as_pptr)).collect(),
            layer: int(v, "m_Layer")? as u32,
            tag: int(v, "m_Tag").unwrap_or(0) as u16,
            active: v.get("m_IsActive").and_then(Value::as_bool).unwrap_or(true),
        })
    }
}

/// Transform or RectTransform (same leading fields).
#[derive(Debug, Clone)]
pub struct Transform {
    pub game_object: PPtr,
    pub local_position: [f32; 3],
    /// Quaternion x, y, z, w.
    pub local_rotation: [f32; 4],
    pub local_scale: [f32; 3],
    pub parent: PPtr,
    pub children: Vec<PPtr>,
}

impl Transform {
    pub fn from_value(v: &Value) -> Result<Self> {
        Ok(Transform {
            game_object: pptr(v, "m_GameObject")?,
            local_position: floats(v, "m_LocalPosition")?,
            local_rotation: floats(v, "m_LocalRotation")?,
            local_scale: floats(v, "m_LocalScale")?,
            parent: pptr(v, "m_Father")?,
            children: pptrs(v, "m_Children"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MeshFilter {
    pub game_object: PPtr,
    pub mesh: PPtr,
}

impl MeshFilter {
    pub fn from_value(v: &Value) -> Result<Self> {
        Ok(MeshFilter { game_object: pptr(v, "m_GameObject")?, mesh: pptr(v, "m_Mesh")? })
    }
}

/// MeshRenderer, or SkinnedMeshRenderer (which also names its mesh).
#[derive(Debug, Clone)]
pub struct MeshRenderer {
    pub game_object: PPtr,
    pub enabled: bool,
    pub materials: Vec<PPtr>,
    pub mesh: Option<PPtr>,
}

impl MeshRenderer {
    pub fn from_value(v: &Value) -> Result<Self> {
        Ok(MeshRenderer {
            game_object: pptr(v, "m_GameObject")?,
            enabled: v.get("m_Enabled").and_then(Value::as_bool).unwrap_or(true),
            materials: pptrs(v, "m_Materials"),
            mesh: v.get("m_Mesh").and_then(Value::as_pptr),
        })
    }
}

/// Vertex attribute slots in `VertexData.m_Channels` (Unity 2019+).
pub mod channel {
    pub const POSITION: usize = 0;
    pub const NORMAL: usize = 1;
    pub const TANGENT: usize = 2;
    pub const COLOR: usize = 3;
    pub const UV0: usize = 4;
    pub const UV1: usize = 5;
    pub const BLEND_WEIGHT: usize = 12;
    pub const BLEND_INDICES: usize = 13;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VertexChannel {
    pub stream: u8,
    pub offset: u8,
    /// Unity `VertexFormat`: 0 Float, 1 Float16, 2 UNorm8, 3 SNorm8, 4 UNorm16, 5 SNorm16,
    /// 6 UInt8, 7 SInt8, 8 UInt16, 9 SInt16, 10 UInt32, 11 SInt32.
    pub format: u8,
    /// Component count (low 4 bits of Unity's field); 0 = channel unused.
    pub dimension: u8,
}

fn format_size(f: u8) -> Option<usize> {
    Some(match f {
        2 | 3 | 6 | 7 => 1,
        1 | 4 | 5 | 8 | 9 => 2,
        0 | 10 | 11 => 4,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct SubMesh {
    pub first_byte: u32,
    pub index_count: u32,
    /// 0 triangles, 2 quads, 3 lines, 4 line strip, 5 points.
    pub topology: i32,
    pub base_vertex: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub vertex_count: usize,
    pub channels: Vec<VertexChannel>,
    /// All vertex streams, inline or from the `.resS` resource.
    pub vertex_data: Vec<u8>,
    /// 0 = 16-bit, 1 = 32-bit indices.
    pub index_format: i32,
    pub index_buffer: Vec<u8>,
    pub submeshes: Vec<SubMesh>,
}

impl Mesh {
    pub fn from_value(v: &Value, res: &dyn Resources) -> Result<Self> {
        if v.get("m_MeshCompression").and_then(Value::as_i64).unwrap_or(0) != 0 {
            return unsupported("compressed mesh (m_MeshCompression != 0)");
        }
        let vd = field(v, "m_VertexData")?;
        let channels = field(vd, "m_Channels")?
            .as_array()
            .unwrap_or_default()
            .iter()
            .map(|c| -> Result<_> {
                Ok(VertexChannel {
                    stream: int(c, "stream")? as u8,
                    offset: int(c, "offset")? as u8,
                    format: int(c, "format")? as u8,
                    dimension: (int(c, "dimension")? & 0xf) as u8,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let vertex_data = match streamed(v, res)? {
            Some(d) => d,
            None => field(vd, "m_DataSize")?.as_bytes().unwrap_or_default().to_vec(),
        };
        let submeshes = field(v, "m_SubMeshes")?
            .as_array()
            .unwrap_or_default()
            .iter()
            .map(|s| -> Result<_> {
                Ok(SubMesh {
                    first_byte: int(s, "firstByte")? as u32,
                    index_count: int(s, "indexCount")? as u32,
                    topology: int(s, "topology")? as i32,
                    base_vertex: int(s, "baseVertex").unwrap_or(0) as u32,
                    first_vertex: int(s, "firstVertex")? as u32,
                    vertex_count: int(s, "vertexCount")? as u32,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mesh = Mesh {
            name: string(v, "m_Name"),
            vertex_count: int(vd, "m_VertexCount")? as usize,
            channels,
            vertex_data,
            index_format: v.get("m_IndexFormat").and_then(Value::as_i64).unwrap_or(0) as i32,
            index_buffer: field(v, "m_IndexBuffer")?.as_bytes().unwrap_or_default().to_vec(),
            submeshes,
        };
        let need = mesh.streams().last().map_or(0, |s| s.0 + s.1 * mesh.vertex_count);
        if mesh.vertex_data.len() < need {
            return format_err(format!("vertex data {} bytes, layout needs {need}", mesh.vertex_data.len()));
        }
        Ok(mesh)
    }

    /// `(offset, stride)` of each vertex stream; streams follow each other, 16-byte aligned.
    pub fn streams(&self) -> Vec<(usize, usize)> {
        let count = self.channels.iter().filter(|c| c.dimension > 0).map(|c| c.stream as usize + 1).max().unwrap_or(0);
        let mut out = Vec::with_capacity(count);
        let mut offset = 0;
        for s in 0..count {
            let stride: usize = self
                .channels
                .iter()
                .filter(|c| c.stream as usize == s && c.dimension > 0)
                .map(|c| c.dimension as usize * format_size(c.format).unwrap_or(0))
                .sum();
            out.push((offset, stride));
            offset = (offset + stride * self.vertex_count).div_ceil(16) * 16;
        }
        out
    }

    /// One channel as floats (normalised formats mapped to 0..1 / -1..1); returns `(dimension, values)`.
    pub fn channel(&self, index: usize) -> Option<(usize, Vec<f32>)> {
        let c = *self.channels.get(index)?;
        if c.dimension == 0 {
            return None;
        }
        let size = format_size(c.format)?;
        let (base, stride) = *self.streams().get(c.stream as usize)?;
        let dim = c.dimension as usize;
        let mut out = Vec::with_capacity(self.vertex_count * dim);
        for v in 0..self.vertex_count {
            for k in 0..dim {
                let at = base + v * stride + c.offset as usize + k * size;
                let b = self.vertex_data.get(at..at + size)?;
                out.push(match c.format {
                    0 => f32::from_le_bytes(b.try_into().ok()?),
                    1 => f16_to_f32(u16::from_le_bytes([b[0], b[1]])),
                    2 => b[0] as f32 / 255.0,
                    3 => (b[0] as i8 as f32 / 127.0).max(-1.0),
                    4 => u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0,
                    5 => (i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0).max(-1.0),
                    6 => b[0] as f32,
                    7 => b[0] as i8 as f32,
                    8 => u16::from_le_bytes([b[0], b[1]]) as f32,
                    9 => i16::from_le_bytes([b[0], b[1]]) as f32,
                    10 => u32::from_le_bytes(b.try_into().ok()?) as f32,
                    _ => i32::from_le_bytes(b.try_into().ok()?) as f32,
                });
            }
        }
        Some((dim, out))
    }

    fn fixed<const N: usize>(&self, index: usize) -> Vec<[f32; N]> {
        let Some((dim, v)) = self.channel(index) else { return Vec::new() };
        v.chunks_exact(dim)
            .map(|c| std::array::from_fn(|k| c.get(k).copied().unwrap_or(if k == 3 { 1.0 } else { 0.0 })))
            .collect()
    }

    pub fn positions(&self) -> Vec<[f32; 3]> {
        self.fixed(channel::POSITION)
    }
    pub fn normals(&self) -> Vec<[f32; 3]> {
        self.fixed(channel::NORMAL)
    }
    pub fn tangents(&self) -> Vec<[f32; 4]> {
        self.fixed(channel::TANGENT)
    }
    pub fn colors(&self) -> Vec<[f32; 4]> {
        self.fixed(channel::COLOR)
    }
    /// Texture coordinate set `n` (0..8).
    pub fn uv(&self, n: usize) -> Vec<[f32; 2]> {
        self.fixed(channel::UV0 + n)
    }

    /// The whole index buffer as u32.
    pub fn indices(&self) -> Vec<u32> {
        if self.index_format == 1 {
            self.index_buffer.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
        } else {
            self.index_buffer.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as u32).collect()
        }
    }

    /// Indices of submesh `i` with its base vertex added.
    pub fn submesh_indices(&self, i: usize) -> Option<Vec<u32>> {
        let s = self.submeshes.get(i)?;
        let size = if self.index_format == 1 { 4 } else { 2 };
        let start = s.first_byte as usize / size;
        let all = self.indices();
        let idx = all.get(start..start + s.index_count as usize)?;
        Some(idx.iter().map(|&x| x + s.base_vertex).collect())
    }
}

/// IEEE half to f32.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let man = (h & 0x3ff) as u32;
    let bits = match exp {
        0 if man == 0 => sign,
        0 => {
            // subnormal: normalise
            let mut e = 127 - 15 + 1;
            let mut m = man;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((m & 0x3ff) << 13)
        }
        31 => sign | 0x7f80_0000 | (man << 13),
        _ => sign | ((exp + 127 - 15) << 23) | (man << 13),
    };
    f32::from_bits(bits)
}

#[derive(Debug, Clone)]
pub struct Texture2D {
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// Unity `TextureFormat`, see [`crate::texture::format_name`].
    pub format: i32,
    pub mip_count: i32,
    pub image_count: i32,
    /// All mips (and images) as stored.
    pub data: Vec<u8>,
}

impl Texture2D {
    pub fn from_value(v: &Value, res: &dyn Resources) -> Result<Self> {
        let data = match streamed(v, res)? {
            Some(d) => d,
            None => v.get("image data").and_then(Value::as_bytes).unwrap_or_default().to_vec(),
        };
        Ok(Texture2D {
            name: string(v, "m_Name"),
            width: int(v, "m_Width")? as usize,
            height: int(v, "m_Height")? as usize,
            format: int(v, "m_TextureFormat")? as i32,
            mip_count: v.get("m_MipCount").and_then(Value::as_i64).unwrap_or(1) as i32,
            image_count: v.get("m_ImageCount").and_then(Value::as_i64).unwrap_or(1) as i32,
            data,
        })
    }

    /// Mip 0 of the first image as RGBA8, top row first.
    pub fn decode_rgba8(&self) -> Result<Vec<u8>> {
        crate::texture::decode_rgba8(self.format, self.width, self.height, &self.data)
    }
}

#[derive(Debug, Clone)]
pub struct TexEnv {
    pub name: String,
    pub texture: PPtr,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub shader: PPtr,
    pub keywords: Vec<String>,
    pub textures: Vec<TexEnv>,
    pub floats: Vec<(String, f32)>,
    pub ints: Vec<(String, i64)>,
    pub colors: Vec<(String, [f32; 4])>,
}

impl Material {
    pub fn from_value(v: &Value) -> Result<Self> {
        let props = field(v, "m_SavedProperties")?;
        let key = |k: &Value| k.as_str().unwrap_or_default().to_string();
        let list = |name: &str| props.get(name).map(Value::pairs).unwrap_or_default();
        let keywords = match v.get("m_ValidKeywords").and_then(Value::as_array) {
            Some(a) => a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect(),
            None => string(v, "m_ShaderKeywords").split_whitespace().map(str::to_string).collect(),
        };
        Ok(Material {
            name: string(v, "m_Name"),
            shader: pptr(v, "m_Shader")?,
            keywords,
            textures: list("m_TexEnvs")
                .into_iter()
                .map(|(k, t)| -> Result<_> {
                    Ok(TexEnv { name: key(k), texture: pptr(t, "m_Texture")?, scale: floats(t, "m_Scale")?, offset: floats(t, "m_Offset")? })
                })
                .collect::<Result<_>>()?,
            floats: list("m_Floats").into_iter().map(|(k, f)| (key(k), f.as_f32().unwrap_or(0.0))).collect(),
            ints: list("m_Ints").into_iter().map(|(k, i)| (key(k), i.as_i64().unwrap_or(0))).collect(),
            colors: list("m_Colors")
                .into_iter()
                .map(|(k, c)| {
                    let f = c.floats();
                    (key(k), std::array::from_fn(|i| f.get(i).copied().unwrap_or(0.0)))
                })
                .collect(),
        })
    }
}

/// The `AssetBundle` object (class 142) every bundle carries: its address table and, for scene
/// bundles, the scene paths.
#[derive(Debug, Clone)]
pub struct AssetBundleInfo {
    pub name: String,
    /// Asset path -> object. For scene bundles the paths are the `.unity` scenes.
    pub container: Vec<(String, PPtr)>,
    pub is_scene: bool,
    pub dependencies: Vec<String>,
}

impl AssetBundleInfo {
    pub fn from_value(v: &Value) -> Result<Self> {
        let container = field(v, "m_Container")?
            .pairs()
            .into_iter()
            .map(|(k, a)| (k.as_str().unwrap_or_default().to_string(), a.get("asset").and_then(Value::as_pptr).unwrap_or_default()))
            .collect();
        Ok(AssetBundleInfo {
            name: string(v, "m_Name"),
            container,
            is_scene: v.get("m_IsStreamedSceneAssetBundle").and_then(Value::as_bool).unwrap_or(false),
            dependencies: v
                .get("m_Dependencies")
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect(),
        })
    }
}
