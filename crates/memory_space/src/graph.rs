use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::node::DesignNode;
use crate::types::{NodeId, Value};

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

    pub fn average_clustering_coefficient(&self) -> f64 {
        let n = self.nodes.len();
        if n < 3 {
            return 0.0;
        }
        let neighbors = self.undirected_neighbors();
        let mut sum = 0.0;
        for node_id in self.nodes.keys() {
            let Some(adj) = neighbors.get(node_id) else {
                continue;
            };
            let k = adj.len();
            if k < 2 {
                continue;
            }
            let mut links = 0usize;
            let adj_vec: Vec<NodeId> = adj.iter().copied().collect();
            for i in 0..adj_vec.len() {
                for j in (i + 1)..adj_vec.len() {
                    let a = adj_vec[i];
                    let b = adj_vec[j];
                    if self.edges.contains(&(a, b)) || self.edges.contains(&(b, a)) {
                        links += 1;
                    }
                }
            }
            let denom = (k * (k - 1)) as f64;
            let c_i = (2.0 * links as f64) / denom;
            sum += c_i;
        }
        (sum / n as f64).clamp(0.0, 1.0)
    }

    pub fn category_counts(&self) -> BTreeMap<String, usize> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for node in self.nodes.values() {
            let Some(Value::Text(category)) = node.attributes.get("category") else {
                continue;
            };
            let trimmed = category.trim();
            if trimmed.is_empty() {
                continue;
            }
            *counts.entry(trimmed.to_string()).or_insert(0) += 1;
        }
        counts
    }

    pub fn normalized_category_entropy(&self) -> Option<f64> {
        let counts = self.category_counts();
        if counts.is_empty() {
            return None;
        }
        let total = counts.values().sum::<usize>() as f64;
        if total <= 0.0 {
            return Some(0.0);
        }
        let mut entropy = 0.0;
        for count in counts.values().copied() {
            let p = count as f64 / total;
            if p > 0.0 {
                entropy -= p * p.ln();
            }
        }
        let k = counts.len() as f64;
        if k <= 1.0 {
            Some(0.0)
        } else {
            Some((entropy / k.ln()).clamp(0.0, 1.0))
        }
    }

    pub fn normalized_degree_entropy(&self) -> f64 {
        let n = self.nodes.len();
        if n < 2 {
            return 0.0;
        }
        let neighbors = self.undirected_neighbors();
        let mut degree_counts: BTreeMap<usize, usize> = BTreeMap::new();
        for node_id in self.nodes.keys() {
            let degree = neighbors.get(node_id).map(|set| set.len()).unwrap_or(0);
            *degree_counts.entry(degree).or_insert(0) += 1;
        }
        let m = degree_counts.len() as f64;
        if m <= 1.0 {
            return 0.0;
        }
        let total = n as f64;
        let mut entropy = 0.0;
        for count in degree_counts.values().copied() {
            let p = count as f64 / total;
            if p > 0.0 {
                entropy -= p * p.ln();
            }
        }
        (entropy / m.ln()).clamp(0.0, 1.0)
    }

    pub fn normalized_degree_mass_entropy(&self) -> f64 {
        let n = self.nodes.len();
        if n < 2 {
            return 0.0;
        }
        let neighbors = self.undirected_neighbors();
        let mut degrees = Vec::with_capacity(n);
        for node_id in self.nodes.keys() {
            let d = neighbors.get(node_id).map(|set| set.len()).unwrap_or(0) as f64;
            degrees.push(d);
        }
        let total_mass = degrees.iter().sum::<f64>();
        if total_mass <= 1e-12 {
            return 0.0;
        }
        let mut entropy = 0.0;
        for d in degrees {
            let p = d / total_mass;
            if p > 0.0 {
                entropy -= p * p.ln();
            }
        }
        (entropy / (n as f64).ln()).clamp(0.0, 1.0)
    }

    pub fn normalized_degree_gini(&self) -> f64 {
        let n = self.nodes.len();
        if n < 2 {
            return 0.0;
        }
        let neighbors = self.undirected_neighbors();
        let mut degrees = Vec::with_capacity(n);
        for node_id in self.nodes.keys() {
            degrees.push(neighbors.get(node_id).map(|set| set.len()).unwrap_or(0) as f64);
        }
        let total = degrees.iter().sum::<f64>();
        if total <= 1e-12 {
            return 0.0;
        }
        degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n_f = n as f64;
        let mut weighted_sum = 0.0;
        for (idx, value) in degrees.iter().enumerate() {
            let rank = (idx + 1) as f64;
            weighted_sum += (2.0 * rank - n_f - 1.0) * *value;
        }
        (weighted_sum / (n_f * total)).clamp(0.0, 1.0)
    }

    pub fn normalized_max_degree(&self) -> f64 {
        let n = self.nodes.len();
        if n < 2 {
            return 0.0;
        }
        let neighbors = self.undirected_neighbors();
        let max_degree = self
            .nodes
            .keys()
            .map(|node_id| neighbors.get(node_id).map(|set| set.len()).unwrap_or(0))
            .max()
            .unwrap_or(0);
        (max_degree as f64 / (n - 1) as f64).clamp(0.0, 1.0)
    }

    pub fn normalized_degree_variance(&self) -> f64 {
        let n = self.nodes.len();
        if n < 3 {
            return 0.0;
        }
        let neighbors = self.undirected_neighbors();
        let mut degrees = Vec::with_capacity(n);
        for node_id in self.nodes.keys() {
            let d = neighbors.get(node_id).map(|s| s.len()).unwrap_or(0);
            degrees.push(d as f64);
        }
        let mean = degrees.iter().sum::<f64>() / n as f64;
        let var = degrees
            .iter()
            .map(|d| {
                let x = *d - mean;
                x * x
            })
            .sum::<f64>()
            / n as f64;
        let max_var = max_degree_variance_for_simple_graph(n);
        if max_var <= 1e-12 {
            return 0.0;
        }
        (var / max_var).clamp(0.0, 1.0)
    }

    fn all_edges_have_valid_endpoints(&self) -> bool {
        self.edges
            .iter()
            .all(|(from, to)| self.nodes.contains_key(from) && self.nodes.contains_key(to))
    }

    fn no_self_loops(&self) -> bool {
        self.edges.iter().all(|(from, to)| from != to)
    }

    fn undirected_neighbors(&self) -> BTreeMap<NodeId, BTreeSet<NodeId>> {
        let mut neighbors: BTreeMap<NodeId, BTreeSet<NodeId>> = self
            .nodes
            .keys()
            .copied()
            .map(|id| (id, BTreeSet::new()))
            .collect();
        for (from, to) in &self.edges {
            if let Some(s) = neighbors.get_mut(from) {
                s.insert(*to);
            }
            if let Some(s) = neighbors.get_mut(to) {
                s.insert(*from);
            }
        }
        neighbors
    }
}

