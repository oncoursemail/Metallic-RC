//! The node-type registry: CV2 resolves a node's type-UUID to its implementation (`CircuitNodeType.uuid -> .NET
//! Type`); here a UUID maps to a [`NodeType`] impl. The built-in logic/math/compare nodes are registered from their
//! unambiguous semantics; game-specific node types are added as their decompiled behaviour is implemented.

use std::collections::HashMap;

use crate::node::NodeType;

/// Maps node-type UUID → behaviour.
#[derive(Default)]
pub struct Registry {
    types: HashMap<String, Box<dyn NodeType>>,
}

impl Registry {
    pub fn register(&mut self, t: Box<dyn NodeType>) {
        self.types.insert(t.uuid().to_string(), t);
    }

    pub fn get(&self, uuid: &str) -> Option<&dyn NodeType> {
        self.types.get(uuid).map(|b| b.as_ref())
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// A registry with the built-in logic/math/compare nodes registered.
    pub fn with_builtins() -> Registry {
        let mut r = Registry::default();
        crate::nodes::register_builtins(&mut r);
        r
    }
}
