//! Netcode's byte packing. An unsigned value shifted left by 2/3/4 bits (u16/u32/u64) keeps its own byte count in
//! those low bits and is written in that many little-endian bytes; a value too large for that is an escape count
//! (3/5/9) followed by the raw value. Signed values are zigzagged first. "Safe" values are plain little-endian.
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct Truncated;

impl fmt::Display for Truncated {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("message ends early")
    }
}

impl std::error::Error for Truncated {}

#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    /// `v` with its byte count in the low `bits`; `escape` when it does not fit.
    fn packed(&mut self, v: u64, bits: u32, max: u64, escape: u8, raw: usize) {
        if v > max {
            self.0.push(escape);
            self.0.extend_from_slice(&v.to_le_bytes()[..raw]);
            return;
        }
        let shifted = v << bits;
        let n = (8 - shifted.leading_zeros() as usize / 8).max(1);
        let w = shifted | n as u64;
        self.0.extend_from_slice(&w.to_le_bytes()[..n]);
    }

    pub fn u16(&mut self, v: u16) {
        self.packed(v as u64, 2, 0x3FFF, 3, 2);
    }

    pub fn u32(&mut self, v: u32) {
        self.packed(v as u64, 3, 0x1FFF_FFFF, 5, 4);
    }

    pub fn u64(&mut self, v: u64) {
        self.packed(v, 4, 0x0FFF_FFFF_FFFF_FFFF, 9, 8);
    }

    pub fn i32(&mut self, v: i32) {
        self.u32(((v << 1) ^ (v >> 31)) as u32);
    }

    pub fn i64(&mut self, v: i64) {
        self.u64(((v << 1) ^ (v >> 63)) as u64);
    }

    /// A packed string: its length in UTF-16 units, then each unit packed.
    pub fn string(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.u32(units.len() as u32);
        for u in units {
            self.u16(u);
        }
    }

    pub fn raw(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }

    /// A byte array: i32 length, then the bytes.
    pub fn bytes(&mut self, b: &[u8]) {
        self.raw(&(b.len() as i32).to_le_bytes());
        self.raw(b);
    }
}

pub struct Reader<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Truncated> {
        let s = self.b.get(self.at..self.at + n).ok_or(Truncated)?;
        self.at += n;
        Ok(s)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.at.min(self.b.len())..];
        self.at = self.b.len();
        s
    }

    fn le(&mut self, n: usize) -> Result<u64, Truncated> {
        let mut v = [0u8; 8];
        v[..n].copy_from_slice(self.take(n)?);
        Ok(u64::from_le_bytes(v))
    }

    fn packed(&mut self, bits: u32, escape: u8, raw: usize) -> Result<u64, Truncated> {
        let first = *self.b.get(self.at).ok_or(Truncated)?;
        let n = (first & ((1 << bits) - 1)) as usize;
        if n == escape as usize {
            self.at += 1;
            return self.le(raw);
        }
        if n == 0 || n > raw {
            return Err(Truncated);
        }
        Ok(self.le(n)? >> bits)
    }

    pub fn u16(&mut self) -> Result<u16, Truncated> {
        self.packed(2, 3, 2).map(|v| v as u16)
    }

    pub fn u32(&mut self) -> Result<u32, Truncated> {
        self.packed(3, 5, 4).map(|v| v as u32)
    }

    pub fn u64(&mut self) -> Result<u64, Truncated> {
        self.packed(4, 9, 8)
    }

    pub fn i32(&mut self) -> Result<i32, Truncated> {
        let z = self.u32()?;
        Ok((z >> 1) as i32 ^ -((z & 1) as i32))
    }

    pub fn i64(&mut self) -> Result<i64, Truncated> {
        let z = self.u64()?;
        Ok((z >> 1) as i64 ^ -((z & 1) as i64))
    }

    pub fn string(&mut self) -> Result<String, Truncated> {
        let n = self.u32()? as usize;
        let units = (0..n).map(|_| self.u16()).collect::<Result<Vec<_>, _>>()?;
        Ok(String::from_utf16_lossy(&units))
    }

    pub fn raw_u16(&mut self) -> Result<u16, Truncated> {
        self.le(2).map(|v| v as u16)
    }

    pub fn raw_u32(&mut self) -> Result<u32, Truncated> {
        self.le(4).map(|v| v as u32)
    }

    pub fn raw_u64(&mut self) -> Result<u64, Truncated> {
        self.le(8)
    }

    pub fn bytes(&mut self) -> Result<&'a [u8], Truncated> {
        let n = self.raw_u32()? as usize;
        self.take(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_sizes_and_escapes() {
        let mut w = Writer::default();
        w.u32(0);
        w.u32(31);
        w.u32(32);
        w.u32(0x1FFF_FFFF);
        w.u32(0x2000_0000);
        // 0 and 31 fit one byte with their 3 count bits; 32 needs two; the largest packed needs 4; beyond it, escape 5
        assert_eq!(w.0[..1], [0x01]);
        assert_eq!(w.0[1..2], [31 << 3 | 1]);
        assert_eq!(w.0[2..4], [0x02, 0x01]);
        assert_eq!(w.0[4] & 7, 4);
        assert_eq!(w.0[8..13], [5, 0, 0, 0, 0x20]);
        let mut r = Reader::new(&w.0);
        for v in [0, 31, 32, 0x1FFF_FFFF, 0x2000_0000] {
            assert_eq!(r.u32(), Ok(v));
        }
    }

    #[test]
    fn round_trips() {
        let mut w = Writer::default();
        for v in [0u16, 1, 0x3FFF, 0x4000, u16::MAX] {
            w.u16(v);
        }
        for v in [0u64, 1 << 40, 0x0FFF_FFFF_FFFF_FFFF, u64::MAX] {
            w.u64(v);
        }
        for v in [0, -1, 1, i32::MIN, i32::MAX] {
            w.i32(v);
        }
        w.i64(-123_456_789_000);
        w.string("Carrot, héllo");
        w.bytes(b"abc");
        let mut r = Reader::new(&w.0);
        for v in [0u16, 1, 0x3FFF, 0x4000, u16::MAX] {
            assert_eq!(r.u16(), Ok(v));
        }
        for v in [0u64, 1 << 40, 0x0FFF_FFFF_FFFF_FFFF, u64::MAX] {
            assert_eq!(r.u64(), Ok(v));
        }
        for v in [0, -1, 1, i32::MIN, i32::MAX] {
            assert_eq!(r.i32(), Ok(v));
        }
        assert_eq!(r.i64(), Ok(-123_456_789_000));
        assert_eq!(r.string().as_deref(), Ok("Carrot, héllo"));
        assert_eq!(r.bytes(), Ok(&b"abc"[..]));
        assert_eq!(r.u32(), Err(Truncated));
    }
}
