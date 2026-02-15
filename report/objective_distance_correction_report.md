# Objective Distance Correction Report

## Scope
- Spec: Objective Distance Correction v1.0
- Run date: 2026-02-15 18:35:00 +0900
- Scenario: Experiment4 Set A (alpha=3.0, T=0.8, lambda_min=0.05, entropy_beta=0.0)
- Conditions: DHM=OFF, Field距離制限=OFF, Diversity Pressure=OFF, beam=5, depth=100, seed=42

## Artifacts
- Trace: report/experiment4_setA_objective_distance_trace.csv
- Bench: report/experiment4_setA_objective_distance_bench.txt
- Metrics: report/objective_distance_correction_metrics.csv

## Implementation Checklist
- [x] Single distance space: normalized objective vectors used for NN/spacing/HV/collapse
- [x] Global robust normalization stats (warmup/fixed) and trace export of median/MAD
- [x] Relative collapse criterion (D(d) < 0.01 * D_med and pareto_front_size >= 2)
- [x] Beam selection reflects distance via max-min in normalized space
- [x] Trace columns added: norm_median_i, norm_mad_i, mean_nn_dist, median_nn_dist_all_depth, collapse_flag, normalization_mode

## Results
- diversity_mean: 0.002618123
- diversity_min: 0.000000000
- collapse_depth_count: 79 / 100 (ratio=0.7900)
- pareto_front_size_mean: 9.400000
- mean_nn_dist_mean: 1.229082304
- median_nn_dist_all_depth: 2.884950638
- avg_total_ms: 30.839
- avg_total_ms (previous Set A): 17.225
- avg_total_ms delta: 79.04%
- lambda_final: 0.224552

## Spec Gate Evaluation
- collapse_depth_count < 20%: FAIL (actual ratio: 0.7900)
- diversity_min non-zero continuity: FAIL (actual min: 0.000000000)
- pareto_front_size_mean >= 5: PASS (actual: 9.400000)
- avg_total_ms within ±20% vs previous Set A: FAIL (delta: 79.04%)

## Collapse Depths
11|12|19|20|21|22|23|24|25|26|27|28|29|30|31|32|33|34|35|36|37|38|39|40|41|42|43|44|45|46|47|48|49|50|51|52|53|54|55|56|57|58|59|60|61|62|63|64|65|66|67|68|71|72|74|75|76|77|78|79|80|81|82|83|85|86|87|88|89|91|92|93|94|95|96|97|98|99|100

## Notes
- Collapse persists under relative criterion and normalized-distance-only selection path.
- Performance regressed vs previous Set A benchmark; optimization in candidate scoring/selection path is needed.
