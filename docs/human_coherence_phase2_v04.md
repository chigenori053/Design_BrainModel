# Phase2.4/2.5 Human Coherence Tuning

## HC v0.5 Formula

Structural components from L2:
- `coverage`: slot coverage (5W2H/success metric presence)
- `consistency`: sign-transition stability across causal links
- `dependency_quality`: inverse of edge concentration and requirement variance

Composition:
1. `HC_struct_raw = clamp(coverage^0.5 * consistency^1.0 * dependency_quality^1.0, 0, 1)`
2. `HC_struct = clamp(HC_struct_raw^1.2, 0, 1)`
3. `alignment = mean(normalized requirement strengths)`
4. `HC_raw = clamp(0.35 * alignment + 0.65 * HC_struct, 0, 1)`
5. `HC = clamp(HC_raw^1.4, 0, 1)`

## Post-Ranking Scope Rule

- Sort priority:
1. `pareto_rank` (ascending)
2. `total_score` (descending)
3. `HC` (descending) only when both candidates satisfy `pareto_rank <= 2`
4. `case_id` (ascending)

Dump analysis uses computed HC values for all cases (no neutral fill fallback).

## Dump Analysis Contract

`report` keys:
- `sample_size`
- `corr_hc_total`
- `corr_hc_o1..o4`
- `corr_hc_pareto_rank`
- `mean_hc_frontier`
- `mean_hc_non_frontier`
- `delta_hc_frontier_non_frontier`
- `hc_mean_all`
- `hc_stddev_all`
- `hc_min_all`
- `hc_max_all`
- `mean_hc_by_rank`
- `hc_frontier_share_top10pct`
- `hc_frontier_share_top20pct`
- `hc_histogram`
