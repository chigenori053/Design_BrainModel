#[cfg(feature = "ci-heavy")]
#[test]
fn stability_depth50_beam5() {
    let cfg = agent_core::TraceRunConfig {
        depth: 50,
        beam: 5,
        seed: 2026,
        norm_alpha: 0.1,
        adaptive_alpha: false,
        hv_guided: false,
        raw_output_path: None,
    };
    let rows = agent_core::runtime::execute_soft_trace(cfg, agent_core::SoftTraceParams::default());
    assert!(!rows.is_empty());
    assert_eq!(rows.last().map(|r| r.depth), Some(50));
}
