use concept_engine::{ActivationEngine, ConceptEdge, ConceptGraph, ConceptId, RelationType};

#[test]
fn concept_activation_propagation() {
    let a = ConceptId::from_name("INTENT_QUERY");
    let b = ConceptId::from_name("QUERY_OPTIMIZATION");

    let mut graph = ConceptGraph::default();
    graph.add_edge(ConceptEdge {
        source: a,
        relation: RelationType::Optimizes,
        target: b,
    });

    let engine = ActivationEngine {
        propagation_steps: 2,
        decay: 0.6,
    };

    let scores = engine.run(&graph, &[a]);
    assert!(scores.get(&b).copied().unwrap_or(0.0) > 0.0);
}
