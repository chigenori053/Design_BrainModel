use causal_domain::{CausalGraph, CausalRelationKind};
use design_domain::{Architecture, DesignUnit, Layer};
use memory_space_phase14::{DesignExperience, DesignPattern, PatternId};
use policy_engine::{ActionType, PolicyStore, evaluate_policy, policy_weight_for_action};
use world_model_core::{Action, WorldState};

#[test]
fn phase15_policy_guided_search_applies_policy_score() {
    let mut policy_store = PolicyStore::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let experience = DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: architecture.clone(),
        architecture_hash: 55,
        causal_graph: architecture.causal_graph(),
        dependency_edges: architecture.graph.edges.clone(),
        layer_sequence: vec![Layer::Ui, Layer::Service],
        score: 0.95,
        search_depth: 2,
    };
    let policy = policy_store.update_policy(&[experience]);
    let mut pattern_graph = CausalGraph::new();
    pattern_graph.add_edge(1, 2, CausalRelationKind::Requires);
    let pattern = DesignPattern {
        pattern_id: PatternId(1),
        causal_graph: pattern_graph,
        dependency_edges: vec![(1, 2)],
        layer_sequence: vec![Layer::Ui, Layer::Service],
        frequency: 1,
        average_score: 0.95,
    };
    let state = WorldState::from_architecture(1, architecture, Vec::new());

    let weights = evaluate_policy(&state, &[pattern], Some(&policy));
    let action = Action::AddDesignUnit {
        name: "Controller".into(),
        layer: Layer::Ui,
    };

    assert!(weights.get(&ActionType::AddUi).copied().unwrap_or(0.0) > 0.0);
    assert!(policy_weight_for_action(&action, &weights) > 0.0);
}
