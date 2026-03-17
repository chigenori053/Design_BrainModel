use std::collections::{BTreeMap, BTreeSet};

use crate::stable_v03::{ArchitectureGraph, Edge, Node, NodeId, ValidationResult};

#[derive(Clone, Debug, Default)]
pub struct ArchitectureGraphBuilder {
    nodes: BTreeMap<NodeId, Node>,
    edges: BTreeSet<Edge>,
}

impl ArchitectureGraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(mut self, node: Node) -> Self {
        self.nodes.insert(node.id.clone(), node);
        self
    }

    pub fn add_edge(mut self, edge: Edge) -> Self {
        self.edges.insert(edge);
        self
    }

    pub fn build(self) -> Result<ArchitectureGraph, ValidationResult> {
        let graph = ArchitectureGraph::new(
            self.nodes.into_values().collect(),
            self.edges.into_iter().collect(),
        );
        let validation = graph.validate();
        if validation.is_valid() {
            Ok(graph)
        } else {
            Err(validation)
        }
    }
}
