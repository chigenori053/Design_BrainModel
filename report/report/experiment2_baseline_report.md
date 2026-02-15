# Experiment 2 Baseline Report (Memory系OFF)

## Conditions
- Mode: `--baseline-off`
- Disabled:
  - DHM interference (`mu=0`, disabled)
  - Field distance restriction (`field_rejected_count` should stay 0)
  - Diversity pressure (`epsilon_effect=0`)
  - Neighborhood-k DHM limit (normal rule expansion path)
- Kept:
  - SHM/CHM rule apply
  - Objective eval
  - Pareto
  - Beam
  - lambda control
- Fixed run conditions:
  - `depth=100`
  - `beam=5`
  - `release`
  - `seed=42`

## Commands
```bash
cargo run -p design_cli --release -- --trace --baseline-off --trace-depth 100 --trace-beam 5 --trace-output trace_depth100_experiment2_baseline.csv
cargo run -p design_cli --release -- --bench --baseline-off --bench-depth 100 --bench-beam 5 --bench-iter 20 --bench-warmup 3 > bench_depth100_experiment2_baseline.txt
```

## Results
- diversity_mean: `0.002648081`
- diversity_min: `0.000000000`
- pareto_mean: `5.750000000`
- lambda_variance: `0.031655089`
- field_rejected_count_mean: `0.000000000`
- avg_total_ms: `21.836`
- avg_per_depth_ms: `0.218`
- avg_pareto_us: `6.318`
- avg_field_us: `92.024`

## Branch Interpretation
- Case A (diversity low ~0.002-0.005): **matched**
- Case B (diversity >=0.02): not matched
- Case C (performance <70ms): **matched**

Interpretation:
- Diversity is already low in baseline, so generation-side quality bottleneck is strongly indicated.
- Runtime is far below 70ms, so baseline-off removes major overhead from control stack, especially Pareto-side filtering interactions and related control path costs.

## Artifacts
- Trace log: `trace_depth100_experiment2_baseline.csv`
- Bench log: `bench_depth100_experiment2_baseline.txt`
- Metrics CSV: `experiment2_baseline_metrics.csv`
