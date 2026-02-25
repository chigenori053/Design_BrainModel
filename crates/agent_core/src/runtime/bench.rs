pub fn run(config: crate::BenchConfig) -> crate::BenchResult {
    run_baseline_off(config)
}

pub fn run_baseline_off(config: crate::BenchConfig) -> crate::BenchResult {
    run_baseline_off_soft(config, crate::SoftTraceParams::default())
}

pub fn run_baseline_off_balanced(config: crate::BenchConfig, m: usize) -> crate::BenchResult {
    let mut params = crate::SoftTraceParams::default();
    params.alpha = (m as f64 / 10.0).clamp(0.1, 1.0);
    run_baseline_off_soft(config, params)
}

pub fn run_baseline_off_soft(
    config: crate::BenchConfig,
    params: crate::SoftTraceParams,
) -> crate::BenchResult {
    let iterations = config.iterations.max(1);
    for i in 0..config.warmup {
        let cfg = crate::TraceRunConfig {
            depth: config.depth,
            beam: config.beam,
            seed: config.seed.wrapping_add(i as u64),
            norm_alpha: config.norm_alpha,
            adaptive_alpha: false,
            hv_guided: false,
            raw_output_path: None,
        };
        let _ = crate::runtime::execute_soft_trace(cfg, params);
    }

    let mut total_ms = 0.0f64;
    let mut lambda_final = 0.0f64;
    for i in 0..iterations {
        let cfg = crate::TraceRunConfig {
            depth: config.depth,
            beam: config.beam,
            seed: config.seed.wrapping_add(i as u64),
            norm_alpha: config.norm_alpha,
            adaptive_alpha: false,
            hv_guided: false,
            raw_output_path: None,
        };
        let start = std::time::Instant::now();
        let rows = crate::runtime::execute_soft_trace(cfg, params);
        total_ms += start.elapsed().as_secs_f64() * 1000.0;
        lambda_final += rows.last().map(|r| r.lambda as f64).unwrap_or(0.5);
    }

    let denom = iterations as f64;
    crate::BenchResult {
        depth: config.depth,
        beam: config.beam,
        iterations,
        avg_total_ms: total_ms / denom,
        avg_per_depth_ms: (total_ms / denom) / config.depth.max(1) as f64,
        avg_field_us: 0.0,
        avg_resonance_us: 0.0,
        avg_chm_us: 0.0,
        avg_dhm_us: 0.0,
        avg_pareto_us: 0.0,
        avg_lambda_us: 0.0,
        lambda_final: lambda_final / denom,
    }
}
