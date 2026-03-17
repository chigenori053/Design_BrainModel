use std::collections::{BTreeMap, BTreeSet};

use crate::{DesignEdge, DesignGraph, DesignNode, DesignNodeId};

#[derive(Clone, Debug, Default)]
pub struct DesignGraphBuilder {
    nodes: BTreeMap<DesignNodeId, DesignNode>,
    edges: BTreeSet<DesignEdge>,
}

impl DesignGraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(mut self, node: DesignNode) -> Self {
        self.nodes.insert(node.id.clone(), node);
        self
    }

    pub fn add_edge(mut self, edge: DesignEdge) -> Self {
        self.edges.insert(edge);
        self
    }

    pub fn build(self) -> DesignGraph {
        DesignGraph::new(
            self.nodes.into_values().collect(),
            self.edges.into_iter().collect(),
        )
    }
}
