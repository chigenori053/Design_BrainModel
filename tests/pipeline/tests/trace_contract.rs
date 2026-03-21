use design_cli::renderer::render_reasoning_trace;
use contracts::{ReasoningTrace, RequestId, TraceStats, TraceStep};

#[test]
fn trace_steps_are_sorted_and_stats_recompute() {
    let trace = ReasoningTrace::new(
        RequestId("r1".to_string()),
        vec![
            TraceStep {
                depth: 2,
                beam_width: 1,
                candidates: 1,
                pruned: 0,
                recall_hits: 0,
            },
            TraceStep {
                depth: 0,
                beam_width: 2,
                candidates: 2,
                pruned: 0,
                recall_hits: 0,
            },
            TraceStep {
                depth: 1,
                beam_width: 2,
                candidates: 3,
                pruned: 1,
                recall_hits: 1,
            },
        ],
    );

    assert!(trace.steps.windows(2).all(|pair| pair[0].depth <= pair[1].depth));
    assert_eq!(trace.stats, TraceStats::from_steps(&trace.steps));
}

#[test]
fn cli_trace_render_matches_contract_fields() {
    let trace = ReasoningTrace::new(
        RequestId("req-1".to_string()),
        vec![TraceStep {
            depth: 0,
            beam_width: 3,
            candidates: 2,
            pruned: 0,
            recall_hits: 1,
        }],
    );
    let mut out = Vec::new();

    render_reasoning_trace(&mut out, &trace).expect("render succeeds");
    let rendered = String::from_utf8(out).expect("utf8");

    assert!(rendered.contains("request_id=req-1"));
    assert!(rendered.contains(&format!("total_nodes={}", trace.stats.total_nodes)));
    assert!(rendered.contains(&format!("max_depth={}", trace.stats.max_depth)));
    assert!(rendered.contains("depth 0 beam=3 candidates=2 pruned=0 recall_hits=1"));
}