fn max_degree_variance_for_simple_graph(n: usize) -> f64 {
    if n < 3 {
        return 0.0;
    }
    let mut degrees = vec![1.0f64; n];
    degrees[0] = (n - 1) as f64;
    let mean = degrees.iter().sum::<f64>() / n as f64;
    degrees
        .iter()
        .map(|d| {
            let x = *d - mean;
            x * x
        })
        .sum::<f64>()
        / n as f64
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

    #[test]
    fn clustering_is_zero_for_chain_dag() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");
        let c = sample_node(3, "C");
        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_node_added(c.clone())
            .with_edge_added(a.id, b.id)
            .with_edge_added(b.id, c.id);
        assert_eq!(graph.average_clustering_coefficient(), 0.0);
    }

    #[test]
    fn normalized_degree_variance_in_range() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");
        let c = sample_node(3, "C");
        let d = sample_node(4, "D");
        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_node_added(c.clone())
            .with_node_added(d.clone())
            .with_edge_added(a.id, b.id)
            .with_edge_added(a.id, c.id)
            .with_edge_added(a.id, d.id);
        let v = graph.normalized_degree_variance();
        assert!((0.0..=1.0).contains(&v));
    }

    #[test]
    fn category_entropy_none_when_category_absent() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");
        let graph = StructuralGraph::default()
            .with_node_added(a)
            .with_node_added(b);
        assert_eq!(graph.normalized_category_entropy(), None);
    }

    #[test]
    fn category_entropy_is_normalized() {
        let mut a = sample_node(1, "A");
        a.attributes
            .insert("category".to_string(), crate::types::Value::Text("X".to_string()));
        let mut b = sample_node(2, "B");
        b.attributes
            .insert("category".to_string(), crate::types::Value::Text("X".to_string()));
        let mut c = sample_node(3, "C");
        c.attributes
            .insert("category".to_string(), crate::types::Value::Text("Y".to_string()));
        let mut d = sample_node(4, "D");
        d.attributes
            .insert("category".to_string(), crate::types::Value::Text("Y".to_string()));
        let graph = StructuralGraph::default()
            .with_node_added(a)
            .with_node_added(b)
            .with_node_added(c)
            .with_node_added(d);
        assert_eq!(graph.normalized_category_entropy(), Some(1.0));
    }

    #[test]
    fn degree_entropy_in_range() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");
        let c = sample_node(3, "C");
        let d = sample_node(4, "D");
        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_node_added(c.clone())
            .with_node_added(d.clone())
            .with_edge_added(a.id, b.id)
            .with_edge_added(b.id, c.id)
            .with_edge_added(c.id, d.id);
        let v = graph.normalized_degree_entropy();
        assert!((0.0..=1.0).contains(&v));
    }

    #[test]
    fn degree_mass_entropy_in_range() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");
        let c = sample_node(3, "C");
        let d = sample_node(4, "D");
        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_node_added(c.clone())
            .with_node_added(d.clone())
            .with_edge_added(a.id, b.id)
            .with_edge_added(a.id, c.id)
            .with_edge_added(a.id, d.id);
        let v = graph.normalized_degree_mass_entropy();
        assert!((0.0..=1.0).contains(&v));
    }

    #[test]
    fn degree_gini_in_range() {
        let a = sample_node(1, "A");
        let b = sample_node(2, "B");
        let c = sample_node(3, "C");
        let d = sample_node(4, "D");
        let graph = StructuralGraph::default()
            .with_node_added(a.clone())
            .with_node_added(b.clone())
            .with_node_added(c.clone())
            .with_node_added(d.clone())
            .with_edge_added(a.id, b.id)
            .with_edge_added(a.id, c.id)
            .with_edge_added(a.id, d.id);
        let v = graph.normalized_degree_gini();
        assert!((0.0..=1.0).contains(&v));
    }
}
