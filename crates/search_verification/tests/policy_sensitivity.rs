use design_search_engine::BeamSearchController;
use search_verification::{
    run_all_scenarios, scenario_states, score_variance, seed_good_experience,
    update_policy_from_memory, verification_config,
};

#[test]
fn policy_sensitivity_stays_in_stable_variance_range() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.91);
    }
    update_policy_from_memory(&controller);

    let mut scores = Vec::new();
    for step in 0..=5 {
        let bias = step as f64 * 0.1;
        let mean = run_all_scenarios(&controller, &verification_config(bias))
            .iter()
            .map(|state| state.score)
            .sum::<f64>()
            / 96.0;
        scores.push(mean);
    }

    assert!(score_variance(&scores) < 0.05);
}
