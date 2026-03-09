use causal_domain::{CausalGraph, CausalRelationKind};
use design_domain::{Architecture, DesignUnit, Layer};
use memory_space_phase14::DesignExperience;
use policy_engine::{ActionType, PolicyStore};

#[test]
fn policy_update_learns_quantized_action_weights() {
    let mut store = PolicyStore::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let mut graph = CausalGraph::new();
    graph.add_edge(1, 2, CausalRelationKind::Requires);

    let policy = store.update_policy(&[DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture,
        architecture_hash: 42,
        causal_graph: graph,
        dependency_edges: vec![(1, 2)],
        layer_sequence: vec![Layer::Ui, Layer::Service],
        score: 0.93,
        search_depth: 2,
    }]);

    assert!(policy.action_weights.contains_key(&ActionType::AddUi));
    assert!(policy.action_weights.contains_key(&ActionType::AddService));
    assert!(
        policy
            .action_weights
            .contains_key(&ActionType::ConnectDependency)
    );
}
