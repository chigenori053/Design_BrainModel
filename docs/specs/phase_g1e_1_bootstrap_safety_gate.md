# PhaseG1e.1 仕様書
## Self-Hosting Resolver Bootstrap Safety Gate

**Document ID:** DBM-CLI-SPEC-PHASE-G1E.1
**Status:** Codex Ready
**Priority:** Critical Bootstrap Safety
**Owner:** DBM CLI Coding Pipeline

---

## 1. Background
Self-hosting dry-run for:

apps/cli/src/source_index.rs

still emits planner patches that are invalid during resolver bootstrap.

Observed invalid patches:
- IntroduceInterface
- MoveDependency
- ImportRebinding

Observed failure:
crate root guessed imports are emitted without export proof.

Example:
use crate::agent_domain_interface;
use crate::adapter_app_interface::AdapterAppInterface;

This causes deterministic E0432 unresolved imports.

---

## 2. Goal
When self-hosting target is:

apps/cli/src/source_index.rs

planner MUST enter ResolverSelfHost bootstrap mode.

This mode MUST prevent:
- interface introduction
- dependency moves
- workspace symbol guessing
- import rebinding
- crate root guessed imports

---

## 3. Target Files
- apps/cli/src/coding.rs
- apps/cli/tests/integration/coding_target_scope.rs

---

## 4. Functional Requirements

### 4.1 Bootstrap Safety Policy
Add:

```rust
enum BootstrapSafetyPolicy {
    Normal,
    ResolverSelfHost,
}
```

Resolution rule:

```rust
Some(path) if path.ends_with("source_index.rs")
=> ResolverSelfHost
```

### 4.2 Hard Planner Gate

Before patch planning:

```rust
if policy == ResolverSelfHost {
    patch_plan.retain(|patch| {
        !matches!(
            patch.action,
            PatchAction::IntroduceInterface { .. }
            | PatchAction::MoveDependency { .. }
        )
    });

    planner.disable_import_rebinding();
    planner.disable_workspace_symbol_guess();
}
```

### 4.3 Allowed Rewrite Scope

ResolverSelfHost allows ONLY:

LocalEdit
same-file helper append
test append
same-file function edits
same-file struct edits

All other rewrite kinds MUST be rejected.

5. Forbidden Patch Types

The following MUST NOT appear in dry-run JSON:

IntroduceInterface
MoveDependency
CreateInterface
ImportRebinding
workspace fallback
crate root guessed interface imports
6. Tests

Add integration tests:

source_index_bootstrap_blocks_introduce_interface
source_index_bootstrap_blocks_move_dependency
source_index_bootstrap_disables_import_rebinding
source_index_bootstrap_same_file_local_edit_only
7. Success Criteria

This command MUST succeed in dry-run:

design_cli coding . \
  --target apps/cli/src/source_index.rs \
  --check --json

Required:

ModifyFile only = source_index.rs
no IntroduceInterface in patches
no ImportRebinding in diff
no E0432 unresolved crate root imports

---

# Codex 実行コマンド
次は **対象ファイル固定 + test gate 付き**で Codex に実行させます。

```bash
codex --full-auto "
Edit only:
- apps/cli/src/coding.rs
- apps/cli/tests/integration/coding_target_scope.rs

Implement docs/specs/phase_g1e_1_bootstrap_safety_gate.md exactly.

Run:
cargo test -p design_cli coding_target_scope -- --nocapture
"
```

Codex の patch workflow はこのような single failure class を deterministic test で潰すケースに非常に強いです。

修正後の再検証

Codex patch 後に必ずこれを再実行します。

design_cli coding . \
  --target apps/cli/src/source_index.rs \
  --check --json
合格条件

次の 3 条件を満たせば G1e.1 完了です。

patches から IntroduceInterface が消える
execution.diff から ImportRebinding が消える
ModifyFile が source_index.rs のみ
次フェーズ

ここを越えると次は本来の resolver 改良に戻れます。

G1e.1 bootstrap safety gate
G1e existing-export-only resolver
G1f cross-file symbol promotion safety
