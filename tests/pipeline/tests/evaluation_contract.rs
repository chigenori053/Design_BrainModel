use design_search_engine::stable_v03::{DeterministicBeamSearchEngine, evaluate_hypothesis};
use pipeline_tests::{extract_fn_body, read_workspace_file};
use world_model::stable_v03::IntentState;

#[test]
fn evaluation_score_matches_weighted_sum_and_is_normalized() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = engine.contract_input(
        &IntentState {
            raw: "api service db".to_string(),
            tokens: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        },
        None,
    );
    let snapshot = engine.inspect_hypotheses(input);
    let score = evaluate_hypothesis(&snapshot.hypotheses[0]);

    let expected = score.parts.relevance * 0.35
        + score.parts.goal_distance * 0.25
        + score.parts.constraint * 0.20
        + score.parts.memory * 0.20;
    assert!(score.is_valid());
    assert!((score.total - expected).abs() < 1e-6);
}

#[test]
fn evaluation_layer_does_not_emit_decision_or_validation() {
    let source = read_workspace_file("crates/engine/design_search_engine/src/stable_v03.rs");
    let body = extract_fn_body(&source, "pub fn evaluate_hypothesis(");

    assert!(!body.contains("Decision"));
    assert!(!body.contains("Validation"));
}
