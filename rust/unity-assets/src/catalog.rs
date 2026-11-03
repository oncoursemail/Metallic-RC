//! Addressables binary content catalog (`catalog.bin`, format version 2): keys (addresses, GUIDs,
//! bundle names) -> resource locations (internal id, provider, type, dependency bundles).
//!
//! The file is one flat buffer; objects are addressed by byte offset. Arrays and strings carry
//! their byte length in the 4 bytes before the offset. String ids with bit 30 set are "dynamic":
//! a linked list of `(part id, next node)` nodes, stored last part first, joined by a separator.
//! Bit 31 marks UTF-16 strings.

use crate::reader::Reader;
use crate::{format_err, unsupported, Result};
use std::collections::HashMap;
use std::path::Path;

pub const MAGIC: u32 = 0x0de3_8942;
const NONE: u32 = u32::MAX;
const DYNAMIC: u32 = 0x4000_0000;
const UNICODE: u32 = 0x8000_0000;

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogLocation {
    pub primary_key: String,
    /// Load path: a bundle path for bundles, or the asset's id inside its bundle
    /// (the bundle's `m_Container` key or the asset path).
    pub internal_id: String,
    pub provider: String,
    /// Full .NET type name of the loaded resource, e.g. `UnityEngine.GameObject`.
    pub resource_type: String,
    /// Primary keys of the locations this one depends on (for assets: the bundles, own bundle first).
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub key: String,
    pub locations: Vec<CatalogLocation>,
}

pub struct Catalog {
    data: Vec<u8>,
    pub version: u32,
    keys_offset: u32,
}

impl Catalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::parse(std::fs::read(path)?)
    }

    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data, false);
        let magic = r.u32()?;
        if magic != MAGIC {
            return format_err(format!("catalog magic {magic:#x}"));
        }
        let version = r.u32()?;
        if version != 2 {
            return unsupported(format!("catalog version {version}"));
        }
        let keys_offset = r.u32()?;
        Ok(Catalog { data, version, keys_offset })
    }

    fn u32_at(&self, at: u32) -> Result<u32> {
        let mut r = Reader::new(&self.data, false);
        r.set_pos(at as usize);
        r.u32()
    }

    /// Length-prefixed byte array at `id`.
    fn blob(&self, id: u32) -> Result<&[u8]> {
        let len = self.u32_at(id.wrapping_sub(4))? as usize;
        self.data.get(id as usize..id as usize + len).ok_or(crate::Error::Format(format!("blob {id:#x} past end")))
    }

    fn u32_array(&self, id: u32) -> Result<Vec<u32>> {
        if id == NONE {
            return Ok(Vec::new());
        }
        Ok(self.blob(id)?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    /// Reads a (possibly dynamic) string; `sep` joins dynamic parts.
    pub fn string(&self, id: u32, sep: char) -> Result<String> {
        if id == NONE {
            return Ok(String::new());
        }
        if id & DYNAMIC != 0 {
            let mut parts = Vec::new();
            let mut node = id & !(DYNAMIC | UNICODE);
            while node != NONE {
                if parts.len() > 4096 {
                    return format_err("dynamic string loop");
                }
                parts.push(self.string(self.u32_at(node)?, sep)?);
                node = self.u32_at(node + 4)?;
            }
            parts.reverse();
            return Ok(parts.join(&sep.to_string()));
        }
        if id & UNICODE != 0 {
            let b = self.blob(id & !UNICODE)?;
            let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            return Ok(String::from_utf16_lossy(&w));
        }
        Ok(String::from_utf8_lossy(self.blob(id)?).into_owned())
    }

    /// A serialized `System.Type`: (assembly, class) dynamic strings.
    fn type_name(&self, id: u32) -> Result<String> {
        if id == NONE {
            return Ok(String::new());
        }
        self.string(self.u32_at(id + 4)?, '.')
    }

    /// A typed key object: `(type id, object id)`. Strings store `(string id, separator)`.
    fn key(&self, id: u32) -> Result<String> {
        let ty = self.type_name(self.u32_at(id)?)?;
        let obj = self.u32_at(id + 4)?;
        Ok(match ty.as_str() {
            "System.String" => {
                let sep = char::from_u32(self.u32_at(obj + 4)?).unwrap_or('/');
                self.string(self.u32_at(obj)?, sep)?
            }
            "System.Int32" => (self.u32_at(obj)? as i32).to_string(),
            _ => format!("<{ty}@{obj:#x}>"),
        })
    }

    fn location(&self, id: u32, deps: bool, cache: &mut HashMap<u32, String>) -> Result<CatalogLocation> {
        let f: Vec<u32> = (0..7).map(|i| self.u32_at(id + 4 * i)).collect::<Result<_>>()?;
        let mut dependencies = Vec::new();
        if deps {
            for d in self.u32_array(f[3])? {
                if let Some(k) = cache.get(&d) {
                    dependencies.push(k.clone());
                    continue;
                }
                let k = self.string(self.u32_at(d)?, '/')?;
                cache.insert(d, k.clone());
                dependencies.push(k);
            }
        }
        Ok(CatalogLocation {
            primary_key: self.string(f[0], '/')?,
            internal_id: self.string(f[1], '/')?,
            provider: self.string(f[2], '.')?,
            resource_type: self.type_name(f[6])?,
            dependencies,
        })
    }

    /// Every key with its locations.
    pub fn entries(&self) -> Result<Vec<CatalogEntry>> {
        let keys = self.u32_array(self.keys_offset)?;
        let mut cache = HashMap::new();
        keys.chunks_exact(2)
            .map(|k| {
                let locations = self
                    .u32_array(k[1])?
                    .into_iter()
                    .map(|l| self.location(l, true, &mut cache))
                    .collect::<Result<_>>()?;
                Ok(CatalogEntry { key: self.key(k[0])?, locations })
            })
            .collect()
    }
}

/// File name of the bundle a location loads from (`.../StandaloneWindows64\x.bundle` -> `x.bundle`).
pub fn bundle_file(loc: &CatalogLocation) -> Option<&str> {
    let p = loc.dependencies.first().unwrap_or(&loc.internal_id);
    p.ends_with(".bundle").then(|| p.rsplit(['/', '\\']).next().unwrap_or(p))
}
