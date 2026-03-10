use architecture_domain::ArchitectureState;
use design_domain::{Architecture, Constraint, DesignUnit, Layer};

#[test]
fn architecture_state_tracks_component_transitions() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let baseline = ArchitectureState::from_architecture(
        &architecture,
        vec![Constraint {
            name: "api".to_string(),
            max_design_units: Some(16),
            max_dependencies: Some(24),
        }],
    );

    architecture.add_design_unit(DesignUnit::with_layer(3, "Repository", Layer::Repository));
    let expanded =
        ArchitectureState::from_architecture(&architecture, baseline.constraints.clone());

    assert_eq!(baseline.metrics.component_count, 2);
    assert_eq!(expanded.metrics.component_count, 3);
    assert!(expanded.deployment.replicas >= baseline.deployment.replicas);
}
