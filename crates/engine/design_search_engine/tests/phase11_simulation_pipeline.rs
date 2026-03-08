use design_search_engine::{BeamSearchController, SearchConfig, SearchController as _};
use memory_space_core::{RecallCandidate, RecallResult};
use world_model_core::WorldState;

#[test]
fn search_pipeline_populates_simulation_before_scoring() {
    let controller = BeamSearchController;
    let config = SearchConfig {
        max_depth: 1,
        max_candidates: 8,
        beam_width: 4,
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
    assert!(states.iter().all(|state| state.world_state.simulation.is_some()));
    assert!(states
        .iter()
        .all(|state| state.world_state.evaluation.simulation_quality > 0.0));
}
