use std::collections::HashSet;

use concept_engine::{ConceptEdge, ConceptGraph, ConceptId, RelationType};

#[test]
fn concept_graph_relation() {
    let mut graph = ConceptGraph::default();
    let source = ConceptId::from_name("QUERY_OPTIMIZATION");
    let target = ConceptId::from_name("DATABASE_INDEX");

    graph.add_edge(ConceptEdge {
        source,
        relation: RelationType::Optimizes,
        target,
    });

    let known = [source, target].into_iter().collect::<HashSet<_>>();

    assert_eq!(graph.edges().len(), 1);
    assert!(graph.validate_integrity(&known));
}
