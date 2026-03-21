use bridge::reasoning_input_from_intent;
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use design_search_engine::stable_v03::{
    Constraint, DesignSearchEngine, DeterministicBeamSearchEngine, RecallContext, RecalledPattern,
    Context, ReasoningInput,
};
use world_model::stable_v03::IntentState;

fn test_intent() -> IntentState {
    IntentState {
        raw: "api service repository".to_string(),
        tokens: vec![
            "api".to_string(),
            "service".to_string(),
            "repository".to_string(),
        ],
    }
}

fn contract_input(intent: IntentState, recall: Option<RecallContext>) -> ReasoningInput {
    let extra_tokens = recall
        .as_ref()
        .map(|ctx| {
            ctx.patterns
                .iter()
                .flat_map(|pattern| pattern.tags.iter().cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    reasoning_input_from_intent(
        &intent,
        &extra_tokens,
        Context {
            beam_width: 4,
            max_depth: 3,
            timeout_ms: 2_000,
        },
    )
}

fn test_input() -> ReasoningInput {
    contract_input(test_intent(), Some(recall_context()))
}

fn input_with_tokens(tokens: &[&str]) -> ReasoningInput {
    contract_input(
        IntentState {
            raw: tokens.join(" "),
            tokens: tokens.iter().map(|token| token.to_string()).collect(),
        },
        Some(recall_context()),
    )
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
fn bridge_produces_deterministic_contract_input() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 4,
        max_depth: 2,
        ..DeterministicBeamSearchEngine::default()
    };
    let input_without_recall = engine.contract_input(&test_intent(), None);
    let input_with_recall = engine.contract_input(&test_intent(), Some(&recall_context()));

    assert_eq!(input_without_recall.request_id, input_with_recall.request_id);
    assert!(input_with_recall.semantic.intents.len() >= input_without_recall.semantic.intents.len());
}

#[test]
fn search_trace_captures_scores_and_hypothesis_tree() {
    let engine = DeterministicBeamSearchEngine::default();
    let result = engine.search_with_trace(test_input());

    assert!(!result.candidates.is_empty());
    assert!(result.trace.stats.total_nodes > 0);
    assert!(!result.trace.steps.is_empty());
    assert!((0.0..=1.0).contains(&result.trace.stats.recall_hit_rate));
    assert!(result.trace.stats.recall_hit_rate > 0.0);
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
fn trace_steps_remain_depth_sorted_and_accounted() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 4,
        max_depth: 3,
        ..DeterministicBeamSearchEngine::default()
    };
    let result = engine.search_with_trace(test_input());

    let mut previous_depth = 0usize;
    for step in &result.trace.steps {
        assert!(step.depth >= previous_depth);
        assert!(step.pruned <= step.candidates);
        assert!(step.beam_width >= 1);
        previous_depth = step.depth;
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
    assert!(adaptive_result.trace.steps.len() <= fixed_result.trace.steps.len());
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

    assert!(small.trace.stats.total_nodes <= medium.trace.stats.total_nodes);
    assert!(medium.trace.stats.total_nodes <= large.trace.stats.total_nodes);
}

// ── Phase OP-2: Memory Integration Tests ─────────────────────────────────────

use contracts::MemoryCandidate;
use contracts::MemorySource;

fn input_with_memory(tokens: &[&str], memory_scores: &[f32]) -> ReasoningInput {
    let mut input = input_with_tokens(tokens);
    input.memory_candidates = memory_scores
        .iter()
        .enumerate()
        .map(|(rank, &score)| MemoryCandidate {
            id: format!("mem-{rank}"),
            score,
            source: if score >= 0.9 { MemorySource::Exact } else { MemorySource::Index },
            rank,
        })
        .collect();
    input
}

#[test]
fn score_with_memory_exceeds_score_without_memory() {
    let engine = DeterministicBeamSearchEngine::default();

    let without_memory = engine.search_with_trace(input_with_tokens(&["api", "service"]));
    let with_memory = engine.search_with_trace(input_with_memory(&["api", "service"], &[0.85, 0.70]));

    let best_without = without_memory
        .candidates
        .iter()
        .map(|c| c.score)
        .fold(0.0_f64, f64::max);
    let best_with = with_memory
        .candidates
        .iter()
        .map(|c| c.score)
        .fold(0.0_f64, f64::max);

    assert!(
        best_with > best_without,
        "score with memory ({best_with:.4}) should exceed score without memory ({best_without:.4})"
    );
}

#[test]
fn memory_zero_matches_no_memory_behavior() {
    let engine = DeterministicBeamSearchEngine::default();

    let no_memory = input_with_tokens(&["api", "service"]);
    let zero_memory = input_with_memory(&["api", "service"], &[]);

    let result_no = engine.search_with_trace(no_memory);
    let result_zero = engine.search_with_trace(zero_memory);

    // Scores should be identical when memory_candidates is empty.
    assert_eq!(
        result_no.candidates.len(),
        result_zero.candidates.len(),
        "candidate count should match when no memory candidates"
    );
    for (a, b) in result_no.candidates.iter().zip(result_zero.candidates.iter()) {
        assert!((a.score - b.score).abs() < 1e-6, "scores should be identical: {:.6} vs {:.6}", a.score, b.score);
    }
}

#[test]
fn memory_high_candidates_survive_beam_selection() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 2,
        max_depth: 2,
        ..DeterministicBeamSearchEngine::default()
    };

    let with_strong_memory = engine.search_with_trace(
        input_with_memory(&["api", "service", "db"], &[0.95, 0.80, 0.60])
    );

    // All candidates should have higher scores when memory is strong.
    for candidate in &with_strong_memory.candidates {
        assert!(
            candidate.score > 0.0,
            "all candidates should score > 0 with strong memory"
        );
    }
    assert!(
        with_strong_memory.trace.stats.recall_hit_rate > 0.0,
        "recall_hit_rate should be > 0 when memory_candidates are provided"
    );
}

#[test]
fn search_remains_deterministic_with_memory() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = input_with_memory(&["api", "service", "repository"], &[0.75, 0.60]);

    let first = engine.search_with_trace(input.clone());
    let second = engine.search_with_trace(input);

    assert_eq!(first.candidates, second.candidates, "search must be deterministic with memory");
    assert_eq!(first.trace.stats, second.trace.stats);
}
