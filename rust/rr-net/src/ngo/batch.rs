//! A batch: what Netcode puts in one transport message. A 24-byte header (magic 0x1160, total size, XXH64 of what
//! follows the header, message count), then each message as packed type id, packed size and body.
use std::fmt;

use twox_hash::XxHash64;

use super::pack::{Reader, Writer};

pub const MAGIC: u16 = 0x1160;
pub const HEADER: usize = 24;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Short,
    Magic(u16),
    Size { said: usize, got: usize },
    Hash,
    Message(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Short => f.write_str("batch shorter than its header"),
            Error::Magic(m) => write!(f, "batch magic {m:#06x}"),
            Error::Size { said, got } => write!(f, "batch says {said} bytes, got {got}"),
            Error::Hash => f.write_str("batch hash mismatch"),
            Error::Message(i) => write!(f, "message {i} of the batch is cut off"),
        }
    }
}

impl std::error::Error for Error {}

/// Collects messages into one batch.
#[derive(Default)]
pub struct Batch {
    body: Writer,
    count: u16,
}

impl Batch {
    pub fn push(&mut self, type_id: u32, message: &[u8]) {
        self.body.u32(type_id);
        self.body.u32(message.len() as u32);
        self.body.raw(message);
        self.count += 1;
    }

    pub fn len(&self) -> usize {
        HEADER + self.body.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn finish(self) -> Vec<u8> {
        let body = self.body.0;
        let mut v = Vec::with_capacity(HEADER + body.len());
        v.extend_from_slice(&MAGIC.to_le_bytes());
        v.extend_from_slice(&[0, 0]);
        v.extend_from_slice(&((HEADER + body.len()) as i32).to_le_bytes());
        v.extend_from_slice(&XxHash64::oneshot(0, &body).to_le_bytes());
        v.extend_from_slice(&self.count.to_le_bytes());
        v.extend_from_slice(&[0; 6]);
        v.extend_from_slice(&body);
        v
    }
}

/// The (type id, body) of each message in a batch.
pub fn read(batch: &[u8]) -> Result<Vec<(u32, &[u8])>, Error> {
    if batch.len() < HEADER {
        return Err(Error::Short);
    }
    let magic = u16::from_le_bytes([batch[0], batch[1]]);
    if magic != MAGIC {
        return Err(Error::Magic(magic));
    }
    let said = i32::from_le_bytes(batch[4..8].try_into().unwrap()) as usize;
    if said != batch.len() {
        return Err(Error::Size { said, got: batch.len() });
    }
    if u64::from_le_bytes(batch[8..16].try_into().unwrap()) != XxHash64::oneshot(0, &batch[HEADER..]) {
        return Err(Error::Hash);
    }
    let count = u16::from_le_bytes([batch[16], batch[17]]) as usize;
    let mut r = Reader::new(&batch[HEADER..]);
    (0..count)
        .map(|i| {
            let t = r.u32().map_err(|_| Error::Message(i))?;
            let n = r.u32().map_err(|_| Error::Message(i))? as usize;
            Ok((t, r.take(n).map_err(|_| Error::Message(i))?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_checks() {
        let mut b = Batch::default();
        b.push(3, b"hello");
        b.push(5, &[]);
        b.push(300, &[7; 70]);
        let v = b.finish();
        assert_eq!(&v[..2], &[0x60, 0x11]);
        assert_eq!(read(&v).unwrap(), vec![(3, &b"hello"[..]), (5, &[][..]), (300, &[7u8; 70][..])]);
        let mut bad = v.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(read(&bad), Err(Error::Hash));
        assert_eq!(read(&v[..v.len() - 1]), Err(Error::Size { said: v.len(), got: v.len() - 1 }));
    }

    #[test]
    fn xxhash_reference_values() {
        // published XXH32/XXH64 values for "" and "abc" (seed 0)
        assert_eq!(XxHash64::oneshot(0, b""), 0xEF46_DB37_51D8_E999);
        assert_eq!(XxHash64::oneshot(0, b"abc"), 0x44BC_2CF5_AD77_0999);
        assert_eq!(twox_hash::XxHash32::oneshot(0, b""), 0x02CC_5D05);
        assert_eq!(twox_hash::XxHash32::oneshot(0, b"abc"), 0x32D1_53FF);
    }
}
