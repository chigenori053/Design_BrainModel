use design_search_engine::{BeamSearchController, SearchConfig, SearchController as _};
use memory_space_core::{RecallCandidate, RecallResult};
use world_model_core::WorldState;

#[test]
fn search_pipeline_populates_simulation_before_scoring() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 1,
        max_candidates: 8,
        beam_width: 4,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let initial = WorldState::new(1, vec![2.0, 1.0]);
    let recall = RecallResult {
        candidates: vec![RecallCandidate {
            memory_id: 9,
            feature_vector: vec![2.0, 1.0],
            relevance_score: 0.9,
        }],
    };

    let states = controller.search(initial, Some(&recall), &config);

    assert!(!states.is_empty());
    assert!(
        states
            .iter()
            .all(|state| state.world_state.simulation.is_some())
    );
    assert!(
        states
            .iter()
            .all(|state| state.world_state.evaluation.simulation_quality > 0.0)
    );
}

#[test]
fn search_only_returns_grammar_valid_candidates() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 2,
        max_candidates: 8,
        beam_width: 4,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };

    let states = controller.search(WorldState::new(1, vec![1.0, 0.0]), None, &config);

    assert!(!states.is_empty());
    assert!(states.iter().all(|state| {
        state
            .grammar_validation
            .as_ref()
            .map(|validation| validation.valid)
            .unwrap_or(false)
    }));
    assert!(
        states
            .iter()
            .all(|state| state.world_state.simulation.is_some())
    );
}
