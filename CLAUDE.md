# CLAUDE.md — AI Assistant Guide for Design_BrainModel

This file provides context for AI assistants (Claude Code, Codex, Gemini CLI, etc.) working in
this repository. Read this before making any changes.

---

## Project Overview

**Design_BrainModel** is a research system for architectural design reasoning and optimization,
written primarily in Rust. It models software architecture as a semantic memory space, performs
multi-objective design search (Pareto optimization), and reasons over causal/semantic graphs.

The system has evolved through a series of numbered phases (Phase 1–22+), each adding capabilities.
The current stable baseline is **PhaseA-Final** (see `DESIGN.md`).

---

## Repository Structure

```
Design_BrainModel/
├── apps/                    # 5 binary applications
│   ├── cli/                 # Main CLI (binaries: design, cli, phase1_batch)
│   ├── gui/                 # GUI application
│   ├── desktop/             # Desktop application
│   ├── server/              # Server application
│   └── lsp/                 # Language Server Protocol implementation
├── crates/                  # 68+ Rust library crates (workspace members)
│   ├── core_types/          # Shared primitive types
│   ├── core/                # Core abstractions (memory_space_core, world_model_core)
│   ├── domain/              # Domain models (design, semantic, architecture, causal, grammar)
│   ├── engine/              # Search engine (design_search_engine)
│   ├── runtime/             # Runtime orchestration (runtime_core, runtime_vm)
│   ├── hybrid_vm/           # Unified VM interface over all layers
│   ├── agent_core/          # Core agent implementation
│   ├── memory_space/        # Main memory abstraction (phase-versioned)
│   ├── dhm/, shm/, chm/     # Hierarchical memory models
│   └── ...                  # See Cargo.toml for full list
├── specs/                   # RFC and architecture specifications
│   ├── RFC-*.md             # Feature RFCs
│   ├── architecture/        # Phase architecture specs (phase10–12)
│   ├── engine/              # Engine specs (tensor_engine, etc.)
│   ├── Memo/                # Design memos (Japanese)
│   └── ui/                  # UI state machine specs
├── tests/                   # Integration and property-based tests
├── tools/                   # Python validation scripts
├── scripts/                 # Shell scripts (CI, task management, incident recording)
├── docs/                    # Documentation (testing.md, human_coherence)
├── report/                  # Phase benchmark and validation reports (JSON)
├── state/                   # Runtime state management
├── runtime/                 # Runtime artifacts and incidents
├── .github/workflows/       # CI pipeline definitions
├── AGENT.md                 # Agent governance charter
├── DESIGN.md                # PhaseA-Final snapshot policy and API freeze list
└── Rule.md                  # System rules for structural integrity and AI collaboration
```

---

## Governance — Read First

This project uses a **specification-first governance model**. The authority order is:

```
specs/  >  Rule.md  >  TASK_STATE.yaml  >  Implementation Code
```

**Never modify implementation code without a corresponding spec reference.**

### Agent Role Boundaries

| Role | Allowed | Not Allowed |
|------|---------|-------------|
| **ResearchAgent** (Gemini CLI) | Propose specs, draft alternatives | Write production code, modify implementations |
| **CodingAgent** (Codex CLI / Claude) | Implement approved specs, refactor, write tests | Redefine architecture, skip spec step |
| **ValidationAgent** | Verify spec-implementation alignment, run checks | Modify code |
| **Architect** (Human) | Approve specs, resolve conflicts | — |

### Change Protocol

1. Create a proposal in `specs/`
2. Architect approves
3. Update `TASK_STATE.yaml` to `approved`
4. CodingAgent implements
5. ValidationAgent reviews
6. Mark `completed`

---

## Build System

**Language:** Rust (edition 2024), Cargo workspace
**Toolchain:** Stable (see `rust-toolchain.toml`)
**Resolver:** Version 2

```bash
# Check entire workspace compiles
cargo check --workspace --locked

# Build all
cargo build --workspace

# Build specific crate
cargo build -p agent_core

# Build CLI binary
cargo build -p cli --bin design
```

---

## Testing

### Local development (fast feedback)

```bash
# Fast suite — Phase1 core crates only
cargo test -p agent_core
cargo test -p design_cli
```

### Heavy tests (behind feature flag)

```bash
cargo test -p agent_core --features ci-heavy
cargo test -p design_cli --features ci-heavy
```

### Full CI suite

```bash
cargo test --all-features
```

### Test categories (used in CI)

| Category | Purpose |
|----------|---------|
| `invariants` | Structural and mathematical invariants |
| `engine` | Search, evaluation, memory correctness |
| `knowledge_engine` | Knowledge retrieval and reasoning |
| `determinism` | Reproducibility and idempotency |
| `integration` | End-to-end pipeline tests |
| `experiments` | Research tests (`#[ignore]`, not run in standard CI) |

Tests marked `#[ignore]` are experimental and should **not** be run unless explicitly requested.

---

## Code Quality

```bash
# Format (must pass in CI)
cargo fmt --all

# Lint (zero warnings policy in CI)
cargo clippy --workspace -- -D warnings

# Python linting (for scripts/tools/)
ruff check .
```

---

## CI Pipeline (5 Tiers)

