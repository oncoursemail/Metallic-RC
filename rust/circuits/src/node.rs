//! Node behaviour. Every CV2 node type has an update function; here each registered node type implements
//! [`NodeType`]. Pure data nodes compute their outputs from their inputs + config in [`NodeType::eval`]. Action/event
//! nodes (ones driven by exec pulses, with world side effects) will add a `fire` entry point in the exec milestone;
//! their exact behaviour comes from the decompiled update fns, since the signatures dump has no method bodies.

use crate::value::Value;

/// What a node sees while evaluating: its own constant config and the current values on its input ports.
pub struct EvalCtx<'a> {
    pub config: &'a [Value],
    pub inputs: &'a [Value],
}

/// The behaviour of one CV2 node type, keyed by its UUID in the [`Registry`](crate::registry::Registry).
pub trait NodeType: Sync + Send {
    /// the node-type UUID this implements (CV2 `CircuitNodeType.uuid`)
    fn uuid(&self) -> &str;

    /// Computes the node's output port values from its inputs and config. Pure/data nodes implement this; the default
    /// produces nothing (an action-only node overrides exec handling instead, added in the exec milestone).
    fn eval(&self, _ctx: &EvalCtx) -> Vec<Value> {
        Vec::new()
    }
}
