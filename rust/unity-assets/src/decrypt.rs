//! Rec Room 2026 base-file encryption (`globalgamemanagers`, `levelN`, `resources.assets`,
//! `sharedassets*.assets`, and some `.resS`).
//!
//! Each encrypted file begins with the 8-byte marker [`MARKER`] followed by a ciphertext body,
//! applied by the game's **Referee.dll anti-cheat** (it inline-hooks `ntdll!NtReadFile` and decrypts
//! matching reads transparently). The decrypt was reverse-engineered through the anti-cheat's
//! obfuscation (see `docs/DECRYPTION_RE_STATUS.md`); it is NOT a PRG but a byte-subtract + XOR
//! against a **static 2 MB keystream pad**:
//!
//! ```text
//! plain[i] = ((ct[i] -/+ sub[op]) & 0xFF) XOR ks[(offset + i + base[op]) & 0x1FFFFF]   for i >= 8
//! plain[0..8] = header8[op]                                                             (marker region)
//! ```
//!
//! where `op` is the file's asset-manifest index (filename→op), and `ks`, `sub[op]`, `base[op]`,
//! `header8[op]` were extracted once from the game. [`Cipher`] implements this exactly (validated
//! byte-for-byte against the full decrypted corpus). The pad + tables are game-derived, so they live
//! in a LOCAL (gitignored) data directory, never committed.
//!
//! Two consumption paths: [`Cipher`] decrypts on the fly from the installed game's extracted tables;
//! [`read_base_file`] / [`plaintext_dir`] resolve an encrypted install to the pre-decrypted corpus.

use crate::{Error, Result};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// The 8-byte literal header on every encrypted base file: `00 00 52 52 00 00 52 52` ("RR").
pub const MARKER: [u8; 8] = [0x00, 0x00, 0x52, 0x52, 0x00, 0x00, 0x52, 0x52];

/// Whether `bytes` begins with the encryption [`MARKER`] (so it is an encrypted base file).
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.len() >= MARKER.len() && bytes[..MARKER.len()] == MARKER
}

/// Whether the file at `path` begins with the encryption [`MARKER`]. Returns `Ok(false)` for a
/// file shorter than the marker; propagates other I/O errors.
pub fn file_is_encrypted(path: &Path) -> io::Result<bool> {
    use io::Read;
    let mut head = [0u8; 8];
    let mut f = std::fs::File::open(path)?;
    match f.read_exact(&mut head) {
        Ok(()) => Ok(head == MARKER),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

/// Reads a base file by name, returning plaintext bytes.
///
/// Reads `data_dir/name`. If it is already plaintext, returns it unchanged. If it carries the
/// encryption [`MARKER`], returns the decrypted copy from `corpus_dir/name` instead; if no corpus
/// copy exists, returns [`io::ErrorKind::NotFound`] (the bytes cannot be decrypted in-process —
/// the corpus must be produced out of band, see module docs).
///
/// `corpus_dir` is the local decrypted corpus (gitignored; never committed).
pub fn read_base_file(data_dir: &Path, corpus_dir: &Path, name: &str) -> io::Result<Vec<u8>> {
    let src = data_dir.join(name);
    let bytes = std::fs::read(&src)?;
    if !is_encrypted(&bytes) {
        return Ok(bytes);
    }
    let corpus = corpus_dir.join(name);
    let plain = std::fs::read(&corpus).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "{name} is encrypted and no decrypted corpus copy exists at {}: {e}",
                corpus.display()
            ),
        )
    })?;
    if is_encrypted(&plain) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("corpus copy of {name} is still encrypted"),
        ));
    }
    Ok(plain)
}

/// Resolves the directory to read a base file's plaintext from: `corpus_dir` when `data_dir`'s
/// `globalgamemanagers` is encrypted (so the whole install is encrypted), else `data_dir` itself.
/// Lets a caller point an existing reader (e.g. the client's reference-install path) at plaintext.
pub fn plaintext_dir(data_dir: &Path, corpus_dir: &Path) -> PathBuf {
    match file_is_encrypted(&data_dir.join("globalgamemanagers")) {
        Ok(true) => corpus_dir.to_path_buf(),
        _ => data_dir.to_path_buf(),
    }
}

/// Mod mask for the 2 MB keystream pad (`& 0x1FFFFF` = mod 2^21).
pub const KS_MASK: usize = 0x1F_FFFF;
/// Size of the keystream pad (2 MB).
pub const KS_SIZE: usize = 0x20_0000;

