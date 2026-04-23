use design_search_engine::BeamSearchController;
use search_verification::{
    action_entropy, run_all_scenarios, scenario_states, seed_good_experience,
    unique_architecture_count, update_policy_from_memory, verification_config,
};

#[test]
fn policy_stability_avoids_collapse() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.9);
    }
    update_policy_from_memory(&controller);

    let states = run_all_scenarios(&controller, &verification_config());

    assert!(unique_architecture_count(&states) >= 19);
    assert!(action_entropy(&states) >= 0.5);
}
