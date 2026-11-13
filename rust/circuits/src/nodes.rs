//! Built-in node implementations whose behaviour is unambiguous from the node name (constants, logic, math, compare).
//! These use placeholder `"builtin.*"` UUIDs until the real CircuitsV2 node-type UUIDs are wired in from the save
//! schema (a node's protobuf oneof case — e.g. `ColorConstantNodeData`, `EqualsNodeV2` — names its type). The
//! game-specific nodes are implemented from their decompiled update fns as a game's graph requires them.

use crate::node::{EvalCtx, NodeType};
use crate::registry::Registry;
use crate::value::Value;

/// Defines a pure data node type: a struct implementing [`NodeType`] with the given UUID and `eval` body.
macro_rules! data_node {
    ($name:ident, $uuid:literal, |$ctx:ident| $body:expr) => {
        pub struct $name;
        impl NodeType for $name {
            fn uuid(&self) -> &str {
                $uuid
            }
            fn eval(&self, $ctx: &EvalCtx) -> Vec<Value> {
                $body
            }
        }
    };
}

fn nth(ctx: &EvalCtx, i: usize) -> f32 {
    ctx.inputs.get(i).map(Value::as_f32).unwrap_or(0.0)
}

// constant: emits its first config value
data_node!(ConstNode, "builtin.const", |ctx| vec![ctx.config.first().cloned().unwrap_or_default()]);

// arithmetic
data_node!(AddNode, "builtin.add", |ctx| vec![Value::Float(ctx.inputs.iter().map(Value::as_f32).sum())]);
data_node!(SubNode, "builtin.sub", |ctx| vec![Value::Float(nth(ctx, 0) - nth(ctx, 1))]);
data_node!(MulNode, "builtin.mul", |ctx| vec![Value::Float(ctx.inputs.iter().map(Value::as_f32).product())]);
data_node!(DivNode, "builtin.div", |ctx| {
    let d = nth(ctx, 1);
    vec![Value::Float(if d != 0.0 { nth(ctx, 0) / d } else { 0.0 })]
});
data_node!(MinNode, "builtin.min", |ctx| vec![Value::Float(nth(ctx, 0).min(nth(ctx, 1)))]);
data_node!(MaxNode, "builtin.max", |ctx| vec![Value::Float(nth(ctx, 0).max(nth(ctx, 1)))]);

// comparison
data_node!(EqualsNode, "builtin.equals", |ctx| vec![Value::Bool((nth(ctx, 0) - nth(ctx, 1)).abs() < 1e-6)]);
data_node!(GreaterNode, "builtin.greater", |ctx| vec![Value::Bool(nth(ctx, 0) > nth(ctx, 1))]);
data_node!(LessNode, "builtin.less", |ctx| vec![Value::Bool(nth(ctx, 0) < nth(ctx, 1))]);

// logic
data_node!(AndNode, "builtin.and", |ctx| vec![Value::Bool(!ctx.inputs.is_empty() && ctx.inputs.iter().all(Value::as_bool))]);
data_node!(OrNode, "builtin.or", |ctx| vec![Value::Bool(ctx.inputs.iter().any(Value::as_bool))]);
data_node!(NotNode, "builtin.not", |ctx| vec![Value::Bool(!ctx.inputs.first().map(Value::as_bool).unwrap_or(false))]);

// selector: input 0 is the bool, outputs the value of input 1 (true) or input 2 (false)
data_node!(SelectNode, "builtin.select", |ctx| {
    let pick = if ctx.inputs.first().map(Value::as_bool).unwrap_or(false) { 1 } else { 2 };
    vec![ctx.inputs.get(pick).cloned().unwrap_or_default()]
});

/// Registers the built-in logic/math/compare nodes.
pub fn register_builtins(r: &mut Registry) {
    r.register(Box::new(ConstNode));
    r.register(Box::new(AddNode));
    r.register(Box::new(SubNode));
    r.register(Box::new(MulNode));
    r.register(Box::new(DivNode));
    r.register(Box::new(MinNode));
    r.register(Box::new(MaxNode));
    r.register(Box::new(EqualsNode));
    r.register(Box::new(GreaterNode));
    r.register(Box::new(LessNode));
    r.register(Box::new(AndNode));
    r.register(Box::new(OrNode));
    r.register(Box::new(NotNode));
    r.register(Box::new(SelectNode));
}
