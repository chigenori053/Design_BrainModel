use design_search_engine::{
    ArchitectureCognitionSearchIntegration, BeamSearchController, SearchConfig,
    SearchController as _,
};
use world_model_core::WorldState;

#[test]
fn test4_search_integration_converges_within_configured_depth() {
    let controller = BeamSearchController::default();
    let shallow = SearchConfig {
        max_depth: 1,
        max_candidates: 8,
        beam_width: 4,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let deep = SearchConfig {
        max_depth: 3,
        ..shallow
    };
    let initial = WorldState::new(1, vec![2.0, 1.0]);

    let shallow_states = controller.search(initial.clone(), None, &shallow);
    let deep_states = controller.search(initial, None, &deep);

    let shallow_best = shallow_states
        .iter()
        .map(|state| state.score)
        .fold(0.0_f64, f64::max);
    let deep_best = deep_states
        .iter()
        .map(|state| state.score)
        .fold(0.0_f64, f64::max);
    let best_state = deep_states
        .iter()
        .max_by(|left, right| left.score.total_cmp(&right.score))
        .expect("deep search must produce states");
    let cognition = ArchitectureCognitionSearchIntegration::default()
        .snapshot(best_state, "architecture search for scalable api");

    assert!(!shallow_states.is_empty());
    assert!(!deep_states.is_empty());
    assert!(
        deep_best >= shallow_best,
        "deep_best={deep_best}, shallow_best={shallow_best}"
    );
    assert!(deep_best >= 0.45, "deep_best={deep_best}");
    assert!(best_state.depth <= deep.max_depth);
    assert!(cognition.score > 0.0);
    assert!(cognition.architecture_state.evaluation.is_some());
}
