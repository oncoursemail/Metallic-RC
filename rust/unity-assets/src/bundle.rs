//! UnityFS asset bundle container: header, block table, directory, block decompression.
//!
//! Blocks are decompressed on demand per read, so a 300 MB bundle costs only its file size
//! in memory until nodes are read.

use crate::reader::Reader;
use crate::{format_err, unsupported, Error, Result};
use std::path::Path;

/// Archive flags (Unity `ArchiveFlags`).
pub const FLAG_COMPRESSION_MASK: u32 = 0x3f;
pub const FLAG_BLOCKS_AND_DIRECTORY_COMBINED: u32 = 0x40;
pub const FLAG_BLOCKS_INFO_AT_END: u32 = 0x80;
pub const FLAG_OLD_WEB_PLUGIN: u32 = 0x100;
pub const FLAG_BLOCK_INFO_PADDING: u32 = 0x200;
/// Every flag bit this reader understands; any other bit (e.g. 0x400, used by UnityCN
/// encryption) makes [`Bundle::from_bytes`] refuse the file.
pub const FLAGS_KNOWN: u32 = 0x3ff;

/// Node flag: the node is a SerializedFile (as opposed to a `.resS` / `.resource` blob).
pub const NODE_SERIALIZED_FILE: u32 = 4;

#[derive(Debug, Clone)]
pub struct BundleHeader {
    pub signature: String,
    pub format: u32,
    pub unity_version: String,
    pub unity_revision: String,
    pub size: i64,
    pub blocks_info_compressed: u32,
    pub blocks_info_size: u32,
    pub flags: u32,
}

/// A file inside the bundle.
#[derive(Debug, Clone)]
pub struct Node {
    pub offset: u64,
    pub size: u64,
    pub flags: u32,
    pub path: String,
}

impl Node {
    pub fn is_serialized_file(&self) -> bool {
        self.flags & NODE_SERIALIZED_FILE != 0
    }
}

#[derive(Debug, Clone)]
struct Block {
    size: u32,
    compressed: u32,
    flags: u16,
    /// Offset of the compressed bytes in the file.
    file_offset: usize,
    /// Offset of the decompressed bytes in the bundle's data stream.
    data_offset: u64,
}

pub struct Bundle {
    pub header: BundleHeader,
    pub nodes: Vec<Node>,
    blocks: Vec<Block>,
    file: Vec<u8>,
}

