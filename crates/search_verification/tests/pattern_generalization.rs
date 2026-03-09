use design_search_engine::{BeamSearchController, SearchController as _};
use search_verification::{
    max_pattern_ratio, pattern_reuse_rate, scenario_states, seed_good_experience,
    update_policy_from_memory, verification_config,
};

#[test]
fn pattern_generalization_reuses_abstract_patterns() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.95);
    }
    update_policy_from_memory(&controller);

    let mut states = Vec::new();
    for _ in 0..10 {
        for state in scenario_states() {
            states.extend(controller.search(state, None, &verification_config(0.2)));
        }
    }

    assert!(pattern_reuse_rate(&states) >= 0.3);
}

#[test]
fn pattern_frequency_does_not_overfit_single_pattern() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.9);
    }
    update_policy_from_memory(&controller);

    let mut states = Vec::new();
    for index in 0..50 {
        let scenario = scenario_states()[index % 3].clone();
        states.extend(controller.search(scenario, None, &verification_config(0.3)));
    }

    assert!(max_pattern_ratio(&states) <= 0.6);
}
