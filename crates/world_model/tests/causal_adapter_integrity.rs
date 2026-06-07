use design_domain::{Architecture, DesignUnit};
use world_model::WorldStateToCausalAdapter;
use world_model_core::WorldState;

#[test]
fn architecture_design_unit_count_is_preserved() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::new(1, "Runtime"));
    architecture.add_design_unit(DesignUnit::new(2, "Policy"));
    architecture.add_design_unit(DesignUnit::new(3, "Memory"));
    let state = WorldState::from_architecture(1, architecture, Vec::new());

    let converted = WorldStateToCausalAdapter::convert(&state);

    assert_eq!(
        state.architecture.design_unit_count(),
        converted.entities.len()
    );
}
