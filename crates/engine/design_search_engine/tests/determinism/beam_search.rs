use design_search_engine::{
    BeamSearchController, SearchConfig, SearchController as _, prune_candidates, rank_candidates,
};
use memory_space_core::RecallResult;
use world_model_core::WorldState;

fn make_world_state() -> WorldState {
    WorldState::new(1, vec![0.5, 1.0, 1.5])
}

fn make_recall() -> RecallResult {
    RecallResult {
        candidates: vec![memory_space_core::RecallCandidate {
            memory_id: 1,
            feature_vector: vec![0.9, 0.8, 0.7],
            relevance_score: 0.85,
        }],
    }
}

#[test]
fn beam_search_controller_is_deterministic() {
    let config = SearchConfig::default();
    let initial = make_world_state();
    let recall = make_recall();

    let a = BeamSearchController::default().search(initial.clone(), Some(&recall), &config);
    let b = BeamSearchController::default().search(initial, Some(&recall), &config);

    assert_eq!(a.len(), b.len(), "search state count must be identical");
    for (sa, sb) in a.iter().zip(b.iter()) {
        assert_eq!(sa.state_id, sb.state_id, "state_id must match");
        assert_eq!(sa.depth, sb.depth, "depth must match");
        assert!((sa.score - sb.score).abs() < 1e-10, "score must match");
        assert_eq!(
            sa.world_state.features, sb.world_state.features,
            "features must match"
        );
    }
}

#[test]
fn beam_search_controller_deterministic_no_recall() {
    let config = SearchConfig::default();
    let initial = make_world_state();

    let a = BeamSearchController::default().search(initial.clone(), None, &config);
    let b = BeamSearchController::default().search(initial, None, &config);

    assert_eq!(a.len(), b.len());
    for (sa, sb) in a.iter().zip(b.iter()) {
        assert_eq!(sa.state_id, sb.state_id);
        assert!((sa.score - sb.score).abs() < 1e-10);
    }
}

#[test]
fn ranking_is_deterministic() {
    let controller = BeamSearchController::default();
    let config = SearchConfig::default();
    let initial = make_world_state();
    let recall = make_recall();

    let states = controller.search(initial, Some(&recall), &config);

    let ranked_a = rank_candidates(states.clone());
    let ranked_b = rank_candidates(states);

    assert_eq!(ranked_a.len(), ranked_b.len());
    for (ra, rb) in ranked_a.iter().zip(ranked_b.iter()) {
        assert_eq!(ra.state.state_id, rb.state.state_id);
        assert!((ra.score - rb.score).abs() < 1e-10);
    }
}

#[test]
fn best_candidate_has_highest_score() {
    let controller = BeamSearchController::default();
    let config = SearchConfig::default();
    let initial = make_world_state();

    let states = controller.search(initial, None, &config);
    let ranked = rank_candidates(states);

    if let Some(best) = ranked.first() {
        for candidate in &ranked {
            assert!(best.score >= candidate.score - 1e-10);
        }
    }
}

#[test]
fn prune_candidates_respects_beam_width() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 2,
        max_candidates: 64,
        beam_width: 3,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let initial = make_world_state();

    let states = controller.search(initial, None, &config);
    let pruned = prune_candidates(states, 3);

    assert!(pruned.len() <= 3, "prune must not exceed beam_width");
}

#[test]
fn search_respects_beam_width() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 2,
        max_candidates: 64,
        beam_width: 2,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let initial = make_world_state();

    let states = controller.search(initial, None, &config);

    assert!(
        states.len() <= 2,
        "search result must not exceed beam_width, got {}",
        states.len()
    );
}
