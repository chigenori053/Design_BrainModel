use design_search_engine::{
    rank_candidates, BeamSearchController, SearchConfig, SearchController as _, SearchState,
};
use memory_space_core::{RecallCandidate, RecallResult};
use world_model_core::WorldState;

#[test]
fn recall_first_uses_memory_seed_when_confidence_is_high() {
    let controller = BeamSearchController;
    let config = SearchConfig {
        max_depth: 0,
        max_candidates: 8,
        beam_width: 4,
    };
    let initial = WorldState::new(1, vec![0.0, 0.0]);
    let recall = RecallResult {
        candidates: vec![RecallCandidate {
            memory_id: 1,
            feature_vector: vec![3.0, 1.0],
            relevance_score: 0.95,
        }],
    };

    let states = controller.search(initial, Some(&recall), &config);

    assert_eq!(states.len(), 1);
    assert_eq!(states[0].world_state.architecture.design_unit_count(), 3);
    assert_eq!(states[0].world_state.architecture.dependencies.len(), 1);
    assert!(states[0].world_state.simulation.is_some());
    assert!(states[0].world_state.evaluation.simulation_quality > 0.0);
}

#[test]
fn pareto_ranking_prefers_less_dominated_candidate() {
    let strong = SearchState::new(1, WorldState::new(1, vec![3.0, 1.0]));
    let weak = SearchState::new(2, WorldState::new(2, vec![1.0, 0.0]));

    let ranked = rank_candidates(vec![weak, strong]);

    assert_eq!(ranked[0].state.state_id, 1);
    assert!(ranked[0].pareto_rank <= ranked[1].pareto_rank);
}
