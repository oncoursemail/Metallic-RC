//! A minimal protobuf **wire-format** reader — enough to walk CircuitsV2's room-save messages without a `.proto`.
//! Protobuf is self-describing at the wire level: each field is a tag (`field_number << 3 | wire_type`) followed by
//! a value whose encoding is given by the wire type. So we can traverse the documented message structure (nodes at
//! field 11, edges at field 5/6, the per-node type at its oneof field number, …) by field number + wire type alone,
//! which is exactly how the room-save load works (see sweet-galileo_cv2-protobuf-schema.md).

/// Protobuf wire types.
pub mod wire {
    pub const VARINT: u8 = 0;
    pub const I64: u8 = 1;
    pub const LEN: u8 = 2;
    pub const I32: u8 = 5;
}

/// A cursor over a protobuf message's bytes.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub fn eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    /// Reads a base-128 varint.
    pub fn varint(&mut self) -> Option<u64> {
        let mut out: u64 = 0;
        let mut shift = 0;
        loop {
            let b = *self.buf.get(self.pos)?;
            self.pos += 1;
            out |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(out);
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
    }

    /// Reads a field tag, returning `(field_number, wire_type)`.
    pub fn tag(&mut self) -> Option<(u32, u8)> {
        let t = self.varint()?;
        Some(((t >> 3) as u32, (t & 7) as u8))
    }

    /// Reads a length-delimited field's bytes (a sub-message or string).
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.varint()? as usize;
        let end = self.pos.checked_add(len)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }

    pub fn fixed32(&mut self) -> Option<u32> {
        let b = self.buf.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(b.try_into().ok()?))
    }

    pub fn fixed64(&mut self) -> Option<u64> {
        let b = self.buf.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }

    /// Skips the value of a field with the given wire type. Returns `None` on an unknown wire type or truncation.
    pub fn skip(&mut self, wire: u8) -> Option<()> {
        match wire {
            wire::VARINT => self.varint().map(|_| ()),
            wire::I64 => self.fixed64().map(|_| ()),
            wire::LEN => self.bytes().map(|_| ()),
            wire::I32 => self.fixed32().map(|_| ()),
            _ => None,
        }
    }
}

/// Encodes a tag + varint value (test/encoding helper).
pub fn put_varint_field(out: &mut Vec<u8>, field: u32, value: u64) {
    put_varint(out, ((field as u64) << 3) | wire::VARINT as u64);
    put_varint(out, value);
}

/// Encodes a tag + length-delimited bytes (test/encoding helper).
pub fn put_len_field(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    put_varint(out, ((field as u64) << 3) | wire::LEN as u64);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

/// Encodes a base-128 varint.
pub fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fields() {
        // field 3 = varint 150, field 11 = bytes "hi"
        let mut buf = Vec::new();
        put_varint_field(&mut buf, 3, 150);
        put_len_field(&mut buf, 11, b"hi");
        let mut r = Reader::new(&buf);
        let (f1, w1) = r.tag().unwrap();
        assert_eq!((f1, w1), (3, wire::VARINT));
        assert_eq!(r.varint().unwrap(), 150);
        let (f2, w2) = r.tag().unwrap();
        assert_eq!((f2, w2), (11, wire::LEN));
        assert_eq!(r.bytes().unwrap(), b"hi");
        assert!(r.eof());
    }
}