Defined in `.github/workflows/ci.yml`. Each tier blocks the next.

| Tier | Name | Key checks |
|------|------|-----------|
| 0 | Build | `cargo check`, `fmt`, `clippy`, secret scan (gitleaks), Python lint |
| 1 | Unit Tests | Boundary rules, invariants, engine, knowledge_engine suites |
| 2 | Math Tests | Property-based tests (proptest): memory_space, field_engine, recomposer, Pareto |
| 3 | Integration | End-to-end pipeline coverage |
| 4 | Stress/Scale | `cargo test --release --features ci-heavy` |
| 5 | Determinism | Reproducibility validation |

Additional workflows: `research.yml` (experimental), `nightly.yml` (extended suite).

---

## Architecture Conventions

### Layer Isolation (enforced by CI boundary rules)

Layers must only depend downward. Cross-layer direct access is **forbidden**.

```
apps/             ← top layer, may depend on any crate
  └── agent_core / hybrid_vm   ← orchestration
        └── engines / memory   ← processing
              └── domain/       ← domain models
                    └── core_types  ← primitives
```

- No circular dependencies
- Memory layer interfaces are **immutable without a version bump**
- State machine changes require a version increment

### Determinism Requirements (from `DESIGN.md`)

All computations must be deterministic:
- Same input → identical `snapshot_v2` hashes, template selection, explanation text
- Hash algorithm: FNV-1a 64-bit, seed `0xcbf29ce484222325`
- Vectors/floats formatted as `{:.6}` before hashing
- Lists sorted before hashing
- Template selection epsilon: `TEMPLATE_SELECTION_EPSILON = 1e-6`

### Current API Status (PhaseA-Final)

V2 APIs are the canonical interface. V1 APIs are deprecated (`since = "PhaseA-Final"`):

| Deprecated | Use instead |
|-----------|------------|
| `HybridVM::snapshot` | `HybridVM::snapshot_v2` |
| `HybridVM::compare_snapshots` | `HybridVM::compare_snapshots_v2` |
| `HybridVM::explain_design` | `HybridVM::explain_design_v2` |
| `HybridVM::get_l1_unit` | `HybridVM::get_l1_unit_v2` |
| `HybridVM::all_l1_units` | `HybridVM::all_l1_units_v2` |
| `HybridVM::rebuild_l2_from_l1` | `HybridVM::rebuild_l2_from_l1_v2` |
| `HybridVM::project_phase_a` | `HybridVM::project_phase_a_v2` |

V1 APIs will be **removed in PhaseC**. Do not introduce new uses.

### Snapshot Format (`MeaningLayerSnapshotV2`)

The canonical snapshot for PhaseA-Final. Comparison keys:
- `l1_hash`
- `l2_hash`
- `version`

`timestamp_ms` is **log-only** and must be ignored in diff/equality judgments.

---

## Specification Files

All specs live in `specs/`. Before implementing any non-trivial feature:

1. Check for an existing RFC or architecture spec
2. If none exists, create one following the standard template (Purpose, Interfaces, Data Structures, State Transitions)
3. Increment spec version on structural changes
4. Move deprecated specs to `specs/deprecated/`

Key specs to reference:
- `specs/architecture/phase10_architecture_spec.md` — foundational layered architecture
- `specs/RFC-020-Pareto-Optimization-Engine.md` — multi-objective optimization
- `specs/engine/tensor_engine_v1.1.md` — tensor computation abstraction
- `DESIGN.md` — snapshot policy and API freeze (PhaseA-Final)

---

## Task State Transitions

Tasks tracked in `TASK_STATE.yaml` follow strict state machine:

```
proposed → approved → in_progress → review → completed
```

- Do **not** skip transitions
- Rollback requires Architect approval
- Agents may only edit `TASK_STATE.yaml` if they are the specified owner of that task

---

## Common Pitfalls for AI Assistants

1. **Do not implement without a spec.** Even small structural changes need a spec reference.
2. **Do not use V1 APIs.** Always use V2 (`snapshot_v2`, `compare_snapshots_v2`, etc.).
3. **Do not break determinism.** Any floating-point output must use `{:.6}` formatting; lists must be sorted before hashing.
4. **Do not cross layer boundaries.** Check dependency direction before adding a `use` or `extern crate`.
5. **Do not add `#[allow(clippy::...)]` without justification.** CI enforces `-D warnings`.
6. **Do not run `#[ignore]` tests** unless explicitly asked — they are experimental.
7. **Do not push to `main` directly.** All changes go through feature branches and CI.
8. **Do not modify `TASK_STATE.yaml`** unless you are the designated owner of that task.

---

## Development Workflow Summary

```bash
# 1. Check out / create a feature branch
git checkout -b feature/my-change

# 2. Make changes (with spec reference)

# 3. Verify code quality
cargo fmt --all
cargo clippy --workspace -- -D warnings

# 4. Run fast local tests
cargo test -p agent_core
cargo test -p design_cli

# 5. Commit with descriptive message referencing the spec
git commit -m "feat(memory_space): implement V2 snapshot comparison per RFC-XXX"

# 6. Push and open PR
git push -u origin feature/my-change
```

---

*Last updated: 2026-03-16*
