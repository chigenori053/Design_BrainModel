use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use contracts::{Hypothesis, HypothesisId, ScoreParts, SemanticHash, State, StateHash};
use design_search_engine::stable_v03::validate_hypothesis_set;
use pipeline_tests::{extract_fn_body, read_workspace_file};

#[test]
fn validation_returns_only_reasons_and_does_not_mutate_scores() {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .build()
        .expect("valid graph");
    let hypotheses = vec![
        Hypothesis {
            id: HypothesisId(1),
            state: State {
                architecture: architecture.clone(),
            },
            parent: None,
            depth: 0,
            score: 1.2,
            score_parts: ScoreParts {
                relevance: 1.0,
                goal_distance: 1.0,
                constraint: 1.0,
                memory: 1.0,
            },
            state_hash: StateHash("same".to_string()),
            semantic_hash: SemanticHash("sem".to_string()),
        },
        Hypothesis {
            id: HypothesisId(2),
            state: State { architecture },
            parent: None,
            depth: 0,
            score: 0.8,
            score_parts: ScoreParts {
                relevance: 0.8,
                goal_distance: 0.8,
                constraint: 1.0,
                memory: 0.0,
            },
            state_hash: StateHash("same".to_string()),
            semantic_hash: SemanticHash("sem".to_string()),
        },
    ];
    let before = hypotheses
        .iter()
        .map(|hypothesis| hypothesis.score)
        .collect::<Vec<_>>();

    let validation = validate_hypothesis_set(&hypotheses);
    let after = hypotheses
        .iter()
        .map(|hypothesis| hypothesis.score)
        .collect::<Vec<_>>();

    assert_eq!(before, after);
    assert!(!validation.is_valid);
    assert!(!validation.reasons.is_empty());
}

#[test]
fn validation_layer_does_not_rewrite_scores() {
    let source = read_workspace_file("crates/engine/design_search_engine/src/stable_v03.rs");
    let body = extract_fn_body(&source, "pub fn validate_hypothesis_set(");

    assert!(!body.contains("score ="));
}
