use crate::stable_v03::{ArchitectureGraph, Edge, Node, NodeId, NodeType};

pub struct ArchitectureQuery<'a> {
    graph: &'a ArchitectureGraph,
}

impl<'a> ArchitectureQuery<'a> {
    pub fn new(graph: &'a ArchitectureGraph) -> Self {
        Self { graph }
    }

    pub fn nodes_by_type(&self, node_type: &NodeType) -> Vec<&'a Node> {
        self.graph.find_by_type(node_type.clone())
    }

    pub fn outgoing_edges(&self, node_id: &NodeId) -> Vec<&'a Edge> {
        self.graph.outgoing(node_id.clone())
    }
}
