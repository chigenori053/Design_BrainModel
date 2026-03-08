use design_domain::{Architecture, Dependency, DependencyKind, DesignUnit, DesignUnitId, Layer};
use design_grammar::GrammarEngine;
use world_model_core::WorldState;

#[test]
fn grammar_rejects_reverse_layer_dependency() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "ControllerUnit", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "DatabaseUnit", Layer::Database));
    architecture.dependencies.push(Dependency {
        from: DesignUnitId(2),
        to: DesignUnitId(1),
        kind: DependencyKind::Calls,
    });
    let state = WorldState::from_architecture(1, architecture, Vec::new());

    let validation = GrammarEngine::default().validate_world_state(&state);

    assert!(!validation.valid);
}

#[test]
fn grammar_accepts_layered_dependency_flow() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "ControllerUnit", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "ServiceUnit", Layer::Service));
    architecture.dependencies.push(Dependency {
        from: DesignUnitId(1),
        to: DesignUnitId(2),
        kind: DependencyKind::Calls,
    });
    let state = WorldState::from_architecture(1, architecture, Vec::new());

    let validation = GrammarEngine::default().validate_world_state(&state);

    assert!(validation.valid);
}
