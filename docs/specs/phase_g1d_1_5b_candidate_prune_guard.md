# PhaseG1d-1.5b 仕様書
## ExplicitTarget Candidate Prune Guard
**Document ID:** DBM-CLI-SPEC-PHASE-G1D-1.5B
**Status:** Review Ready
**Priority:** Bootstrap Blocker
**Owner:** DBM CLI Coding Pipeline

---

## 1. Background
Current `design_cli coding --target <file>` still allows
candidate generation to create cross-crate fallback candidates.

Observed failure:

```text
patch_scope_violation:
target file apps/cli/src/coding.rs
!= generated patch path
crates/runtime/runtime_vm/src/adapter_app_interface.rs
```

This indicates the candidate generation layer still emits
runtime_vm fallback candidates before PatchScope fencing.

## 2. Goal

When `PatchScope::ExplicitTargetOnly` is active,
candidate generation results MUST be pruned so that only
candidates whose path exactly matches the explicit target remain.

This is a bootstrap-layer mechanical guard.

## 3. Target Files
- `apps/cli/src/coding.rs`
- `apps/cli/tests/integration/coding_target_scope.rs`

## 4. Functional Requirements
### 4.1 Candidate Prune Guard

Immediately after candidate generation:

```rust
if matches!(options.patch_scope, PatchScope::ExplicitTargetOnly) {
    if let Some(target) = options.target.as_ref() {
        candidates.retain(|c| c.path == *target);
    }
}
```

### 4.2 Hard Rule

The following MUST be true:

```rust
candidate.path == explicit_target
```

All non-matching candidates MUST be dropped before patch planning.

### 4.3 Forbidden Fallbacks

The prune guard MUST eliminate:

- `runtime_vm` fallback
- `adapter_app_interface` synthesis
- cross-crate symbol expansion
- external trait stub generation
- `source_index` workspace fallback

## 5. Safety Requirements

This guard executes BEFORE patch generation.

It MUST prevent:

- `CreateFile` outside explicit target
- `ModifyFile` outside explicit target
- `mod.rs` registration drift
- `Cargo` dependency synthesis

## 6. Test Requirements

Add / update integration test:

- `apps/cli/tests/integration/coding_target_scope.rs`

Required cases:

- `explicit_target_prunes_runtime_vm_candidate`
- `explicit_target_drops_cross_crate_candidates`
- `patch_scope_violation_no_longer_occurs_for_same_file`

## 7. Success Criteria

The following MUST no longer fail:

```bash
design_cli coding . \
  --target apps/cli/src/coding.rs \
  --check
```

Expected:

```text
ModifyFile apps/cli/src/coding.rs only
```

MUST NOT produce:

```text
crates/runtime/runtime_vm/src/adapter_app_interface.rs
```

---

# Codex 実行コマンド
次は **Codex に対象ファイルを完全固定して実行**します。

```bash
codex --full-auto "
Edit only:
- apps/cli/src/coding.rs
- apps/cli/tests/integration/coding_target_scope.rs

Implement docs/specs/phase_g1d_1_5b_candidate_prune_guard.md exactly.
Run:
cargo test -p design_cli coding_target_scope -- --nocapture
"
```

Codex は file-scoped patch + test loop に非常に強いので、この bootstrap patch は通しやすいです。

## 成功後の次工程

これが通れば self-hosting に戻せます。

次の順序:

1. `G1d-1.5b` Codex bootstrap prune
2. `G1d-1.5` planner suppression
3. `G1d-2` locality bias
4. `G1a-0` slash isolation

## 最終判断

今回は Codex で `coding.rs` の candidate prune guard を bootstrap patch するのが最短正解です。

ここを越えると、再び `design_cli` 単独で `planner.rs` 修正が通る可能性が高くなります。
