use design_search_engine::{BeamSearchController, SearchController as _};
use memory_space_phase14::architecture_hash;
use search_verification::{
    rest_api_state, seed_good_experience, update_policy_from_memory, verification_config,
};

#[test]
fn determinism_with_policy_keeps_hash_variance_zero() {
    let mut hashes = Vec::new();
    for _ in 0..20 {
        let controller = BeamSearchController::default();
        let state = rest_api_state();
        seed_good_experience(&controller, &state, 0.94);
        update_policy_from_memory(&controller);

        let best = controller
            .search(state, None, &verification_config(0.3))
            .into_iter()
            .max_by(|lhs, rhs| lhs.score.total_cmp(&rhs.score))
            .expect("best state");
        hashes.push(architecture_hash(&best.world_state));
    }

    hashes.dedup();
    assert_eq!(hashes.len(), 1);
}
