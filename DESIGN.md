# Design_BrainModel PhaseA-Final

## Snapshot V2 Fixed Policy
- `MeaningLayerSnapshotV2` is the canonical snapshot format for PhaseA-Final.
- Comparison keys are only:
  - `l1_hash`
  - `l2_hash`
  - `version`
- `timestamp_ms` is log-only and ignored in diff judgment.

### Hash policy (fixed)
- Algorithm: FNV-1a 64bit
- Seed: `0xcbf29ce484222325`
- Prime: `0x100000001b3`
- Byte order: deterministic UTF-8 byte sequence
- Canonicalization:
  - vectors and floating point values are string-formatted with fixed precision (`{:.6}`)
  - list-like fields are sorted before hashing
  - `Option` values are encoded as `null` or `some:<value>`
  - empty string and `None` are distinct

## API Freeze and Deprecation
PhaseA-Final promotes V2 APIs as default.

Deprecated APIs are marked with:
- `since = "PhaseA-Final"`
- `note = "Will be removed in PhaseC"`

Examples:
- `HybridVM::snapshot` -> use `HybridVM::snapshot_v2`
- `HybridVM::compare_snapshots` -> use `HybridVM::compare_snapshots_v2`
- `HybridVM::explain_design` -> use `HybridVM::explain_design_v2`
- `HybridVM::get_l1_unit` -> use `HybridVM::get_l1_unit_v2`
- `HybridVM::all_l1_units` -> use `HybridVM::all_l1_units_v2`
- `HybridVM::rebuild_l2_from_l1` -> use `HybridVM::rebuild_l2_from_l1_v2`
- `HybridVM::project_phase_a` -> use `HybridVM::project_phase_a_v2`

## Determinism Gate
- Same input must produce identical:
  - `snapshot_v2` hashes
  - template selection
  - explanation text
- Template ambiguity epsilon is fixed:
  - `TEMPLATE_SELECTION_EPSILON = 1e-6`
