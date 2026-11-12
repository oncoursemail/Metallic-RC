//! Values that flow on circuit data wires. CV2 ports are typed; this is the Rust union of the common port types
//! (bool/int/float/vector/quaternion/color/text and networked object & player handles). The exact CV2 type set lives
//! in the decompiled node ports; this grows as real nodes are implemented, but the arithmetic/logic coercions below
//! cover the common cases the logic/compare nodes need.

/// A value on a data port.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    /// an unconnected / empty port
    #[default]
    None,
    Bool(bool),
    Int(i64),
    Float(f32),
    Vec3([f32; 3]),
    Quat([f32; 4]),
    Color([f32; 4]),
    Text(String),
    /// a networked object/gadget, by room view id (see rr-net::objects)
    Object(u32),
    /// a player, by actor id
    Player(u32),
}

impl Value {
    /// Truthiness for logic nodes: Bool(true), any non-zero number, or a present handle.
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Object(id) | Value::Player(id) => *id != 0,
            _ => false,
        }
    }

    /// Numeric value for math/compare nodes (bools as 0/1; non-numbers as 0).
    pub fn as_f32(&self) -> f32 {
        match self {
            Value::Float(f) => *f,
            Value::Int(i) => *i as f32,
            Value::Bool(b) => *b as i32 as f32,
            _ => 0.0,
        }
    }

    pub fn as_i64(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => *b as i64,
            _ => 0,
        }
    }
}
