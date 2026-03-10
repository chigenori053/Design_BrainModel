use design_domain::{Architecture, DesignUnit, Layer};
use design_search_engine::{BeamSearchController, SearchConfig, SearchController as _};
use memory_space_phase14::{DesignExperience, MemorySpace};
use world_model_core::WorldState;

#[test]
#[ignore = "experimental"]
fn experience_prior_biases_search_toward_matching_patterns() {
    let controller = BeamSearchController::default();
    {
        let mut memory = controller.memory.lock().expect("memory lock");
        let mut architecture = Architecture::seeded();
        architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
        architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
        architecture.add_design_unit(DesignUnit::with_layer(3, "Repository", Layer::Repository));
        let state = WorldState::from_architecture(99, architecture, Vec::new());
        memory.store_experience(DesignExperience {
            semantic_context: Default::default(),
            inferred_semantics: Default::default(),
            architecture: state.architecture.clone(),
            architecture_hash: 99,
            causal_graph: state.architecture.causal_graph(),
            dependency_edges: state.architecture.graph.edges.clone(),
            layer_sequence: vec![Layer::Ui, Layer::Service, Layer::Repository],
            score: 0.98,
            search_depth: 3,
        });
    }

    let config = SearchConfig {
        max_depth: 1,
        max_candidates: 8,
        beam_width: 4,
        experience_bias: 0.4,
        policy_bias: 0.15,
    };
    let initial = WorldState::new(1, vec![1.0, 0.0]);

    let states = controller.search(initial, None, &config);

    assert!(!states.is_empty());
    assert!(states.iter().any(|state| state.prior_score > 1.0));
}
