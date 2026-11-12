//! Loads a CircuitsV2 room-save / built-in template into a raw circuit (nodes + edges + nested subcircuits), by
//! walking the documented protobuf structure (sweet-galileo_cv2-protobuf-schema.md) with the wire reader in [`pb`].
//!
//! Known structure used here:
//! - `CircuitRootData` / `CircuitTemplateRootData`: **field 2** = the circuit/board message.
//! - the circuit/board (`UXWZRHWNBAR`): **field 11** = repeated node (`CircuitNodeData`), **field 5|6** = repeated
//!   edge (`CSRFDGGPJBF`), **field 14** = repeated nested subcircuit (recursive).
//! - a node (`CircuitNodeData`): common id/flag fields, then a protobuf **oneof at field ≥ 102** whose set case is
//!   the node's type-param message (e.g. `ColorConstantNodeData`) — the field number identifies the node type.
//!
//! This gives the node/edge counts and, crucially, the exact set of node types a circuit uses (its oneof field
//! numbers) so the Dodgeball slice can implement just those. The precise field assignments *inside* a node (id /
//! type-UUID / port ids) and the edge endpoints are reversed against a real save next, then wired into [`Graph`].
//!
//! [`pb`]: crate::pb
//! [`Graph`]: crate::graph::Graph

use std::collections::BTreeMap;

use crate::pb::{wire, Reader};

/// The node-type oneof begins at this field number in `CircuitNodeData` (per the schema map).
const NODE_ONEOF_MIN: u32 = 102;

/// A node as read from the save: the oneof field number identifying its type, that type-param message's bytes, and
/// the node's plain varint fields (ids/flags) for later resolution.
pub struct RawNode {
    /// the set oneof field number (≥ 102) — identifies the node type (e.g. a given `*NodeData`)
    pub type_field: u32,
    /// the type-param (`*NodeData`) message bytes, decoded per node type when implemented
    pub param_bytes: Vec<u8>,
    /// the node's scalar (varint) fields, as `(field_number, value)` — ids/flags
    pub common: Vec<(u32, u64)>,
}

/// A circuit/board parsed from the save: its nodes, raw edge messages, and nested subcircuits.
pub struct RawCircuit {
    pub nodes: Vec<RawNode>,
    pub edges: Vec<Vec<u8>>,
    pub subcircuits: Vec<RawCircuit>,
}

/// Parses a `CircuitRootData` / `CircuitTemplateRootData` and returns its circuit (field 2).
pub fn parse_root(bytes: &[u8]) -> Option<RawCircuit> {
    let mut r = Reader::new(bytes);
    while let Some((f, w)) = r.tag() {
        if f == 2 && w == wire::LEN {
            return parse_circuit(r.bytes()?);
        }
        r.skip(w)?;
    }
    None
}

/// Parses a circuit/board message: field 11 nodes, field 5|6 edges, field 14 nested subcircuits.
pub fn parse_circuit(bytes: &[u8]) -> Option<RawCircuit> {
    let (mut nodes, mut edges, mut subcircuits) = (Vec::new(), Vec::new(), Vec::new());
    let mut r = Reader::new(bytes);
    while let Some((f, w)) = r.tag() {
        match (f, w) {
            (11, wire::LEN) => {
                if let Some(n) = parse_node(r.bytes()?) {
                    nodes.push(n);
                }
            }
            (5, wire::LEN) | (6, wire::LEN) => edges.push(r.bytes()?.to_vec()),
            (14, wire::LEN) => {
                if let Some(sc) = parse_circuit(r.bytes()?) {
                    subcircuits.push(sc);
                }
            }
            _ => r.skip(w)?,
        }
    }
    Some(RawCircuit { nodes, edges, subcircuits })
}

/// Parses a `CircuitNodeData`: collects scalar fields and the set oneof type-param (field ≥ 102).
fn parse_node(bytes: &[u8]) -> Option<RawNode> {
    let (mut type_field, mut param_bytes, mut common) = (0, Vec::new(), Vec::new());
    let mut r = Reader::new(bytes);
    while let Some((f, w)) = r.tag() {
        if f >= NODE_ONEOF_MIN && w == wire::LEN {
            type_field = f;
            param_bytes = r.bytes()?.to_vec();
        } else if w == wire::VARINT {
            common.push((f, r.varint()?));
        } else {
            r.skip(w)?;
        }
    }
    Some(RawNode { type_field, param_bytes, common })
}

impl RawCircuit {
    /// Total nodes, including nested subcircuits.
    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.subcircuits.iter().map(RawCircuit::node_count).sum::<usize>()
    }

    /// Total edges, including nested subcircuits.
    pub fn edge_count(&self) -> usize {
        self.edges.len() + self.subcircuits.iter().map(RawCircuit::edge_count).sum::<usize>()
    }

    /// Histogram of node-type oneof field numbers → count (recursive): the set of node types this circuit uses.
    pub fn type_histogram(&self) -> BTreeMap<u32, usize> {
        let mut h = BTreeMap::new();
        self.collect_types(&mut h);
        h
    }

    fn collect_types(&self, h: &mut BTreeMap<u32, usize>) {
        for n in &self.nodes {
            *h.entry(n.type_field).or_default() += 1;
        }
        for s in &self.subcircuits {
            s.collect_types(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pb::{put_len_field, put_varint_field};

    fn node(id: u64, type_field: u32, param: &[u8]) -> Vec<u8> {
        let mut n = Vec::new();
        put_varint_field(&mut n, 6, id); // a common id field
        put_len_field(&mut n, type_field, param); // the oneof type-param
        n
    }

    #[test]
    fn walks_a_circuit() {
        // a circuit with two nodes (types at oneof fields 102 and 103) and one edge, inside a CircuitRootData
        let mut circuit = Vec::new();
        put_len_field(&mut circuit, 11, &node(1, 102, b"\x08\x01")); // node 1, type 102
        put_len_field(&mut circuit, 11, &node(2, 103, b"\x08\x02")); // node 2, type 103
        put_len_field(&mut circuit, 11, &node(3, 102, b"\x08\x03")); // node 3, type 102 again
        put_len_field(&mut circuit, 5, b"edge-bytes"); // one edge
        let mut root = Vec::new();
        put_varint_field(&mut root, 1, 7); // serialization version
        put_len_field(&mut root, 2, &circuit); // the circuit

        let c = parse_root(&root).expect("parse");
        assert_eq!(c.node_count(), 3);
        assert_eq!(c.edge_count(), 1);
        let hist = c.type_histogram();
        assert_eq!(hist.get(&102), Some(&2));
        assert_eq!(hist.get(&103), Some(&1));
        assert_eq!(c.nodes[0].common.iter().find(|(f, _)| *f == 6).map(|(_, v)| *v), Some(1));
    }
}
