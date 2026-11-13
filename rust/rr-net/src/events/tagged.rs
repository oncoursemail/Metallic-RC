//! The tagged object format of relayed event content: a 1-byte `SerializedType` tag, then the value. Only the types
//! the room layer uses so far; any other tag is refused.
use super::{In, Malformed, Out};

const NULL: u8 = 0;
const OBJECT: u8 = 1;
const ARRAY: u8 = 2;
const BOOL: u8 = 4;
const BYTE: u8 = 5;
const SHORT: u8 = 8;
const INT: u8 = 10;
const LONG: u8 = 12;
const FLOAT: u8 = 14;
const STRING: u8 = 16;
const VECTOR3: u8 = 18;
const QUATERNION: u8 = 21;
const BYTE_ARRAY: u8 = 24;
const INT_ARRAY: u8 = 29;
const DICTIONARY: u8 = 52;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Byte(u8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    Vector3([f32; 3]),
    /// x, y, z, w
    Quaternion([f32; 4]),
    ByteArray(Vec<u8>),
    IntArray(Vec<i32>),
    /// `object[]`: count, element tag Object, then each element with its own tag
    Array(Vec<Value>),
    /// count, then tagged key and tagged value pairs (our reading of tag 52)
    Dictionary(Vec<(Value, Value)>),
}

impl Value {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        self.write(&mut o);
        o.0
    }

    pub fn decode(b: &[u8]) -> Result<Value, Malformed> {
        let mut r = In::new(b);
        let v = Value::read(&mut r, 0)?;
        if r.at != b.len() {
            return Err(Malformed);
        }
        Ok(v)
    }

    pub fn write(&self, o: &mut Out) {
        match self {
            Value::Null => o.u8(NULL),
            Value::Bool(v) => {
                o.u8(BOOL);
                o.bool(*v);
            }
            Value::Byte(v) => {
                o.u8(BYTE);
                o.u8(*v);
            }
            Value::Short(v) => {
                o.u8(SHORT);
                o.i16(*v);
            }
            Value::Int(v) => {
                o.u8(INT);
                o.i32(*v);
            }
            Value::Long(v) => {
                o.u8(LONG);
                o.i64(*v);
            }
            Value::Float(v) => {
                o.u8(FLOAT);
                o.f32(*v);
            }
            Value::String(s) => {
                o.u8(STRING);
                o.string(s);
            }
            Value::Vector3(v) => {
                o.u8(VECTOR3);
                v.iter().for_each(|&f| o.f32(f));
            }
            Value::Quaternion(q) => {
                o.u8(QUATERNION);
                q.iter().for_each(|&f| o.f32(f));
            }
            Value::ByteArray(b) => {
                o.u8(BYTE_ARRAY);
                o.i32(b.len() as i32);
                o.0.extend_from_slice(b);
            }
            Value::IntArray(v) => {
                o.u8(INT_ARRAY);
                o.i32(v.len() as i32);
                v.iter().for_each(|&i| o.i32(i));
            }
            Value::Array(v) => {
                o.u8(ARRAY);
                o.i32(v.len() as i32);
                o.u8(OBJECT);
                v.iter().for_each(|e| e.write(o));
            }
            Value::Dictionary(v) => {
                o.u8(DICTIONARY);
                o.i32(v.len() as i32);
                for (k, e) in v {
                    k.write(o);
                    e.write(o);
                }
            }
        }
    }

    /// `depth` bounds nesting so a hostile packet cannot overflow the stack.
    pub fn read(r: &mut In, depth: u32) -> Result<Value, Malformed> {
        if depth > 16 {
            return Err(Malformed);
        }
        Ok(match r.u8()? {
            NULL => Value::Null,
            BOOL => Value::Bool(r.bool()?),
            BYTE => Value::Byte(r.u8()?),
            SHORT => Value::Short(r.i16()?),
            INT => Value::Int(r.i32()?),
            LONG => Value::Long(r.i64()?),
            FLOAT => Value::Float(r.f32()?),
            STRING => Value::String(r.string()?),
            VECTOR3 => Value::Vector3([r.f32()?, r.f32()?, r.f32()?]),
            QUATERNION => Value::Quaternion([r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
            BYTE_ARRAY => {
                let n = r.count(1)?;
                Value::ByteArray(r.take(n)?.to_vec())
            }
            INT_ARRAY => {
                let n = r.count(4)?;
                Value::IntArray((0..n).map(|_| r.i32()).collect::<Result<_, _>>()?)
            }
            ARRAY => {
                let n = r.count(1)?;
                if r.u8()? != OBJECT {
                    return Err(Malformed);
                }
                Value::Array((0..n).map(|_| Value::read(r, depth + 1)).collect::<Result<_, _>>()?)
            }
            DICTIONARY => {
                let n = r.count(2)?;
                Value::Dictionary((0..n).map(|_| Ok((Value::read(r, depth + 1)?, Value::read(r, depth + 1)?))).collect::<Result<_, _>>()?)
            }
            _ => return Err(Malformed),
        })
    }

    /// The entry of a byte-keyed dictionary (instantiate and RPC content use byte keys).
    pub fn get(&self, key: u8) -> Option<&Value> {
        match self {
            Value::Dictionary(v) => v.iter().find(|(k, _)| *k == Value::Byte(key)).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_refuses_junk() {
        let v = Value::Dictionary(vec![
            (Value::Byte(0), Value::String("Player ü".into())),
            (Value::Byte(1), Value::Vector3([1.0, 2.0, -3.5])),
            (Value::Byte(2), Value::Quaternion([0.0, 0.0, 0.0, 1.0])),
            (Value::Byte(5), Value::IntArray(vec![100001])),
            (Value::Byte(6), Value::Array(vec![Value::Null, Value::Bool(true), Value::Short(-2), Value::Long(1 << 40), Value::Float(0.5), Value::ByteArray(vec![1, 2])])),
        ]);
        let b = v.encode();
        assert_eq!(Value::decode(&b), Ok(v.clone()));
        assert_eq!(v.get(0).and_then(Value::as_str), Some("Player ü"));
        assert_eq!(Value::decode(&b[..b.len() - 1]), Err(Malformed));
        assert_eq!(Value::decode(&[DICTIONARY, 0xff, 0xff, 0xff, 0x7f]), Err(Malformed));
        assert_eq!(Value::decode(&[99]), Err(Malformed));
        // nesting beyond the bound is refused, not followed
        let deep: Vec<u8> = std::iter::repeat_n([ARRAY, 1, 0, 0, 0, OBJECT], 40).flatten().chain([NULL]).collect();
        assert_eq!(Value::decode(&deep), Err(Malformed));
    }
}
