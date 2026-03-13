use design_domain::{Architecture, DesignUnit, Layer};
use world_model_core::{Action, WorldState};

#[test]
fn phase29_add_design_unit_transition_updates_state_consistently() {
    let state = WorldState::from_architecture(1, Architecture::seeded(), Vec::new());
    let next = state.apply_action(
        &Action::AddDesignUnit {
            name: "RuntimeGateway".into(),
            layer: Layer::Service,
        },
        2,
    );

    assert_eq!(next.state_id, 2);
    assert_eq!(next.depth, 1);
    assert_eq!(next.history, vec![Action::AddDesignUnit {
        name: "RuntimeGateway".into(),
        layer: Layer::Service,
    }]);
    assert_eq!(next.architecture.design_unit_count(), 1);
    assert!(next.simulation.is_none());
}

#[test]
fn phase29_dependency_transition_builds_expected_graph_edge() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::new(1, "ServiceA"));
    architecture.add_design_unit(DesignUnit::new(2, "DatabaseB"));
    let state = WorldState::from_architecture(10, architecture, Vec::new());

    let next = state.apply_action(&Action::ConnectDependency { from: 1, to: 2 }, 11);

    assert_eq!(next.depth, 1);
    assert_eq!(next.architecture.dependencies.len(), 1);
    assert_eq!(next.architecture.graph.edges, vec![(1, 2)]);
    assert_eq!(next.features[1], 1.0);
}
