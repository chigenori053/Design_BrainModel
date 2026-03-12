use design_search_engine::{BeamSearchController, SearchConfig};
use world_model_core::WorldState;

#[test]
fn test7_search_explosion_stability() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 10,
        max_candidates: 16,
        beam_width: 8,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let trace = controller.search_trace(WorldState::new(1, vec![2.0, 1.0]), None, &config);
    let converged = trace
        .depth_best_scores
        .windows(2)
        .all(|pair| pair[1] >= pair[0]);

    println!(
        "Test7 Search Explosion\nmax_depth: {}\nstates_explored: {}\nconverged: {}",
        config.max_depth, trace.explored_state_count, converged
    );

    assert!(!trace.final_beam.is_empty());
    assert!(trace.explored_state_count <= 10_000);
    assert!(converged, "depth_best_scores={:?}", trace.depth_best_scores);
}
