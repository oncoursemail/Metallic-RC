//! Reads Unity 6 asset bundles (UnityFS) and the serialized files inside them, from a local install.
//!
//! Layers, bottom up: [`Bundle`] (container + decompression), [`SerializedFile`] (object table and
//! type trees), [`Value`] (any object as a tree, driven by its type tree) and typed views in
//! [`objects`] (GameObject, Transform, Mesh, Texture2D, Material, ...). [`catalog`] reads the
//! Addressables `catalog.bin`.

mod reader;
pub mod animclip;
pub mod bundle;
pub mod catalog;
pub mod decrypt;
pub mod classes;
pub mod level;
pub mod objects;
pub mod serialized;
pub mod stripped;
pub mod texture;
pub mod value;
#[cfg(test)]
mod tests;

pub use animclip::{AnimationClip, Attribute, Binding};
pub use bundle::{Bundle, BundleHeader, Node};
pub use objects::*;
pub use reader::Reader;
pub use serialized::{External, ObjectInfo, SerializedFile, SerializedType, TypeNode, TypeTree};
pub use value::{PPtr, Value};
/// The LZ4 block codec the bundles use, for callers that keep their own compressed data (the client's cache).
pub use lz4_flex;

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// Ran past the end of the input.
    Eof { at: usize, need: usize, len: usize },
    /// Bytes do not match the expected layout.
    Format(String),
    /// Valid input that this crate does not handle (yet).
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Eof { at, need, len } => write!(f, "eof: need {need} bytes at {at} of {len}"),
            Error::Format(m) => write!(f, "format: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub(crate) fn format_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Format(msg.into()))
}

pub(crate) fn unsupported<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Unsupported(msg.into()))
}
