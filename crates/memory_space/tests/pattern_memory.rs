use causal_domain::{CausalGraph, CausalRelationKind};
use design_domain::{Architecture, DesignUnit, Layer};
use memory_space_phase14::{DesignExperience, InMemoryMemorySpace, MemorySpace};
use world_model_core::WorldState;

fn sample_experience(hash: u64, score: f64) -> DesignExperience {
    let mut graph = CausalGraph::new();
    graph.add_edge(1, 2, CausalRelationKind::Requires);
    graph.add_edge(2, 3, CausalRelationKind::Requires);
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    architecture.add_design_unit(DesignUnit::with_layer(3, "Repository", Layer::Repository));
    DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture,
        architecture_hash: hash,
        causal_graph: graph,
        dependency_edges: vec![(1, 2), (2, 3)],
        layer_sequence: vec![Layer::Ui, Layer::Service, Layer::Repository],
        score,
        search_depth: 3,
    }
}

#[test]
fn pattern_memory_aggregates_and_recalls_consistently() {
    let mut memory = InMemoryMemorySpace::default();
    memory.store_experience(sample_experience(100, 0.9));
    memory.store_experience(sample_experience(101, 0.8));

    assert_eq!(memory.pattern_store.patterns.len(), 1);
    let pattern = &memory.pattern_store.patterns[0];
    assert_eq!(pattern.frequency, 2);
    assert!((pattern.average_score - 0.85).abs() < 1e-10);

    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let state = WorldState::from_architecture(1, architecture, Vec::new());
    let patterns = memory.recall_patterns(&state);

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].layer_sequence, vec![Layer::Ui, Layer::Service, Layer::Repository]);
}
