use design_search_engine::BeamSearchController;
use search_verification::{
    best_scores_over_iterations, scenario_states, seed_good_experience, update_policy_from_memory,
    verification_config,
};

#[test]
fn experience_feedback_loop_improves_score_trend() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.88);
    }
    update_policy_from_memory(&controller);

    let trajectory = best_scores_over_iterations(&controller, 100, &verification_config(0.2));
    let first_avg = trajectory.iter().take(10).sum::<f64>() / 10.0;
    let last_avg = trajectory.iter().rev().take(10).sum::<f64>() / 10.0;

    assert!(
        last_avg + 1e-9 >= first_avg,
        "first_avg={first_avg}, last_avg={last_avg}"
    );
}
