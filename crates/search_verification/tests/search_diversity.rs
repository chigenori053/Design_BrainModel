use design_search_engine::{BeamSearchController, SearchController as _};
use memory_space_phase14::architecture_hash;
use search_verification::{
    average_similarity, scenario_states, seed_good_experience, update_policy_from_memory,
    verification_config,
};

#[test]
fn search_diversity_stays_below_similarity_threshold() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.92);
    }
    update_policy_from_memory(&controller);

    let mut representative_states = Vec::new();
    for policy_bias in [0.0, 0.2, 0.5] {
        for state in scenario_states() {
            let best = controller
                .search(state, None, &verification_config(policy_bias))
                .into_iter()
                .max_by(|lhs, rhs| lhs.score.total_cmp(&rhs.score))
                .expect("representative state");
            representative_states.push(best);
        }
    }
    let mut by_hash = std::collections::BTreeMap::new();
    for state in representative_states {
        by_hash
            .entry(architecture_hash(&state.world_state))
            .or_insert(state);
    }
    let similarity = average_similarity(&by_hash.into_values().collect::<Vec<_>>());

    assert!(similarity < 0.7, "average_similarity={similarity}");
    assert!(similarity < 0.9, "collapse similarity={similarity}");
}
