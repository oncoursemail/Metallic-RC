//! Rec Room 2026 base-file encryption (`globalgamemanagers`, `levelN`, `resources.assets`,
//! `sharedassets*.assets`, and some `.resS`).
//!
//! Each encrypted file begins with the 8-byte marker [`MARKER`] followed by a ciphertext body.
//! The body is a **per-file-keyed stream cipher** (full-entropy, offset/position-based in
//! 256/512-byte blocks) applied by the game's **Referee.dll anti-cheat**, which inline-hooks
//! `ntdll!NtReadFile` and decrypts matching reads transparently. The decrypt routine is
//! **code-virtualized** (a VM dispatcher at `Referee.dll+0x6abb80` jumping through a handler
//! table), so it cannot be faithfully reimplemented here without devirtualizing that VM — see
//! `docs/DECRYPTION_RE_STATUS.md`.
//!
//! Practical path (what this module supports): the complete set of base files is decrypted once,
//! out of band, via the running game as a decryption oracle (read each file back through the
//! active hook; script in the local extraction tree), producing a plaintext **corpus**. This
//! module detects encryption and resolves an encrypted base file to its decrypted corpus copy, so
//! the rest of `unity-assets` can parse it like any plaintext [`crate::SerializedFile`].

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
}
