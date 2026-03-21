use contracts::{Decision, EvaluationScore, ScoreParts};
use design_search_engine::stable_v03::decide;
use pipeline_tests::{extract_fn_body, read_workspace_file};

#[test]
fn decision_depends_only_on_evaluation_score() {
    let accept = decide(&EvaluationScore::from_parts(ScoreParts {
        relevance: 1.0,
        goal_distance: 1.0,
        constraint: 1.0,
        memory: 1.0,
    }));
    let reject = decide(&EvaluationScore::from_parts(ScoreParts {
        relevance: 0.0,
        goal_distance: 0.0,
        constraint: 0.0,
        memory: 0.0,
    }));
    let cont = decide(&EvaluationScore::from_parts(ScoreParts {
        relevance: 0.6,
        goal_distance: 0.6,
        constraint: 0.5,
        memory: 0.5,
    }));

    assert_eq!(accept, Decision::Accept);
    assert_eq!(reject, Decision::Reject);
    assert_eq!(cont, Decision::Continue);
}

#[test]
fn decision_layer_does_not_call_validation() {
    let source = read_workspace_file("crates/engine/design_search_engine/src/stable_v03.rs");
    let body = extract_fn_body(&source, "pub fn decide(");

    assert!(!body.contains("Validation"));
}
