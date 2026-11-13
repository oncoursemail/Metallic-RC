//! **CircuitsV2 (CV2) runtime** — Rec Room's gameplay engine, reimplemented in Rust.
//!
//! Rec Room's built-in games (Paintball, Dodgeball, Golf, Quest, Laser Tag, …) are authored as **Circuits**: each
//! game's rules are a `*_GameRuleWrapperCircuit`, a graph of typed nodes ("chips") run by the CV2 engine. So running
//! the games 1:1 rests on this one foundation — a faithful CV2 runtime — with each game then being its rule circuit +
//! gadget prefabs + room scene on top.
//!
//! The engine mirrors the real CV2 (`CircuitsV2Manager` + `GraphSys`): a data-oriented dataflow graph of nodes with
//! input/output ports, connected by data wires (carry [`Value`]s) and exec wires (carry execution pulses). This crate
//! is engine-agnostic (no Bevy) so both the client and the authoritative server can drive it.
//!
//! Status: the graph model, node registry, and pull-based data executor are in; the built-in logic/math/compare nodes
//! are implemented. Next: the protobuf room-save loader (schema mapped), exec/event firing, then the Dodgeball
//! vertical slice and the game-specific nodes (implemented from their decompiled update fns).

pub mod exec;
pub mod graph;
pub mod load;
pub mod node;
pub mod nodes;
pub mod pb;
pub mod registry;
pub mod value;

pub use exec::Executor;
pub use graph::{Edge, EdgeKind, Graph, Node, NodeId, Port};
pub use node::{EvalCtx, NodeType};
pub use registry::Registry;
pub use value::Value;

#[cfg(test)]
mod tests {
    use super::*;

    /// A small data graph evaluates correctly: (2 + 3) == 5, and a selector picks the right branch.
    #[test]
    fn dataflow_evaluates() {
        let reg = Registry::with_builtins();
        let mut g = Graph::default();
        let two = g.add_node("builtin.const", vec![Value::Float(2.0)], 0, 1);
        let three = g.add_node("builtin.const", vec![Value::Float(3.0)], 0, 1);
        let add = g.add_node("builtin.add", vec![], 2, 1);
        g.connect(EdgeKind::Data, Port::new(two, 0), Port::new(add, 0));
        g.connect(EdgeKind::Data, Port::new(three, 0), Port::new(add, 1));
        let five = g.add_node("builtin.const", vec![Value::Float(5.0)], 0, 1);
        let eq = g.add_node("builtin.equals", vec![], 2, 1);
        g.connect(EdgeKind::Data, Port::new(add, 0), Port::new(eq, 0));
        g.connect(EdgeKind::Data, Port::new(five, 0), Port::new(eq, 1));

        let mut ex = Executor::new(&g, &reg);
        assert_eq!(ex.output(add, 0), Value::Float(5.0));
        assert_eq!(ex.output(eq, 0), Value::Bool(true));

        // a selector gated on the equality picks input 1 (true branch)
        let yes = g.add_node("builtin.const", vec![Value::Int(111)], 0, 1);
        let no = g.add_node("builtin.const", vec![Value::Int(222)], 0, 1);
        let sel = g.add_node("builtin.select", vec![], 3, 1);
        g.connect(EdgeKind::Data, Port::new(eq, 0), Port::new(sel, 0));
        g.connect(EdgeKind::Data, Port::new(yes, 0), Port::new(sel, 1));
        g.connect(EdgeKind::Data, Port::new(no, 0), Port::new(sel, 2));
        let mut ex = Executor::new(&g, &reg);
        assert_eq!(ex.output(sel, 0), Value::Int(111));
    }

    /// An unconnected input reads as None, and an unknown node type yields its declared count of empty outputs.
    #[test]
    fn unconnected_and_unknown() {
        let reg = Registry::with_builtins();
        let mut g = Graph::default();
        let add = g.add_node("builtin.add", vec![], 2, 1);
        let mystery = g.add_node("game.unimplemented", vec![], 1, 2);
        let mut ex = Executor::new(&g, &reg);
        assert_eq!(ex.output(add, 0), Value::Float(0.0));
        assert_eq!(ex.output(mystery, 1), Value::None);
    }
}
