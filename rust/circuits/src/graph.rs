//! The circuit graph: typed nodes with input/output ports connected by edges. This mirrors CV2's `GraphSys` — a
//! data-oriented node+port+edge dataflow graph — with a straightforward adjacency model (CV2's Structure-of-Arrays
//! packing is a later performance optimization; the behaviour is identical). Two edge kinds: Data wires carry
//! [`Value`](crate::value::Value)s output→input, Exec wires carry execution pulses that fire a node's action.

use crate::value::Value;

/// A node's runtime id (index into [`Graph::nodes`]). CV2 calls this `Id32<NodeId>`; a saved graph's persistent
/// `Id128` is mapped onto these at load time.
pub type NodeId = u32;

/// One endpoint of an edge: a node and one of its ports, by index (CV2 keeps input and output ports in separate id
/// spaces; here the [`EdgeKind`] + direction disambiguates, and `port` indexes that node's inputs or outputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Port {
    pub node: NodeId,
    pub port: u16,
}

impl Port {
    pub fn new(node: NodeId, port: u16) -> Self {
        Port { node, port }
    }
}

/// Whether an edge carries data (a value) or execution (a pulse). CV2's dataflow propagates values along data edges
/// and fires node updates along exec/event edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    Data,
    Exec,
}

/// A directed edge from an output port (`from`) to an input port (`to`).
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub kind: EdgeKind,
    pub from: Port,
    pub to: Port,
}

/// One node instance: its type (by CV2 node-type UUID), its constant config/inlined values, and its port counts.
pub struct Node {
    /// the node-type UUID — resolved against the [`Registry`](crate::registry::Registry) to its behaviour
    pub uuid: String,
    /// per-node constants / inlined literals (CV2 stores these in the node's save data)
    pub config: Vec<Value>,
    pub inputs: u16,
    pub outputs: u16,
}

/// A loaded circuit graph.
#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    /// Adds a node and returns its id.
    pub fn add_node(&mut self, uuid: impl Into<String>, config: Vec<Value>, inputs: u16, outputs: u16) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(Node { uuid: uuid.into(), config, inputs, outputs });
        id
    }

    /// Connects an output port to an input port with an edge of `kind`.
    pub fn connect(&mut self, kind: EdgeKind, from: Port, to: Port) {
        self.edges.push(Edge { kind, from, to });
    }

    /// The output port feeding data input `to`, if any (a data input has at most one source).
    pub fn source_of(&self, to: Port) -> Option<Port> {
        self.edges.iter().find(|e| e.kind == EdgeKind::Data && e.to == to).map(|e| e.from)
    }

    /// The input ports driven by exec output `from` (fan-out).
    pub fn exec_targets(&self, from: Port) -> impl Iterator<Item = Port> + '_ {
        self.edges.iter().filter(move |e| e.kind == EdgeKind::Exec && e.from == from).map(|e| e.to)
    }
}
