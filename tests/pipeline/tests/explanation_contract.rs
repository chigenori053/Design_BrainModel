use design_cli::renderer::render_explanation;
use contracts::{
    MemoryRef, NodeId, ReasoningTrace, Relation, RelationType, RequestId, Strategy,
    StrategyReason, TraceProofStep, TraceStep,
};
use runtime_core::{
    CompressionConfig, TraceIndex, build_proof, compress_proof, explain_reasoning_trace,
    infer_with_explain_full,
};

fn relation(from: &str, to: &str) -> Relation {
    Relation {
        from: NodeId(from.to_string()),
        to: NodeId(to.to_string()),
        relation_type: RelationType::DependsOn,
    }
}

fn traced_reasoning() -> ReasoningTrace {
    ReasoningTrace::with_explainability(
        RequestId("req-1".to_string()),
        vec![
            TraceStep {
                depth: 0,
                beam_width: 2,
                candidates: 2,
                pruned: 0,
                recall_hits: 1,
            },
            TraceStep {
                depth: 1,
                beam_width: 2,
                candidates: 1,
                pruned: 0,
                recall_hits: 1,
            },
        ],
        vec![
            TraceProofStep {
                trace_id: 1,
                inputs: vec![],
                output: relation("service", "db"),
                rule: Some("memory_rule".to_string()),
                memory_refs: vec![MemoryRef {
                    experience_id: "mem-1".to_string(),
                    confidence: 0.82,
                    contribution: 0.75,
                }],
                strategy: Strategy::Backward,
            },
            TraceProofStep {
                trace_id: 2,
                inputs: vec![relation("service", "db")],
                output: relation("api", "service"),
                rule: Some("compose".to_string()),
                memory_refs: Vec::new(),
                strategy: Strategy::Backward,
            },
        ],
        StrategyReason {
            strategy: Strategy::Backward,
            estimated_cost: 1.5,
            branching_factor: 1,
            reason: "goal is specific and branching is low".to_string(),
        },
    )
}

#[test]
fn proof_tree_matches_trace_index_parents() {
    let trace = traced_reasoning();
    let index = TraceIndex::from_trace(&trace);
    let proof = build_proof(&relation("api", "service"), &index);

    assert_eq!(proof.parents.len(), 1);
    assert_eq!(proof.parents[0].relation, relation("service", "db"));
}

#[test]
fn infer_with_explain_is_complete_and_deterministic() {
    let trace = traced_reasoning();

    let lhs = infer_with_explain_full(&trace).expect("explanation");
    let rhs = infer_with_explain_full(&trace).expect("explanation");

    assert_eq!(lhs, rhs);
    assert_eq!(lhs.0.len(), trace.proof_steps.len());
    assert_eq!(lhs.1.memory_summary.len(), 1);
}

#[test]
fn proof_builder_stops_on_cycles() {
    let trace = ReasoningTrace::with_explainability(
        RequestId("req-cycle".to_string()),
        vec![TraceStep {
            depth: 0,
            beam_width: 1,
            candidates: 1,
            pruned: 0,
            recall_hits: 0,
        }],
        vec![
            TraceProofStep {
                trace_id: 1,
                inputs: vec![relation("b", "a")],
                output: relation("a", "b"),
                rule: Some("loop".to_string()),
                memory_refs: Vec::new(),
                strategy: Strategy::Backward,
            },
            TraceProofStep {
                trace_id: 2,
                inputs: vec![relation("a", "b")],
                output: relation("b", "a"),
                rule: Some("loop".to_string()),
                memory_refs: Vec::new(),
                strategy: Strategy::Backward,
            },
        ],
        StrategyReason {
            strategy: Strategy::Backward,
            estimated_cost: 2.0,
            branching_factor: 1,
            reason: "cycle test".to_string(),
        },
    );
    let explanation = explain_reasoning_trace(&trace).expect("explanation");

    assert!(explanation.text.contains("cycle detected"));
}

#[test]
fn explanation_reflects_memory_and_strategy_in_text() {
    let trace = traced_reasoning();
    let mut output = Vec::new();
    let explanation = runtime_core::Explanation {
        intent: Vec::new(),
        decisions: Vec::new(),
        reasoning: explain_reasoning_trace(&trace),
    };

    render_explanation(&mut output, &explanation).expect("render succeeds");
    let rendered = String::from_utf8(output).expect("utf8");

    assert!(rendered.contains("[Reasoning Proof]"));
    assert!(rendered.contains("strategy: Backward"));
    assert!(rendered.contains("mem-1"));
    assert!(rendered.contains("goal is specific and branching is low"));
}

#[test]
fn compression_limits_depth_without_changing_conclusion() {
    let trace = traced_reasoning();
    let explanation = explain_reasoning_trace(&trace).expect("explanation");
    let compressed = compress_proof(
        explanation.proof.clone(),
        CompressionConfig {
            max_depth: 0,
            min_confidence: 0.0,
        },
    );

    assert_eq!(compressed.relation, explanation.proof.relation);
    assert!(compressed.parents.is_empty());
}
