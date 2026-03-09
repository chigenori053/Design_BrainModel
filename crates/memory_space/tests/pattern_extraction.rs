use causal_domain::{CausalGraph, CausalRelationKind};
use design_domain::{Architecture, DesignUnit, Layer};
use memory_space_phase14::{DesignExperience, InMemoryMemorySpace, MemorySpace};

#[test]
fn pattern_extraction_aggregates_repeated_experiences() {
    let mut memory = InMemoryMemorySpace::default();
    let mut graph = CausalGraph::new();
    graph.add_edge(1, 2, CausalRelationKind::Requires);
    graph.add_edge(2, 3, CausalRelationKind::Requires);
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    architecture.add_design_unit(DesignUnit::with_layer(3, "Repository", Layer::Repository));

    for architecture_hash in [100_u64, 101_u64] {
        memory.store_experience(DesignExperience {
            semantic_context: Default::default(),
            inferred_semantics: Default::default(),
            architecture: architecture.clone(),
            architecture_hash,
            causal_graph: graph.clone(),
            dependency_edges: vec![(1, 2), (2, 3)],
            layer_sequence: vec![Layer::Ui, Layer::Service, Layer::Repository],
            score: if architecture_hash == 100 { 0.9 } else { 0.8 },
            search_depth: 3,
        });
    }

    assert_eq!(memory.pattern_store.patterns.len(), 1);
    let pattern = &memory.pattern_store.patterns[0];
    assert_eq!(pattern.frequency, 2);
    assert!((pattern.average_score - 0.85).abs() < 1e-10);
}
