use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::node::DesignNode;
use crate::types::NodeId;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructuralGraph {
    nodes: BTreeMap<NodeId, DesignNode>,
    edges: BTreeSet<(NodeId, NodeId)>,
}

impl StructuralGraph {
    pub fn new(nodes: BTreeMap<NodeId, DesignNode>, edges: BTreeSet<(NodeId, NodeId)>) -> Self {
        let graph = Self { nodes, edges };
        assert!(graph.all_edges_have_valid_endpoints());
        assert!(graph.no_self_loops());
        assert!(graph.is_dag());
        graph
    }

    pub fn nodes(&self) -> &BTreeMap<NodeId, DesignNode> {
        &self.nodes
    }

    pub fn edges(&self) -> &BTreeSet<(NodeId, NodeId)> {
        &self.edges
    }

    pub fn with_node_added(&self, node: DesignNode) -> Self {
        if self.nodes.contains_key(&node.id) {
            return self.clone();
        }

        let mut nodes = self.nodes.clone();
        nodes.insert(node.id, node);

        Self {
            nodes,
            edges: self.edges.clone(),
        }
    }

    pub fn with_node_removed(&self, id: NodeId) -> Self {
        if !self.nodes.contains_key(&id) {
            return self.clone();
        }

        let mut nodes = self.nodes.clone();
        nodes.remove(&id);

        let mut edges = self.edges.clone();
        edges.retain(|(from, to)| *from != id && *to != id);

        Self { nodes, edges }
    }

    pub fn with_edge_added(&self, from: NodeId, to: NodeId) -> Self {
        if from == to {
            return self.clone();
        }
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return self.clone();
        }
        if self.edges.contains(&(from, to)) {
            return self.clone();
        }

        let mut edges = self.edges.clone();
        edges.insert((from, to));

        let candidate = Self {
            nodes: self.nodes.clone(),
            edges,
        };

        if candidate.is_dag() {
            candidate
        } else {
            self.clone()
        }
    }

    pub fn with_edge_removed(&self, from: NodeId, to: NodeId) -> Self {
        if !self.edges.contains(&(from, to)) {
            return self.clone();
        }

        let mut edges = self.edges.clone();
        edges.remove(&(from, to));

        Self {
            nodes: self.nodes.clone(),
            edges,
        }
    }

    pub fn is_dag(&self) -> bool {
        if !self.all_edges_have_valid_endpoints() || !self.no_self_loops() {
            return false;
        }

        let mut indegree: BTreeMap<NodeId, usize> =
            self.nodes.keys().copied().map(|id| (id, 0usize)).collect();
        let mut adjacency: BTreeMap<NodeId, Vec<NodeId>> = self
            .nodes
            .keys()
            .copied()
            .map(|id| (id, Vec::new()))
            .collect();

        for (from, to) in &self.edges {
            if let Some(value) = indegree.get_mut(to) {
                *value += 1;
            }
            if let Some(neighbors) = adjacency.get_mut(from) {
                neighbors.push(*to);
            }
        }

        let mut queue: VecDeque<NodeId> = indegree
            .iter()
            .filter_map(|(id, value)| if *value == 0 { Some(*id) } else { None })
            .collect();

        let mut visited = 0usize;

        while let Some(node_id) = queue.pop_front() {
            visited += 1;
            if let Some(neighbors) = adjacency.get(&node_id) {
                for neighbor in neighbors {
                    if let Some(value) = indegree.get_mut(neighbor) {
                        *value -= 1;
                        if *value == 0 {
                            queue.push_back(*neighbor);
                        }
                    }
                }
            }
        }

        visited == self.nodes.len()
    }

    fn all_edges_have_valid_endpoints(&self) -> bool {
        self.edges
            .iter()
            .all(|(from, to)| self.nodes.contains_key(from) && self.nodes.contains_key(to))
    }

    fn no_self_loops(&self) -> bool {
        self.edges.iter().all(|(from, to)| from != to)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::graph::StructuralGraph;
    use crate::node::DesignNode;
    use crate::types::Uuid;

    fn sample_node(id: u128, kind: &str) -> DesignNode {
        DesignNode::with_id(Uuid::from_u128(id), kind, BTreeMap::new())
    }

    #[test]
    fn node_addition_preserves_immutability() {
        let graph = StructuralGraph::default();
        let node = sample_node(1, "Root");

        let next = graph.with_node_added(node.clone());

        assert!(graph.nodes().is_empty());
        assert_eq!(next.nodes().len(), 1);
        assert!(next.nodes().contains_key(&node.id));
    }

    #[test]
    fn edge_addition_rejects_cycles() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");

        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_edge_added(a.id, b.id);

        let next = graph.with_edge_added(b.id, a.id);

        assert_eq!(next, graph);
        assert!(next.is_dag());
        assert!(!next.edges().contains(&(b.id, a.id)));
    }

    #[test]
    fn removing_node_removes_incident_edges() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");

        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_edge_added(a.id, b.id);

        let next = graph.with_node_removed(a.id);

        assert!(!next.nodes().contains_key(&a.id));
        assert!(next.edges().is_empty());
        assert!(next.is_dag());
    }
}
