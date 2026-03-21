use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use design_search_engine::stable_v03::{
    Constraint, DesignSearchEngine, DeterministicBeamSearchEngine, RecallContext, RecalledPattern,
    SearchInput,
};
use world_model::stable_v03::IntentState;

fn test_input() -> SearchInput {
    SearchInput {
        intent: IntentState {
            raw: "api service repository".to_string(),
            tokens: vec![
                "api".to_string(),
                "service".to_string(),
                "repository".to_string(),
            ],
        },
        recall: Some(recall_context()),
    }
}

fn input_with_tokens(tokens: &[&str]) -> SearchInput {
    SearchInput {
        intent: IntentState {
            raw: tokens.join(" "),
            tokens: tokens.iter().map(|token| token.to_string()).collect(),
        },
        recall: Some(recall_context()),
    }
}

fn recall_context() -> RecallContext {
    let recalled = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("repository", NodeType::Component))
        .build()
        .expect("valid graph");
    RecallContext {
        patterns: vec![RecalledPattern {
            record_id: "memory-1".to_string(),
            architecture: recalled,
            score: 0.9,
            tags: vec!["api".to_string(), "repository".to_string()],
        }],
        constraints: vec![Constraint {
            key: "layer".to_string(),
            value: "service".to_string(),
        }],
        confidence: 0.8,
    }
}

#[test]
fn search_is_pure_for_same_input() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = test_input();

    let first = engine.search(input.clone());
    let second = engine.search(input);

    assert_eq!(first, second);
}

#[test]
fn recall_reduces_search_width() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 4,
        max_depth: 2,
        ..DeterministicBeamSearchEngine::default()
    };
    let input_without_recall = SearchInput {
        intent: test_input().intent,
        recall: None,
    };
    let input_with_recall = test_input();

    let without_recall = engine.search(input_without_recall);
    let with_recall = engine.search(input_with_recall);

    assert!(with_recall.len() < without_recall.len());
}

#[test]
fn search_trace_captures_scores_and_hypothesis_tree() {
    let engine = DeterministicBeamSearchEngine::default();
    let result = engine.search_with_trace(test_input());

    assert!(!result.candidates.is_empty());
    assert!(result.trace.generated_hypotheses > 0);
    assert!(!result.trace.frontier_by_depth.is_empty());
    assert!(!result.trace.score_breakdown.is_empty());
    assert_eq!(result.trace.recall_hit_rate, 1.0);
}

#[test]
fn search_trace_is_deterministic_for_same_input() {
    let engine = DeterministicBeamSearchEngine::default();

    let lhs = engine.search_with_trace(test_input());
    let rhs = engine.search_with_trace(test_input());

    assert_eq!(lhs.trace, rhs.trace);
    assert_eq!(lhs.candidates, rhs.candidates);
}

#[test]
fn best_score_does_not_regress_across_depths() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 4,
        max_depth: 3,
        ..DeterministicBeamSearchEngine::default()
    };
    let result = engine.search_with_trace(test_input());

    let mut previous = 0.0_f32;
    for ids in result.trace.frontier_by_depth.values() {
        let current = ids
            .iter()
            .filter_map(|id| result.trace.score_breakdown.get(id))
            .map(|score| score.total)
            .fold(previous, f32::max);
        assert!(current + 0.05 >= previous);
        previous = current;
    }
}

#[test]
fn adaptive_beam_reduces_nodes_and_latency_against_fixed_baseline() {
    let adaptive = DeterministicBeamSearchEngine {
        beam_width: 5,
        max_depth: 4,
        max_beam_width: 5,
        adaptive_beam: true,
        ..DeterministicBeamSearchEngine::default()
    };
    let fixed = DeterministicBeamSearchEngine {
        beam_width: 5,
        max_depth: 4,
        max_beam_width: 5,
        adaptive_beam: false,
        ..DeterministicBeamSearchEngine::default()
    };
    let input = input_with_tokens(&["api", "service", "repository", "cache", "worker", "queue"]);

    let adaptive_result = adaptive.search_with_trace(input.clone());
    let fixed_result = fixed.search_with_trace(input);

    assert!(adaptive_result.trace.stats.total_nodes < fixed_result.trace.stats.total_nodes);
    assert!(adaptive_result.trace.execution_time_ms * 100 <= fixed_result.trace.execution_time_ms * 80);
}

#[test]
fn synthetic_latency_scales_with_input_complexity() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 4,
        max_depth: 3,
        ..DeterministicBeamSearchEngine::default()
    };

    let small = engine.search_with_trace(input_with_tokens(&["api", "db"]));
    let medium = engine.search_with_trace(input_with_tokens(&["api", "service", "db", "cache"]));
    let large = engine.search_with_trace(input_with_tokens(&[
        "api", "service", "db", "cache", "worker", "queue", "analytics",
    ]));

    assert!(small.trace.execution_time_ms <= medium.trace.execution_time_ms);
    assert!(medium.trace.execution_time_ms <= large.trace.execution_time_ms);
}
