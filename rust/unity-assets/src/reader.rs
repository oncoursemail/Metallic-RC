//! Bounds-checked cursor over a byte slice with switchable endianness.

use crate::{Error, Result};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    pub big_endian: bool,
}

macro_rules! num {
    ($($name:ident: $t:ty),*) => {$(
        pub fn $name(&mut self) -> Result<$t> {
            let b = self.bytes(std::mem::size_of::<$t>())?.try_into().unwrap();
            Ok(if self.big_endian { <$t>::from_be_bytes(b) } else { <$t>::from_le_bytes(b) })
        }
    )*};
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], big_endian: bool) -> Self {
        Reader { data, pos: 0, big_endian }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(Error::Eof { at: self.pos, need: n, len: self.data.len() });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    num!(u16: u16, i16: i16, u32: u32, i32: i32, u64: u64, i64: i64, f32: f32, f64: f64);

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    /// Null-terminated string (lossy UTF-8).
    pub fn cstr(&mut self) -> Result<String> {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let end = rest.iter().position(|&b| b == 0).ok_or(Error::Eof {
            at: self.pos,
            need: rest.len() + 1,
            len: self.data.len(),
        })?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(s)
    }

    /// Unity string: i32 length, bytes, then align to 4.
    pub fn aligned_string(&mut self) -> Result<String> {
        let n = self.len_prefix(1)?;
        let s = String::from_utf8_lossy(self.bytes(n)?).into_owned();
        self.align(4);
        Ok(s)
    }

    /// i32 element count, checked against the bytes left (each element at least `min_size`).
    pub fn len_prefix(&mut self, min_size: usize) -> Result<usize> {
        let at = self.pos;
        let n = self.i32()?;
        if n < 0 || (n as usize).saturating_mul(min_size) > self.remaining() {
            return Err(Error::Format(format!("bad length {n} at {at}")));
        }
        Ok(n as usize)
    }

    /// Aligns the position (absolute within this slice) up to a multiple of `n`.
    pub fn align(&mut self, n: usize) {
        self.pos = self.pos.div_ceil(n) * n;
    }
}
