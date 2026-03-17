use std::collections::BTreeMap;
use std::sync::Arc;

use crate::stable_v03::{Edge, Node, NodeId, NodeType, ValidationResult};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArchitectureGraph {
    nodes: Arc<Vec<Node>>,
    edges: Arc<Vec<Edge>>,
    node_index: Arc<BTreeMap<NodeId, usize>>,
    outgoing_index: Arc<BTreeMap<NodeId, Vec<usize>>>,
    incoming_index: Arc<BTreeMap<NodeId, Vec<usize>>>,
    type_index: Arc<BTreeMap<NodeType, Vec<usize>>>,
}

impl Default for ArchitectureGraph {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new())
    }
}

impl ArchitectureGraph {
    pub fn new(nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        let mut node_index = BTreeMap::new();
        let mut outgoing_index = BTreeMap::new();
        let mut incoming_index = BTreeMap::new();
        let mut type_index = BTreeMap::new();

        for (index, node) in nodes.iter().enumerate() {
            node_index.insert(node.id.clone(), index);
            type_index
                .entry(node.node_type.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }
        for (index, edge) in edges.iter().enumerate() {
            outgoing_index
                .entry(edge.source.clone())
                .or_insert_with(Vec::new)
                .push(index);
            incoming_index
                .entry(edge.target.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }

        Self {
            nodes: Arc::new(nodes),
            edges: Arc::new(edges),
            node_index: Arc::new(node_index),
            outgoing_index: Arc::new(outgoing_index),
            incoming_index: Arc::new(incoming_index),
            type_index: Arc::new(type_index),
        }
    }

    pub fn nodes(&self) -> &[Node] {
        self.nodes.as_slice()
    }

    pub fn edges(&self) -> &[Edge] {
        self.edges.as_slice()
    }

    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.node_index
            .get(id)
            .and_then(|index| self.nodes.get(*index))
    }

    pub fn neighbors(&self, node_id: NodeId) -> Vec<NodeId> {
        self.outgoing_index
            .get(&node_id)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter_map(|index| self.edges.get(*index))
            .map(|edge| edge.target.clone())
            .collect()
    }

    pub fn find_by_type(&self, node_type: NodeType) -> Vec<&Node> {
        self.type_index
            .get(&node_type)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter_map(|index| self.nodes.get(*index))
            .collect()
    }

    pub fn outgoing(&self, node_id: NodeId) -> Vec<&Edge> {
        self.outgoing_index
            .get(&node_id)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter_map(|index| self.edges.get(*index))
            .collect()
    }

    pub fn incoming(&self, node_id: NodeId) -> Vec<&Edge> {
        self.incoming_index
            .get(&node_id)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter_map(|index| self.edges.get(*index))
            .collect()
    }

    pub fn validate(&self) -> ValidationResult {
        crate::stable_v03::validation::validate(self)
    }

    pub fn with_node(&self, node: Node) -> Self {
        let mut nodes = self.nodes().to_vec();
        nodes.push(node);
        Self::new(nodes, self.edges().to_vec())
    }

    pub fn with_edge(&self, edge: Edge) -> Self {
        let mut edges = self.edges().to_vec();
        edges.push(edge);
        Self::new(self.nodes().to_vec(), edges)
    }
}
