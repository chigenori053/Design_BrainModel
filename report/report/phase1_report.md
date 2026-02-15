# Phase 1 Report: Objective Independence Verification

## Scope
- DHM: OFF
- Field distance constraint: OFF
- Soft category balancing: ON
- Lambda control: enabled
- Beam=5, Depth=100, Seed=42

## Artifacts
- `report/trace_phase1_raw.csv`
- `report/trace_phase1_summary.csv`
- `report/correlation_heatmap.png`
- `report/objective_overlap_report.md`
- `report/phase1_variant_metrics.csv`

## Matrix
- Base: normalization ON, Delta OFF, Ortho OFF
- Delta: normalization ON, Delta ON, Ortho OFF
- Ortho: normalization ON, Delta OFF, Ortho epsilon=0.02

## Key Metrics
From `phase1_variant_metrics.csv`:
- Base:
  - collapse_depth_count: 0/100
  - mean_nn_dist_mean: 721.013195311
  - pareto_front_size_mean: 10.41
- Delta:
  - collapse_depth_count: 44/100
  - mean_nn_dist_mean: 661.450384344
  - pareto_front_size_mean: 10.79
- Ortho:
  - collapse_depth_count: 85/100
  - mean_nn_dist_mean: 765.816473516
  - pareto_front_size_mean: 14.59

## Collapse / Contraction Checks
- Collapse criterion (spec): `mean_nn_dist < 1e-4 && pareto_front_size >= 2`
- Base variant result:
  - collapse_depth_count = 0 (criterion not triggered)
  - high correlation multi-pair depths = 69
  - contraction decision: NOT_CONFIRMED (because collapse ratio < 50%)

## Acceptance Against Phase1 Conditions
- `collapse_depth_count < 30%`: PASS (Base)
- `mean_nn_dist_mean > 1e-3`: PASS
- `|Corr| > 0.9` pairs half reduction: NOT ACHIEVED
- `diversity_mean` upward trend: INCONCLUSIVE in this run

## Notes
- Robust normalization is applied only for distance/correlation computations.
- Raw objective values are unchanged in logged raw vectors.
- Delta/Ortho are diagnostic variants, not production objective definitions.
