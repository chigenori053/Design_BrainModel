#[test]
fn fixed_seed_trace_signature_regression() {
    let cfg = agent_core::TraceRunConfig {
        depth: 3,
        beam: 4,
        seed: 42,
        norm_alpha: 0.1,
        adaptive_alpha: false,
        hv_guided: false,
        raw_output_path: None,
    };
    let rows = agent_core::runtime::execute_soft_trace(cfg, agent_core::SoftTraceParams::default());
    let sig = rows
        .last()
        .map(|r| (r.depth, r.pareto_size, r.collapse_flag))
        .unwrap_or((0, 0, false));
    assert_eq!(sig.0, 3);
    assert!(sig.1 > 0);
}
