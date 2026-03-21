use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use bridge::{legacy::LegacyHypothesis, reasoning_input_from_intent};
use contracts::{
    Context, EvaluationScore, Hypothesis, HypothesisId, MemoryCandidate, MemorySource, NodeId,
    ReasoningInput, ReasoningTrace, RequestId, ScoreParts, SemanticHash, SemanticRepresentation, State,
    StateHash, TraceStats, TraceStep, ValidationReason, ValidationResult,
};
use design_search_engine::stable_v03::{
    Constraint, DesignSearchEngine, DeterministicBeamSearchEngine, RecallContext, RecalledPattern,
};
use world_model::stable_v03::IntentState;

fn recall_context() -> RecallContext {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .build()
        .expect("valid graph");
    RecallContext {
        patterns: vec![RecalledPattern {
            record_id: "seed".to_string(),
            architecture,
            score: 0.9,
            tags: vec!["api".to_string(), "service".to_string()],
        }],
        constraints: vec![Constraint {
            key: "contains".to_string(),
            value: "service".to_string(),
        }],
        confidence: 0.9,
    }
}

fn search_input() -> ReasoningInput {
    let intent = IntentState {
        raw: "api service cache".to_string(),
        tokens: vec!["api".to_string(), "service".to_string(), "cache".to_string()],
    };
    let extra_tokens = recall_context()
        .patterns
        .iter()
        .flat_map(|pattern| pattern.tags.iter().cloned())
        .collect::<Vec<_>>();
    reasoning_input_from_intent(
        &intent,
        &extra_tokens,
        Context {
            beam_width: 3,
            max_depth: 2,
            timeout_ms: 2_000,
        },
    )
}

#[test]
fn score_ranges_and_relation_invariants_hold() {
    let score = EvaluationScore::from_parts(ScoreParts {
        relevance: 0.8,
        goal_distance: 0.7,
        constraint: 1.0,
        memory: 0.5,
    });
    let candidate = MemoryCandidate {
        id: "m1".to_string(),
        score: 1.0,
        source: MemorySource::Exact,
        rank: 0,
    };
    let semantic = SemanticRepresentation::new(
        vec![contracts::Relation {
            from: NodeId("intent:a".to_string()),
            to: NodeId("intent:b".to_string()),
            relation_type: contracts::RelationType::DerivedFrom,
        }],
        vec![
            contracts::Intent {
                label: "a".to_string(),
            },
            contracts::Intent {
                label: "b".to_string(),
            },
        ],
    );

    assert!(score.is_valid());
    assert!(candidate.is_valid());
    assert!(semantic.relations.iter().all(|relation| relation.from != relation.to));
}

#[test]
fn trace_is_deterministic_for_same_input() {
    let engine = DeterministicBeamSearchEngine::default();

    let lhs = engine.search_with_trace(search_input());
    let rhs = engine.search_with_trace(search_input());

    assert_eq!(lhs.trace, rhs.trace);
    assert_eq!(lhs.candidates, rhs.candidates);
}

#[test]
fn candidate_ordering_is_stable() {
    let engine = DeterministicBeamSearchEngine::default();
    let result = engine.search_with_trace(search_input());

    let mut sorted = result.candidates.clone();
    sorted.sort_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).then_with(|| lhs.id.cmp(&rhs.id)));
    assert_eq!(result.candidates, sorted);
}

#[test]
fn trace_stats_recompute_from_steps() {
    let trace = ReasoningTrace::new(
        RequestId("r1".to_string()),
        vec![
            TraceStep {
                depth: 0,
                beam_width: 2,
                candidates: 2,
                pruned: 0,
                recall_hits: 2,
            },
            TraceStep {
                depth: 1,
                beam_width: 2,
                candidates: 4,
                pruned: 2,
                recall_hits: 1,
            },
        ],
    );

    assert_eq!(trace.stats, TraceStats::from_steps(&trace.steps));
}

#[test]
fn boundary_cases_are_handled() {
    let engine = DeterministicBeamSearchEngine::default();
    let no_recall = reasoning_input_from_intent(
        &IntentState {
            raw: "api".to_string(),
            tokens: vec!["api".to_string()],
        },
        &[],
        Context {
            beam_width: 3,
            max_depth: 2,
            timeout_ms: 2_000,
        },
    );
    let duplicate_validation = ValidationResult::new(vec![ValidationReason::DuplicateState]);

    let result = engine.search_with_trace(no_recall);

    assert!(!result.candidates.is_empty());
    assert!(result.trace.stats.max_depth <= engine.max_depth);
    assert!(!duplicate_validation.is_valid);
}

#[test]
fn hypothesis_is_contract_valid() {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .build()
        .expect("valid graph");
    let hypothesis = Hypothesis {
        id: HypothesisId(1),
        state: State { architecture },
        parent: None,
        depth: 0,
        score: 0.9,
        score_parts: ScoreParts {
            relevance: 0.9,
            goal_distance: 0.9,
            constraint: 1.0,
            memory: 0.5,
        },
        state_hash: StateHash("s1".to_string()),
        semantic_hash: SemanticHash("sem1".to_string()),
    };

    assert!(hypothesis.is_valid());
}

#[test]
fn legacy_to_contract_conversion_normalizes_boundary_values() {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .build()
        .expect("valid graph");
    let legacy = LegacyHypothesis {
        id: 7,
        state: architecture,
        parent: None,
        depth: 1,
        score: 1.4,
        relevance: 0.9,
        goal_distance: 0.8,
        constraint: 1.2,
        memory: -0.1,
        semantic_hash: SemanticHash("sem".to_string()),
    };

    let contract: Hypothesis = legacy.into();

    assert!(contract.is_valid());
    assert!((0.0..=1.0).contains(&contract.score));
}
