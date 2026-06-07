use design_domain::{Architecture, Constraint, DesignUnit, Layer};
use world_model::WorldStateToCausalAdapter;
use world_model_core::{Action, WorldState};

fn sample_state() -> WorldState {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Runtime", Layer::Service));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Memory", Layer::Repository));

    let mut state = WorldState::from_architecture(
        7,
        architecture,
        vec![Constraint {
            name: "bounded".to_string(),
            max_design_units: Some(4),
            max_dependencies: Some(3),
        }],
    );
    state.history.push(Action::SplitStructure);
    state
}

#[test]
fn conversion_creates_causal_entities() {
    let converted = WorldStateToCausalAdapter::convert(&sample_state());

    assert_eq!(converted.entities.len(), 2);
    assert!(
        converted
            .entities
            .iter()
            .all(|entity| !entity.metadata.is_empty())
    );
    assert_eq!(converted.causal_state.seed_history, ["split_structure"]);
}

#[test]
fn conversion_is_deterministic() {
    let state = sample_state();

    assert_eq!(
        WorldStateToCausalAdapter::convert(&state),
        WorldStateToCausalAdapter::convert(&state)
    );
}

#[test]
fn signature_is_stable_for_identical_input() {
    let state = sample_state();
    let left = WorldStateToCausalAdapter::convert(&state);
    let right = WorldStateToCausalAdapter::convert(&state);

    assert!(!left.world_signature.is_empty());
    assert_eq!(left.world_signature, right.world_signature);
}

#[test]
fn non_empty_architecture_produces_entities() {
    let converted = WorldStateToCausalAdapter::convert(&sample_state());

    assert!(!converted.entities.is_empty());
}

#[test]
fn constraints_are_propagated_without_loss() {
    let state = sample_state();
    let converted = WorldStateToCausalAdapter::convert(&state);

    assert_eq!(converted.environmental_constraints, state.constraints);
}
