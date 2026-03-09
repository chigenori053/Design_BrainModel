use design_domain::{Architecture, DesignUnit, Layer};
use memory_space_phase14::{DesignExperience, DesignPattern, PatternId};
use policy_engine::{ActionType, PolicyStore, evaluate_policy};
use world_model_core::WorldState;

#[test]
fn policy_evaluation_returns_action_weights_for_matching_state() {
    let mut policy_store = PolicyStore::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let experience = DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: architecture.clone(),
        architecture_hash: 1,
        causal_graph: architecture.causal_graph(),
        dependency_edges: architecture.graph.edges.clone(),
        layer_sequence: vec![Layer::Ui, Layer::Service],
        score: 0.9,
        search_depth: 2,
    };
    let policy = policy_store.update_policy(&[experience]);
    let pattern = DesignPattern {
        pattern_id: PatternId(1),
        causal_graph: architecture.causal_graph(),
        dependency_edges: architecture.graph.edges.clone(),
        layer_sequence: vec![Layer::Ui, Layer::Service],
        frequency: 1,
        average_score: 0.9,
    };
    let state = WorldState::from_architecture(1, architecture, Vec::new());

    let weights = evaluate_policy(&state, &[pattern], Some(&policy));

    assert!(weights.get(&ActionType::AddUi).copied().unwrap_or(0.0) > 0.0);
    assert!(weights.get(&ActionType::AddService).copied().unwrap_or(0.0) > 0.0);
}
