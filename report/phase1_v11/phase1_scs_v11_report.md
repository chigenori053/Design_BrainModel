# Phase1 SCS v1.1 Integration Report

## Run Conditions
- cases per seed: 100
- seeds: 42, 43, 44, 45
- output:
  - `report/phase1_v11/phase1_scs_v11_raw.jsonl`
  - `report/phase1_v11/phase1_scs_v11_summary.json`
  - `report/phase1_v11/seed_{42,43,44,45}/integrity_summary.json`
  - `report/phase1_v11/seed_{42,43,44,45}/case_digest.csv`

## Integration Scope
- `InputSanityCheck` applied before `DependencyConsistency`
  - empty id: auto numbering
  - duplicate id: `_n` suffix
  - dangling depends_on: dropped
- `compute_scs_v1_1(ScsInputs)` connected in Phase1 path
- additional JSONL fields added:
  - `dependency_consistency`
  - `connectivity`
  - `cyclicity`
  - `orphan_rate`
  - `scs_v1_1`
  - plus temporary `scs_v1`

## Summary Metrics
- avg_cls: `0.005314`
- avg_scs_v1_1: `0.728738`
- revision_rate: `0.97`
- avg_questions: `0.010629` (proxy)
- phase2_false_trigger_rate: `0.0` (proxy)
- abnormal_rate: `0.0`
- scs_1.0_rate: `0.0`
- objective: `0.728738`
- objective_var: `0.0`
- avg_dependency_consistency: `0.5`
- avg_cyclicity: `0.0`

## Step0 Gate Result
- seed 42: PASS
- seed 43: PASS
- seed 44: PASS
- seed 45: PASS
- integrity summaries: 4 files generated
- case digests: 4 files generated
- T0-1..T0-5 (`cargo test -p design_cli step0`): PASS

## Acceptance Check (v1.1)
- avg_cls <= 0.45: PASS
- avg_scs >= 0.70: PASS
- revision_rate <= 0.20: FAIL
- phase2_false_trigger_rate <= 5%: PASS (proxy)
- abnormal_rate == 0: PASS
- scs_1.0_rate <= 5%: PASS
- objective variance ~= 0 across seeds: PASS

## Notes
- `cyclicity` is `0.0` across sampled rows in this run.
- `phase2_false_trigger_rate` and `avg_questions` are computed as proxies in the current pipeline.
- category assignment is provided by Step0 fixed distribution and aggregated across 4 seeds
  - A=80, B=100, C=80, D=60, E=40, F=40
