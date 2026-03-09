use causal_domain::{CausalGraph, CausalRelationKind};
use design_domain::{Architecture, DesignUnit, Layer};
use memory_space_phase14::{DesignExperience, InMemoryMemorySpace, MemorySpace};
use world_model_core::WorldState;

#[test]
fn pattern_matching_recalls_matching_patterns() {
    let mut memory = InMemoryMemorySpace::default();
    let mut graph = CausalGraph::new();
    graph.add_edge(1, 2, CausalRelationKind::Requires);
    graph.add_edge(2, 3, CausalRelationKind::Requires);
    memory.store_experience(DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: Architecture::seeded(),
        architecture_hash: 7,
        causal_graph: graph,
        dependency_edges: vec![(1, 2), (2, 3)],
        layer_sequence: vec![Layer::Ui, Layer::Service, Layer::Repository],
        score: 0.92,
        search_depth: 3,
    });

    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let state = WorldState::from_architecture(1, architecture, Vec::new());

    let patterns = memory.recall_patterns(&state);

    assert_eq!(patterns.len(), 1);
    assert_eq!(
        patterns[0].layer_sequence,
        vec![Layer::Ui, Layer::Service, Layer::Repository]
    );
}