#[derive(Clone, Copy)]
struct OpEntry {
    base: u32,
    sub: u8,
    kind: u8, // 0 = subtract, 1 = add
    header8: [u8; 8],
    valid: bool,
}

/// The reversed Rec Room base-file cipher: a byte-subtract + XOR against a static keystream pad.
///
/// Load it from a local data directory produced by the extraction tooling (`ks.bin`, `ops.bin`,
/// `file_ops.tsv` — see the module docs and `docs/DECRYPTION_RE_STATUS.md`). Then
/// [`Cipher::decrypt_file`] turns an encrypted base file into plaintext with no running game.
pub struct Cipher {
    ks: Vec<u8>,
    ops: Vec<OpEntry>,
    file_ops: HashMap<String, u32>,
}

impl Cipher {
    /// Loads the keystream pad and per-op tables from `dir` (`ks.bin`, `ops.bin`, `file_ops.tsv`).
    pub fn load(dir: &Path) -> io::Result<Self> {
        let ks = std::fs::read(dir.join("ks.bin"))?;
        if ks.len() != KS_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ks.bin is not 2 MB"));
        }
        // ops.bin: entries of 14 bytes [base u32 LE, sub u8, kind u8, header8[8]]
        let raw = std::fs::read(dir.join("ops.bin"))?;
        let mut ops = Vec::with_capacity(raw.len() / 14);
        for c in raw.chunks_exact(14) {
            let base = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
            ops.push(OpEntry {
                base,
                sub: c[4],
                kind: c[5],
                header8: c[6..14].try_into().unwrap(),
                valid: base != 0xFFFF_FFFF,
            });
        }
        let mut file_ops = HashMap::new();
        for line in std::fs::read_to_string(dir.join("file_ops.tsv"))?.lines() {
            if let Some((name, op)) = line.split_once('\t') {
                if let Ok(op) = op.trim().strip_prefix("0x").map_or_else(|| op.trim().parse::<u32>(), |h| u32::from_str_radix(h, 16)) {
                    file_ops.insert(name.to_string(), op);
                }
            }
        }
        Ok(Cipher { ks, ops, file_ops })
    }

    /// The asset-manifest `op` for a base-file name, if known.
    pub fn op_for(&self, filename: &str) -> Option<u32> {
        self.file_ops.get(filename).copied()
    }

    /// Decrypts an encrypted base file's bytes under `op`, reading from file `offset` (0 for a whole
    /// file). Returns plaintext of the same length. Errors if `op` is unknown/unused.
    pub fn decrypt(&self, ciphertext: &[u8], op: u32, offset: usize) -> Result<Vec<u8>> {
        let e = self
            .ops
            .get(op as usize)
            .filter(|e| e.valid)
            .ok_or_else(|| Error::Unsupported(format!("no cipher table for op {op:#x}")))?;
        let n = ciphertext.len();
        let mut out = ciphertext.to_vec();
        // marker region (the first 8 bytes of the whole file) → the op's plaintext header
        if offset == 0 {
            let h = n.min(8);
            out[..h].copy_from_slice(&e.header8[..h]);
        }
        let base = e.base as usize;
        let start = if offset == 0 { 8 } else { 0 };
        for i in start..n {
            let pos = offset + i;
            let b = if e.kind == 0 { ciphertext[i].wrapping_sub(e.sub) } else { ciphertext[i].wrapping_add(e.sub) };
            out[i] = b ^ self.ks[(pos + base) & KS_MASK];
        }
        Ok(out)
    }

    /// Decrypts a whole encrypted base file by name (maps the name to its `op`, then decrypts from
    /// offset 0). Returns `None` for an unknown file name.
    pub fn decrypt_file(&self, ciphertext: &[u8], filename: &str) -> Option<Result<Vec<u8>>> {
        let op = self.op_for(filename)?;
        Some(self.decrypt(ciphertext, op, 0))
    }

    /// Reads a base file straight from an install's `RecRoom_Data` (`data_dir`) and returns its
    /// plaintext, decrypting on the fly with the loaded keystream pad — no pre-decrypted corpus
    /// needed. A file without the encryption [`MARKER`] is returned unchanged. This is how the client
    /// reads the user's OWN encrypted install (animations/audio/particles in `resources.assets`, the
    /// scenes, etc.) with no shipped game data beyond the locally-built pad.
    pub fn decrypt_install_file(&self, data_dir: &Path, name: &str) -> io::Result<Vec<u8>> {
        let bytes = std::fs::read(data_dir.join(name))?;
        if !is_encrypted(&bytes) {
            return Ok(bytes);
        }
        match self.decrypt_file(&bytes, name) {
            Some(Ok(plain)) => Ok(plain),
            Some(Err(e)) => Err(io::Error::new(io::ErrorKind::InvalidData, format!("decrypt {name}: {e}"))),
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("no cipher op for {name}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_detection() {
        let mut enc = MARKER.to_vec();
        enc.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        assert!(is_encrypted(&enc));

        // a plaintext Unity SerializedFile legacy header is all-zero, never the marker
        let plain = [0u8; 16];
        assert!(!is_encrypted(&plain));

        assert!(!is_encrypted(&[0x00, 0x00, 0x52])); // too short
        assert!(!is_encrypted(&[])); // empty
    }

    /// End-to-end: load the extracted cipher tables and confirm the Rust decrypt reproduces the
    /// decrypted corpus byte-for-byte, with no running game. Gated on `RR_CIPHER_DIR` (the local,
    /// gitignored data dir) and `RR_DATA_DIR` (the encrypted install's `RecRoom_Data`) and
    /// `RR_CORPUS_DIR` (the plaintext corpus); skipped when unset so CI stays data-free.
    #[test]
    fn decrypt_matches_corpus() {
        let (Ok(cipher_dir), Ok(enc_dir), Ok(corpus_dir)) = (
            std::env::var("RR_CIPHER_DIR"),
            std::env::var("RR_DATA_DIR"),
            std::env::var("RR_CORPUS_DIR"),
        ) else {
            eprintln!("decrypt_matches_corpus: skipped (set RR_CIPHER_DIR/RR_DATA_DIR/RR_CORPUS_DIR)");
            return;
        };
        let cipher = Cipher::load(Path::new(&cipher_dir)).expect("load cipher tables");
        let mut tested = 0;
        for name in ["level0", "level6", "globalgamemanagers", "level103", "sharedassets0.assets"] {
            let enc = std::fs::read(Path::new(&enc_dir).join(name));
            let want = std::fs::read(Path::new(&corpus_dir).join(name));
            let (Ok(enc), Ok(want)) = (enc, want) else { continue };
            let got = cipher.decrypt_file(&enc, name).expect("known op").expect("decrypt");
            assert_eq!(got.len(), want.len(), "{name}: length");
            // bytes >= 8 must match exactly; the 8-byte header comes from the op table
            assert_eq!(&got[8..], &want[8..], "{name}: body mismatch");
            tested += 1;
        }
        assert!(tested > 0, "no files available to test");
    }

    /// Proves the install-decrypt path: [`Cipher::decrypt_install_file`] reproduces the decrypted
    /// corpus BYTE-FOR-BYTE straight from the raw (encrypted) install, with only the locally-built
    /// keystream pad. Gated on `RR_CIPHER_DIR` (pad + tables), `RR_DATA_DIR` (the install's encrypted
    /// `RecRoom_Data`) and `RR_CORPUS_DIR` (the decrypted reference); skipped when unset.
    #[test]
    fn decrypt_install_matches_corpus() {
        let (Ok(cipher_dir), Ok(enc_dir), Ok(corpus_dir)) = (
            std::env::var("RR_CIPHER_DIR"),
            std::env::var("RR_DATA_DIR"),
            std::env::var("RR_CORPUS_DIR"),
        ) else {
            eprintln!("decrypt_install_matches_corpus: skipped (set RR_CIPHER_DIR/RR_DATA_DIR/RR_CORPUS_DIR)");
            return;
        };
        let cipher = Cipher::load(Path::new(&cipher_dir)).expect("load cipher tables");
        let mut tested = 0;
        // resources.assets holds the real animations/audio/particles; the others are scenes/managers
        for name in ["resources.assets", "globalgamemanagers", "level0", "level7", "sharedassets7.assets"] {
            let Ok(want) = std::fs::read(Path::new(&corpus_dir).join(name)) else { continue };
            let got = cipher.decrypt_install_file(Path::new(&enc_dir), name).expect("decrypt install file");
            assert_eq!(got.len(), want.len(), "{name}: length");
            assert_eq!(got, want, "{name}: not byte-for-byte identical to the corpus");
            tested += 1;
        }
        assert!(tested > 0, "no files available to test");
        eprintln!("decrypt_install_matches_corpus: {tested} files decrypted from the raw install, byte-for-byte");
    }
}
