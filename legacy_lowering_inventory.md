# Legacy Lowering Inventory

## Scope

`apps/cli/src/coding.rs` production change-set lowering and report projection.

## Inventory

1. `generic_full_file_regeneration`
   - Symptom: oversized `ModifyFile` hunk such as `service.rs 1-700`
   - Current status: guarded by bounded hunk synthesis and surfaced in telemetry if detected

2. `old_todo_trait_template`
   - Symptom: `// TODO: define required methods`
   - Current status: replaced by `generate_interface_trait_source(...)`; telemetry fails closed if literal reappears

3. `pre_expansion_target_override_short_circuit`
   - Symptom: explicit target collapses companion interface files into the target file
   - Current status: `build_apply_resolutions()` reuses the shared `ChangeSet.patches` stream and preserves companion resolutions

4. `crate_unaware_import_rewrite`
   - Symptom: crate root / sibling interface imports drift across crates
   - Current status: crate-aware rewrite remains in the shared lowering path; no report-only rewrite remains

5. `report_only_change_reprojection`
   - Symptom: JSON/report assembly diverges from apply/change-set output
   - Current status: `build_apply_resolutions()` now consumes `CodeChangeSet` instead of rebuilding from external patch slices

## Telemetry Contract

Top-level `CodingReport.telemetry` is the legacy drift gate.

- `legacy_lowering_path_used`
- `legacy_lowering_usage_count`
- `legacy_paths`

Expected steady state: zero usage.
