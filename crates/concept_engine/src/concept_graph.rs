use std::collections::HashSet;

use crate::concept::ConceptId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationType {
    DependsOn,
    Optimizes,
    ConflictsWith,
    PartOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConceptEdge {
    pub source: ConceptId,
    pub relation: RelationType,
    pub target: ConceptId,
}

#[derive(Clone, Debug, Default)]
pub struct ConceptGraph {
    edges: Vec<ConceptEdge>,
}

impl ConceptGraph {
    pub fn add_edge(&mut self, edge: ConceptEdge) {
        self.edges.push(edge);
    }

    pub fn edges(&self) -> &[ConceptEdge] {
        &self.edges
    }

    pub fn validate_integrity(&self, known_concepts: &HashSet<ConceptId>) -> bool {
        self.edges
            .iter()
            .all(|e| known_concepts.contains(&e.source) && known_concepts.contains(&e.target))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::concept::ConceptId;

    use super::{ConceptEdge, ConceptGraph, RelationType};

    #[test]
    fn graph_integrity_requires_known_nodes() {
        let mut graph = ConceptGraph::default();
        let a = ConceptId::from_name("database");
        let b = ConceptId::from_name("query_optimization");

        graph.add_edge(ConceptEdge {
            source: a,
            relation: RelationType::Optimizes,
            target: b,
        });

        let known = [a, b].into_iter().collect::<HashSet<_>>();
        assert!(graph.validate_integrity(&known));
    }
}