impl Bundle {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_bytes(std::fs::read(path)?)
    }

    pub fn from_bytes(file: Vec<u8>) -> Result<Self> {
        Self::parse(file, false)
    }

    /// Only the header and directory, read from the start of the file: enough to index many bundles by their
    /// node names without reading them whole. Reading data from it fails.
    pub fn open_head(path: impl AsRef<Path>) -> Result<Self> {
        use std::io::Read;
        let mut f = std::fs::File::open(path)?;
        let len = f.metadata()?.len() as usize;
        let mut file = Vec::new();
        let mut want = 4096.min(len);
        loop {
            (&mut f).take((want - file.len()) as u64).read_to_end(&mut file)?;
            match Self::parse(file.clone(), true) {
                Err(_) if want < len => want = (want * 4).min(len),
                done => return done,
            }
        }
    }

    fn parse(file: Vec<u8>, head_only: bool) -> Result<Self> {
        let mut r = Reader::new(&file, true);
        let signature = r.cstr()?;
        if signature != "UnityFS" {
            return unsupported(format!("bundle signature {signature:?}"));
        }
        let format = r.u32()?;
        if format < 6 {
            return unsupported(format!("UnityFS format {format}"));
        }
        let header = BundleHeader {
            signature,
            format,
            unity_version: r.cstr()?,
            unity_revision: r.cstr()?,
            size: r.i64()?,
            blocks_info_compressed: r.u32()?,
            blocks_info_size: r.u32()?,
            flags: r.u32()?,
        };
        let flags = header.flags;
        if flags & !FLAGS_KNOWN != 0 {
            return unsupported(format!("bundle flags {flags:#x} (unknown bits, possibly encrypted)"));
        }
        if format >= 7 {
            r.align(16);
        }
        let csize = header.blocks_info_compressed as usize;
        let info_raw = if flags & FLAG_BLOCKS_INFO_AT_END != 0 {
            let start = file.len().checked_sub(csize).ok_or_else(|| Error::Format("blocks info past start".into()))?;
            &file[start..]
        } else {
            r.bytes(csize)?
        };
        if flags & FLAG_BLOCK_INFO_PADDING != 0 {
            r.align(16);
        }
        let mut data_pos = r.pos();
        let info = decompress(flags & FLAG_COMPRESSION_MASK, info_raw, header.blocks_info_size as usize)?;

        let mut r = Reader::new(&info, true);
        r.bytes(16)?; // hash of the uncompressed data, unused
        let block_count = r.len_prefix(10)?;
        let mut blocks = Vec::with_capacity(block_count);
        let mut data_offset = 0u64;
        for _ in 0..block_count {
            let size = r.u32()?;
            let compressed = r.u32()?;
            let bflags = r.u16()?;
            blocks.push(Block { size, compressed, flags: bflags, file_offset: data_pos, data_offset });
            data_pos += compressed as usize;
            data_offset += size as u64;
        }
        if data_pos > file.len() && !head_only {
            return format_err(format!("blocks end at {data_pos}, file is {}", file.len()));
        }
        let node_count = r.len_prefix(21)?;
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let offset = r.i64()?;
            let size = r.i64()?;
            let nflags = r.u32()?;
            let path = r.cstr()?;
            if offset < 0 || size < 0 || (offset + size) as u64 > data_offset {
                return format_err(format!("node {path:?} out of range"));
            }
            nodes.push(Node { offset: offset as u64, size: size as u64, flags: nflags, path });
        }
        Ok(Bundle { header, nodes, blocks, file })
    }

    /// Total size of the decompressed data stream.
    pub fn data_size(&self) -> u64 {
        self.blocks.last().map_or(0, |b| b.data_offset + b.size as u64)
    }

    /// Node whose path or file name matches `name` (`archive:/CAB-x/CAB-x.resS` also matches).
    pub fn node(&self, name: &str) -> Option<&Node> {
        let base = name.rsplit('/').next().unwrap_or(name);
        self.nodes.iter().find(|n| n.path == name || n.path == base)
    }

    pub fn read_node(&self, node: &Node) -> Result<Vec<u8>> {
        self.read_data(node.offset, node.size)
    }

    /// Reads `size` bytes at `offset` within `node`.
    pub fn read_node_range(&self, node: &Node, offset: u64, size: u64) -> Result<Vec<u8>> {
        if offset.checked_add(size).is_none_or(|end| end > node.size) {
            return format_err(format!("range {offset}+{size} outside node {:?} ({})", node.path, node.size));
        }
        self.read_data(node.offset + offset, size)
    }

    /// Reads a range of the decompressed data stream, decompressing only the blocks it touches.
    pub fn read_data(&self, offset: u64, size: u64) -> Result<Vec<u8>> {
        let end = offset + size;
        if end > self.data_size() {
            return format_err(format!("range {offset}+{size} past data end {}", self.data_size()));
        }
        let mut out = Vec::with_capacity(size as usize);
        let first = self.blocks.partition_point(|b| b.data_offset + b.size as u64 <= offset);
        for b in &self.blocks[first..] {
            if b.data_offset >= end || out.len() as u64 == size {
                break;
            }
            let raw = self.file.get(b.file_offset..b.file_offset + b.compressed as usize)
                .ok_or_else(|| Error::Format("block outside the bytes read (bundle opened head only)".into()))?;
            let kind = (b.flags & 0x3f) as u32;
            let tmp;
            let bytes: &[u8] = if kind == 0 {
                raw
            } else {
                tmp = decompress(kind, raw, b.size as usize)?;
                &tmp
            };
            let lo = offset.saturating_sub(b.data_offset) as usize;
            let hi = ((end - b.data_offset) as usize).min(bytes.len());
            out.extend_from_slice(&bytes[lo..hi]);
        }
        Ok(out)
    }

    /// Parses every SerializedFile node, as `(node path, file)`.
    pub fn serialized_files(&self) -> Result<Vec<(String, crate::SerializedFile)>> {
        self.nodes
            .iter()
            .filter(|n| n.is_serialized_file())
            .map(|n| Ok((n.path.clone(), crate::SerializedFile::parse(self.read_node(n)?)?)))
            .collect()
    }

    /// Compression kinds used by the data blocks, as `(kind, count)`.
    pub fn block_compression(&self) -> Vec<(u32, usize)> {
        let mut v: Vec<(u32, usize)> = Vec::new();
        for b in &self.blocks {
            let k = (b.flags & 0x3f) as u32;
            match v.iter_mut().find(|(x, _)| *x == k) {
                Some(e) => e.1 += 1,
                None => v.push((k, 1)),
            }
        }
        v
    }
}

/// Decompresses one Unity block. Kinds: 0 none, 1 LZMA, 2 LZ4, 3 LZ4HC, 4 LZHAM.
pub fn decompress(kind: u32, src: &[u8], size: usize) -> Result<Vec<u8>> {
    match kind {
        0 => {
            if src.len() < size {
                return format_err("stored block shorter than its size");
            }
            Ok(src[..size].to_vec())
        }
        1 => {
            // Unity writes the 5 property bytes but no 8-byte size field.
            let opts = lzma_rs::decompress::Options {
                unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(Some(size as u64)),
                ..Default::default()
            };
            let mut out = Vec::with_capacity(size);
            lzma_rs::lzma_decompress_with_options(&mut &src[..], &mut out, &opts)
                .map_err(|e| Error::Format(format!("lzma: {e}")))?;
            if out.len() != size {
                return format_err(format!("lzma gave {} bytes, want {size}", out.len()));
            }
            Ok(out)
        }
        2 | 3 => {
            let mut out = vec![0u8; size];
            let n = lz4_flex::block::decompress_into(src, &mut out).map_err(|e| Error::Format(format!("lz4: {e}")))?;
            if n != size {
                return format_err(format!("lz4 gave {n} bytes, want {size}"));
            }
            Ok(out)
        }
        4 => unsupported("LZHAM compression"),
        k => unsupported(format!("compression kind {k}")),
    }
}
