//! The Rec Room layer above NamedMessage (realtime.md section 5): server operations (NamedMessage 5 up, 6 down),
//! relayed events (3 up, 4 down) and the tagged object format their content uses.
//!
//! Field order inside these structs is our best reading (realtime.md open items 1 and 2): declaration order, raw
//! little-endian values, strings as a u32 length and UTF-16 units. Each struct has one codec, so a correction found
//! against the stock client later touches one place.
pub mod ops;
pub mod relay;
pub mod tagged;

use crate::ngo::NgoDelivery;

/// NamedMessage hashes the room connection uses (RR `NamedMessageType`).
pub const RELAY_UP: u16 = 3;
pub const RELAY_DOWN: u16 = 4;
pub const OP_UP: u16 = 5;
pub const OP_DOWN: u16 = 6;

/// `RRNetworkDelivery`, and the Netcode delivery the 2026 client maps each one to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RrDelivery {
    Unreliable = 0,
    UnreliableUnsequenced = 1,
    Reliable = 2,
    ReliableUnsequenced = 3,
}

impl RrDelivery {
    pub fn from_u8(b: u8) -> Option<Self> {
        [Self::Unreliable, Self::UnreliableUnsequenced, Self::Reliable, Self::ReliableUnsequenced].get(b as usize).copied()
    }

    pub fn ngo(self) -> NgoDelivery {
        match self {
            RrDelivery::Unreliable => NgoDelivery::UnreliableSequenced,
            RrDelivery::UnreliableUnsequenced => NgoDelivery::Unreliable,
            RrDelivery::Reliable => NgoDelivery::ReliableFragmentedSequenced,
            RrDelivery::ReliableUnsequenced => NgoDelivery::Reliable,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Malformed;

/// Raw little-endian writing, as Netcode's `WriteValueSafe` does.
#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn bool(&mut self, v: bool) {
        self.0.push(v as u8);
    }
    pub fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn string(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.0.extend_from_slice(&(units.len() as u32).to_le_bytes());
        for u in units {
            self.0.extend_from_slice(&u.to_le_bytes());
        }
    }
}

pub struct In<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

impl<'a> In<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        In { b, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Malformed> {
        let s = self.b.get(self.at..self.at.checked_add(n).ok_or(Malformed)?).ok_or(Malformed)?;
        self.at += n;
        Ok(s)
    }
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.at..];
        self.at = self.b.len();
        s
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], Malformed> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    pub fn u8(&mut self) -> Result<u8, Malformed> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool, Malformed> {
        Ok(self.u8()? != 0)
    }
    pub fn i16(&mut self) -> Result<i16, Malformed> {
        Ok(i16::from_le_bytes(self.arr()?))
    }
    pub fn i32(&mut self) -> Result<i32, Malformed> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    pub fn u64(&mut self) -> Result<u64, Malformed> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    pub fn i64(&mut self) -> Result<i64, Malformed> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    pub fn f32(&mut self) -> Result<f32, Malformed> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    /// A count, refused when the rest of the buffer could not hold that many items of `min` bytes.
    pub fn count(&mut self, min: usize) -> Result<usize, Malformed> {
        let n = self.i32()?;
        if n < 0 || (n as usize).saturating_mul(min) > self.b.len() - self.at {
            return Err(Malformed);
        }
        Ok(n as usize)
    }
    pub fn string(&mut self) -> Result<String, Malformed> {
        let n = self.count(2)?;
        let units: Vec<u16> = self.take(n * 2)?.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).map_err(|_| Malformed)
    }
}
