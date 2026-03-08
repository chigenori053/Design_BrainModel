use design_search_engine::{BeamSearchController, SearchConfig, SearchController as _};
use world_model_core::WorldState;

#[test]
fn search_only_returns_grammar_valid_candidates() {
    let controller = BeamSearchController;
    let config = SearchConfig {
        max_depth: 2,
        max_candidates: 8,
        beam_width: 4,
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
    assert!(states
        .iter()
        .all(|state| state.world_state.simulation.is_some()));
}
