//! The executor: evaluates a circuit graph's data flow. Mirrors `CircuitsV2Manager`'s role (owns a graph and ticks
//! it). Data evaluation is pull-based with per-tick memoization: asking for an output port's value evaluates that
//! node, pulling its inputs (and their upstream nodes) as needed, caching each node's outputs for the tick. This
//! handles arbitrary DAGs in one pass and guards against feedback cycles (a node already being evaluated yields its
//! default/empty outputs, as a latch would until the next tick). Exec/event firing is the next milestone.

use crate::graph::{Graph, NodeId, Port};
use crate::node::EvalCtx;
use crate::registry::Registry;
use crate::value::Value;

/// Evaluates data flow over a graph for one tick.
pub struct Executor<'a> {
    graph: &'a Graph,
    reg: &'a Registry,
    /// memoized output values per node for the current tick (None = not yet evaluated)
    cache: Vec<Option<Vec<Value>>>,
    /// nodes currently on the evaluation stack, to break cycles
    visiting: Vec<bool>,
}

impl<'a> Executor<'a> {
    pub fn new(graph: &'a Graph, reg: &'a Registry) -> Self {
        let n = graph.nodes.len();
        Executor { graph, reg, cache: vec![None; n], visiting: vec![false; n] }
    }

    /// Clears the per-tick memoization (call at the start of each tick).
    pub fn reset(&mut self) {
        for c in &mut self.cache {
            *c = None;
        }
    }

    /// The value on a node's output port, evaluating upstream nodes as needed.
    pub fn output(&mut self, node: NodeId, port: u16) -> Value {
        self.eval_node(node);
        self.cache[node as usize]
            .as_ref()
            .and_then(|o| o.get(port as usize))
            .cloned()
            .unwrap_or_default()
    }

    /// The value arriving at an input port: the source output's value, or `None` if the port is unconnected.
    fn input(&mut self, port: Port) -> Value {
        match self.graph.source_of(port) {
            Some(src) => self.output(src.node, src.port),
            None => Value::None,
        }
    }

    fn eval_node(&mut self, node: NodeId) {
        let i = node as usize;
        if self.cache[i].is_some() {
            return;
        }
        if self.visiting[i] {
            // a cycle: yield empty outputs this tick (a real latch node carries its own prior state; added later)
            self.cache[i] = Some(vec![Value::None; self.graph.nodes[i].outputs as usize]);
            return;
        }
        self.visiting[i] = true;
        let n = &self.graph.nodes[i];
        let inputs: Vec<Value> = (0..n.inputs).map(|p| self.input(Port::new(node, p))).collect();
        let mut out = match self.reg.get(&n.uuid) {
            Some(t) => t.eval(&EvalCtx { config: &n.config, inputs: &inputs }),
            None => Vec::new(),
        };
        // a node always exposes its declared number of output ports (unknown/action nodes yield empty values)
        out.resize(n.outputs as usize, Value::None);
        self.visiting[i] = false;
        self.cache[i] = Some(out);
    }
}
