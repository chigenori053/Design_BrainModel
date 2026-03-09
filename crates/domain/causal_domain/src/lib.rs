use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CausalRelationKind {
    Enables,
    Inhibits,
    Requires,
    Emits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CausalRelation {
    pub target: u64,
    pub kind: CausalRelationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CausalEdge {
    pub from: u64,
    pub to: u64,
    pub kind: CausalRelationKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CausalGraph {
    nodes: BTreeSet<u64>,
    edges: Vec<CausalEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalValidation {
    pub valid: bool,
    pub issues: Vec<String>,
}

impl CausalGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: u64) {
        self.nodes.insert(node);
    }

    pub fn add_edge(&mut self, from: u64, to: u64, kind: CausalRelationKind) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.edges.push(CausalEdge { from, to, kind });
    }

    pub fn nodes(&self) -> impl Iterator<Item = &u64> {
        self.nodes.iter()
    }

    pub fn edges(&self) -> &[CausalEdge] {
        &self.edges
    }

    pub fn closure_map(&self) -> BTreeMap<u64, BTreeSet<u64>> {
        self.nodes
            .iter()
            .map(|node| (*node, self.causal_closure(*node)))
            .collect()
    }

    pub fn causal_closure(&self, source: u64) -> BTreeSet<u64> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([source]);

        while let Some(current) = queue.pop_front() {
            for edge in self.edges.iter().filter(|edge| edge.from == current) {
                if visited.insert(edge.to) {
                    queue.push_back(edge.to);
                }
            }
        }

        visited
    }

    pub fn validate(&self) -> CausalValidation {
        let mut issues = Vec::new();

        for edge in &self.edges {
            if edge.from == edge.to {
                issues.push(format!("self causal edge detected at node {}", edge.from));
            }
        }

        for edge in &self.edges {
            if !self.nodes.contains(&edge.from) || !self.nodes.contains(&edge.to) {
                issues.push(format!(
                    "edge {} -> {} references an unknown node",
                    edge.from, edge.to
                ));
            }
        }

        for edge in &self.edges {
            if self.edges.iter().any(|other| {
                edge.from == other.to && edge.to == other.from && edge.kind != other.kind
            }) {
                issues.push(format!(
                    "conflicting causal edges detected between {} and {}",
                    edge.from, edge.to
                ));
            }
        }

        let closure = self.closure_map();
        for node in self.nodes() {
            if closure
                .get(node)
                .map(|reachable| reachable.contains(node))
                .unwrap_or(false)
            {
                issues.push(format!("causal cycle detected at node {}", node));
            }
        }

        issues.sort();
        issues.dedup();

        CausalValidation {
            valid: issues.is_empty(),
            issues,
        }
    }
}

impl Default for CausalValidation {
    fn default() -> Self {
        Self {
            valid: true,
            issues: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_transitive_closure() {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, CausalRelationKind::Requires);
        graph.add_edge(2, 3, CausalRelationKind::Enables);

        let closure = graph.causal_closure(1);

        assert!(closure.contains(&2));
        assert!(closure.contains(&3));
    }

    #[test]
    fn rejects_cycles() {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, CausalRelationKind::Requires);
        graph.add_edge(2, 1, CausalRelationKind::Requires);

        let validation = graph.validate();

        assert!(!validation.valid);
        assert!(
            validation
                .issues
                .iter()
                .any(|issue| issue.contains("causal cycle"))
        );
    }
}
