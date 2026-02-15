# Experiment 4 Report: Soft-Balanced Exploration & Stability Control

## Scope
Implemented and validated against the provided Experiment 4 spec under fixed conditions:
- DHM OFF
- Field distance OFF
- Diversity Pressure OFF
- SHM/CHM ON, Objective ON, Pareto ON
- beam=5, depth=100, seed=42

## Implemented Controls
- Soft category balancing:
  - `w_i = exp(-alpha * (p_i - u_i))`
  - `S_final = S_base * w_cat`
  - softmax with temperature `T`
- Entropy logging per depth:
  - `category_entropy`
- Lambda redesign:
  - `lambda_min=0.05`, `lambda_max=1.0`
  - `e = H* - H`, `lambda' = lambda + k*e`
  - EMA smoothing and clamp
- Pareto dispersion logging:
  - `pareto_front_size`
  - `pareto_mean_nn_dist`
  - `pareto_spacing`
  - `pareto_hv_2d` (2D rectangle approximation)
- Field cost profiling columns:
  - `field_extract_us`, `field_score_us`, `field_aggregate_us`, `field_total_us`
- Cheap ranking stage:
  - detailed field scoring on top `beam*5`
- CLI options added:
  - `--category-soft`
  - `--category-alpha`
  - `--temperature`
  - `--entropy-beta`
  - `--lambda-min`
  - `--lambda-target-entropy`
  - `--lambda-k`
  - `--lambda-ema`
  - `--log-per-depth`
  - `--field-profile`

## Matrix Executed
- Set A: alpha=3.0, T=0.8, lambda_min=0.05, entropy_beta=0.00
- Set B: alpha=5.0, T=0.7, lambda_min=0.05, entropy_beta=0.00
- Set C: alpha=2.0, T=1.0, lambda_min=0.05, entropy_beta=0.00
- Set D: alpha=3.0, T=0.8, lambda_min=0.05, entropy_beta=0.02

## Aggregated Results
See: `report/experiment4_matrix/experiment4_matrix_summary.csv`

Key outcomes (all sets):
- diversity_mean: `0.001167086`
- diversity_min: `0.000000000`
- lambda_final_bench: `0.445198` (> 0.05)
- pareto_front_size_mean: `9.48` (>= 5)
- avg_total_ms: `17.2~17.7ms` (<= 35ms)

## Success Criteria Check
Required:
- diversity_mean >= 0.03: FAIL
- diversity_min > 0: FAIL
- lambda_final > 0.05: PASS
- pareto_front_size_mean >= 5: PASS
- avg_total_ms <= 35ms: PASS

Overall: all sets **FAIL** due to diversity criteria.

## Collapse Diagnosis
- Collapse depth count (diversity==0): 91/100 (all sets)
- First collapse depths: 10..19 (see summary CSV)
- Entropy and category counts remain non-zero, so collapse is not pure single-category collapse.
- Current behavior indicates objective-space contraction despite balanced category exposure.

## Artifacts
- Set A trace: `report/experiment4_matrix/trace_set_A.csv`
- Set A bench: `report/experiment4_matrix/bench_set_A.txt`
- Set B trace: `report/experiment4_matrix/trace_set_B.csv`
- Set B bench: `report/experiment4_matrix/bench_set_B.txt`
- Set C trace: `report/experiment4_matrix/trace_set_C.csv`
- Set C bench: `report/experiment4_matrix/bench_set_C.txt`
- Set D trace: `report/experiment4_matrix/trace_set_D.csv`
- Set D bench: `report/experiment4_matrix/bench_set_D.txt`
- Summary CSV: `report/experiment4_matrix/experiment4_matrix_summary.csv`
