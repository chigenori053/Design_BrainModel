# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Check, build, format, lint
cargo check --workspace --locked
cargo build --workspace --locked
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings

# Fast local test cycle (Phase1 target crates only)
cargo test -p agent_core
cargo test -p design_cli

# Heavy tests (behind feature flag)
cargo test -p agent_core --features ci-heavy
cargo test -p design_cli --features ci-heavy

# Specific test suites via the CI script
bash scripts/run_ci_test_suite.sh invariants
bash scripts/run_ci_test_suite.sh engine
bash scripts/run_ci_test_suite.sh knowledge_engine
bash scripts/run_ci_test_suite.sh determinism     # must run --test-threads=1
bash scripts/run_ci_test_suite.sh integration
bash scripts/run_ci_test_suite.sh all

# Determinism tests must be run single-threaded
cargo test -p design_search_engine --test determinism --locked -- --test-threads=1

# Property tests
cargo test -p memory_space --lib proptest_props --locked
cargo test -p field_engine --lib proptest_props --locked
cargo test -p recomposer --lib proptest_props --locked

# Python linting
ruff check .

# Run the CLI
cargo run -p design_cli --bin design -- analyze
cargo run -p design_cli --bin design -- explain
cargo run -p design_cli --bin design -- simulate
```

The CI pipeline runs in five sequential tiers: Build → Tier1 (Invariants/Engine) → Tier2 (Math Properties) → Tier3 (Integration) → Tier4 (Stress) → Tier5 (Determinism). The nightly run adds `--release` and `cargo audit`.

## Architecture Overview

This is a Rust workspace (`edition = "2024"`, stable toolchain) implementing a design intelligence system — a "brain model" for reasoning about software architecture. The system takes design inputs and produces ranked architecture candidates via multi-objective search.

### Memory Hierarchy (CHM / DHM / SHM)

Three complementary memory layers, each in its own crate, unified behind the `hybrid_vm` facade:

- **SHM** (`crates/shm`): Structural/transformation rules. Holds `DesignRule` entries with `precondition`, `Transformation`, and `EffectVector` (delta_struct, delta_field, delta_risk, delta_cost). `Shm::with_default_rules()` is the standard entry point.
- **DHM** (`crates/dhm`): Design holographic memory. Stores `DhmRecord`s keyed by depth — objective vectors at each search depth.
- **CHM** (`crates/chm`): Causal edge memory. Stores `CausalEdge` relationships between rules (from_rule, to_rule, strength).

**`hybrid_vm`** (`crates/hybrid_vm`) is the central facade that combines CHM, DHM, SHM together with `language_dhm`, `semantic_dhm`, `recomposer`, `meaning_extractor`, and `design_reasoning`. All higher layers must go through `hybrid_vm` — never access CHM/DHM/SHM directly from `agent_core` or the CLI.

### Core Types

`crates/core_types` defines the fundamental value types shared across the workspace:
- `ObjectiveVector` — four-dimensional score: `f_struct`, `f_field`, `f_risk`, `f_shape`
- `ProfileVector` — weight vector for scoring objectives
- `DesignUnit`, `ClassNode`, `StructureNode`, `LayerKind`, `DesignIR`

### Search and Evaluation

- **`design_search_engine`** (`crates/engine/design_search_engine`): Core beam search. `BeamSearchController` is the main entry point. `SearchConfig` controls `max_depth`, `beam_width`, `experience_bias`, `policy_bias`.
- **`agent_core`** (`crates/agent_core`): Runs the Phase1 search matrix loop (`run_phase1_matrix`). Contains `ParetoFront`, `BeamSearch`, `Phase45Controller`, and the `Orchestrator`/`Dispatcher`/`AgentRegistry` runtime.
- **`evaluation_engine`** (`crates/evaluation_engine`): `EvaluationEngine` scoring across dependency, performance, cost, and complexity dimensions.
- **`policy_engine`** (`crates/policy_engine`): `SearchPolicy` (action weights + pattern weights), `PolicyStore`, pattern generalization.
- **`memory_space_phase14`** (`crates/memory_space`): Experience store, pattern store, search priors used to bias beam search.
- **`search_verification`** (`crates/search_verification`): Pre-built test states (rest_api, layered, microservice) used in integration tests.

### Language and Semantic Layers

- **`semantic_dhm`**: `SemanticUnitL1`, `SemanticUnitL2`, `ConceptUnit`, `MeaningLayerSnapshot`, `MeaningLayerSnapshotV2`. L1 = atomic semantic units; L2 = integrated/composed view.
- **`language_dhm`**: Language-specific DHM with `LanguageUnit` and `LangId`.
- **`recomposer`**: Recombines design components; must only depend on `semantic_dhm`, not `language_dhm`, `shm`, or `chm`.
- **`design_reasoning`**: `Phase1Engine`, `HypothesisEngine`, `LanguageEngine`, `MeaningEngine`, `SnapshotEngine`, `ProjectionEngine`.

### Runtime Stack

- **`runtime_core`** (`crates/runtime/runtime_core`): `Phase9RuntimeContext` and `RuntimeStage` enum (Input → Normalize → Recall → HypothesisGeneration → Search → Simulation → Evaluation → Ranking → Output). Defines port traits: `DecisionPolicy`, `ReasoningEngine`, `MemoryRecallEngine`, `MultimodalEncoder`, `GeometryEvaluator`, `LanguageRenderer`.
- **`runtime_vm`** (`crates/runtime/runtime_vm`): `HybridVm`, `Pipeline`, `PipelineRuntime`, `Phase9RuntimeAdapter`. Orchestrates the full agent pipeline.
- **`ai_context`** (`crates/ai_context`): `AIContext` aggregate — holds `ArchitectureState`, `SemanticGraph`, `KnowledgeGraph`, `ExperienceState`, `EvaluationState`, `RuntimeState`.
- **`world_model_core`** (`crates/core/world_model_core`): `WorldModel`, `DeterministicWorldModel`, `WorldState`, `Hypothesis`, `HypothesisGenerator`.

### Domain Layer

`crates/domain/` contains pure domain types:
- `design_domain`: `Architecture`, `DesignUnit`, `Dependency`, `Layer` (Ui/Service/Repository/Database)
- `architecture_domain`: `ArchitectureState`, metrics, deployment model
- `causal_domain`, `semantic_domain`, `design_grammar`: domain-specific types

### Applications

- `apps/cli` (`design_cli`): Main CLI with `design` binary. Commands: `analyze`, `explain`, `simulate`, and more. Depends only on `hybrid_vm` (never on `semantic_dhm`, `recomposer`, or `language_dhm` directly).
- `apps/server` (`design_server`): HTTP server; thin wrapper over `interface_ui`.
- `apps/gui`, `apps/desktop`, `apps/lsp`: GUI, desktop, and LSP frontends.

## Critical Invariants (Enforced in CI)

These are checked by the `tier1_unit` boundary rules step and will break CI if violated:

1. **`chm` must not depend on `shm`** — no `shm v` in `cargo tree -p chm` output
2. **`agent_core` must not directly use `shm`, `chm`, `dhm`, `language_dhm`, or `semantic_dhm`** — all layer access goes through `hybrid_vm`
3. **`recomposer` must only depend on `semantic_dhm`** — no `language_dhm`, `shm`, `chm`, or `meaning_extractor` imports in `crates/recomposer/src`
4. **CLI must only depend on `hybrid_vm`** — no `semantic_dhm`, `recomposer`, or `language_dhm` in `apps/cli/src` or `apps/cli/tests`
5. **`agent_core` must not call `Shm::new` or `Chm::new` directly**

## Determinism Requirements

The system is fully deterministic — the same input must always produce identical results:
- **Snapshot format**: `MeaningLayerSnapshotV2` is canonical. Comparison uses only `l1_hash`, `l2_hash`, `version`; `timestamp_ms` is log-only and excluded from diffs.
- **Hash algorithm**: FNV-1a 64-bit, seed `0xcbf29ce484222325`, prime `0x100000001b3`, deterministic UTF-8 byte order. Floats are formatted with `{:.6}`, list fields sorted before hashing, `Option` encoded as `null`/`some:<value>`.
- **Template selection**: `TEMPLATE_SELECTION_EPSILON = 1e-6` prevents ambiguity.
- Determinism tests must run with `--test-threads=1`.

## API Conventions (PhaseA-Final)

V2 APIs are the default. V1 APIs are deprecated since `PhaseA-Final` and will be removed in PhaseC. Always use:
- `HybridVM::snapshot_v2` (not `snapshot`)
- `HybridVM::compare_snapshots_v2`, `explain_design_v2`, `get_l1_unit_v2`, `all_l1_units_v2`, `rebuild_l2_from_l1_v2`, `project_phase_a_v2`

## Spec-Driven Workflow

All changes require an approved spec:

1. Create a spec file in `specs/` using the `specs/_TEMPLATE.md` structure (Purpose, Interfaces, Data Structures, State Transitions, Constraints, Acceptance Criteria).
2. Get Architect approval — update `state/TASK_STATE.yaml` with `architect_approved: true`.
3. Implement against the spec.
4. Mark status `review` then `completed` after validation.

Task status transitions: `proposed → approved → in_progress → review → completed`. No skipping steps. Technical debt must be recorded immediately in `state/TASK_STATE.yaml` under `technical_debt`.

The `specs/` RFC files (RFC-001, RFC-003, etc.) document significant feature designs. The `state/ARCH_DECISIONS.yaml` and `state/CHANGELOG.yaml` track architectural history.
