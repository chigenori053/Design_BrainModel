# CLI Integration Test Inventory

`cargo test -p design_cli --test integration` is reserved for the minimal KEEP set.
Legacy and broader end-to-end integration modules are quarantined behind the
`integration-late` feature.

## KEEP

- `runtime_bootstrap_isolation`

The optional KEEP modules named in the quarantine rewiring spec are not present
in this worktree and are therefore not registered:

- `repl_exit`
- `repl_executor_reachability`
- `noop_composite_stop`

## QUARANTINE / integration-late

These modules are loaded only by:

```sh
cargo test -p design_cli --test integration --features integration-late
```

- `analyze_design_json`
- `analyze_node_binding_ranking`
- `apply_preview_bridge`
- `break_cycle_analyzer_semantic_alignment`
- `break_cycle_change_set_representative_target`
- `break_cycle_mutation`
- `canonical_patch_stream`
- `cli_json_canonical_wiring`
- `cli_planner_snapshot_unification`
- `coding_apply_path_resolution`
- `coding_cargo_resolution`
- `coding_from_design_snapshot`
- `coding_import_rebinding`
- `coding_malformed_import_batch`
- `coding_mutation_flow`
- `coding_sandbox_copy`
- `coding_semantic_recovery`
- `coding_target_scope`
- `coding_target_semantic_pruning`
- `command_flow`
- `cycle_recommendation_bridge`
- `git_commit_preview`
- `gui_viewer`
- `ir_route_cleanup`
- `legacy_pipeline_elimination`
- `mutation_apply_parity`
- `mutation_target_ranking`
- `narrow_semantic_cluster_matching`
- `nl_autonomous_loop`
- `nl_multiturn_repl`
- `nl_repl_flow`
- `onboarding_aliases`
- `phase_analyze_cli`
- `planner_multilingual_semantic`
- `planner_semantic_intent`
- `preview_diff_bridge`
- `promote_workspace_patch`
- `refactor_runtime`
- `repl_apply_resolution`
- `repl_continuation_v2_wiring`
- `repl_deterministic_v2_wiring`
- `repl_diff_render`
- `repl_file_target_routing`
- `repl_followup_apply_promotion`
- `repl_semantic_cluster_narrowing`
- `repl_session_continuity`
- `repl_stability_verification`
- `repl_subcommand_dispatch`
- `run_dsl`
- `structure_session`
- `transaction_execution_bridge`
- `transaction_preview_bridge`
- `transactional_safe_apply`
- `unified_analyze_cli`
- `viewer_keymap_isolation`
- `workspace_symbol_rebinding`
