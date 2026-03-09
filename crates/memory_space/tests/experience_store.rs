use causal_domain::{CausalGraph, CausalRelationKind};
use design_domain::{Architecture, Layer};
use memory_space_phase14::{DesignExperience, ExperienceStore};

#[test]
fn experience_store_only_keeps_high_score_architectures() {
    let mut store = ExperienceStore::new(0.7);
    let mut graph = CausalGraph::new();
    graph.add_edge(1, 2, CausalRelationKind::Requires);

    let low = store.update_experience(DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: Architecture::seeded(),
        architecture_hash: 1,
        causal_graph: graph.clone(),
        dependency_edges: vec![(1, 2)],
        layer_sequence: vec![Layer::Service],
        score: 0.4,
        search_depth: 1,
    });
    let high = store.update_experience(DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: Architecture::seeded(),
        architecture_hash: 2,
        causal_graph: graph,
        dependency_edges: vec![(1, 2)],
        layer_sequence: vec![Layer::Service],
        score: 0.9,
        search_depth: 1,
    });

    assert!(!low);
    assert!(high);
    assert_eq!(store.experiences().len(), 1);
    assert_eq!(store.experiences()[0].architecture_hash, 2);
}
