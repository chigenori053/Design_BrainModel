# Design_BrainModel

> An AI-native architecture reasoning system that generates, evaluates, and refines system architectures from natural language requirements — without relying on external LLMs.
>
> 自然言語の要件からシステムアーキテクチャを生成・評価・精緻化する AI ネイティブ設計推論システムです。外部 LLM に依存しません。

---

## Overview / 概要

Design_BrainModel is a Rust workspace that implements a complete pipeline for **architecture generative AI**:

1. **Natural language input** → Phase9 pipeline (HybridVM + BeamSearch)
2. **Architecture search** → deterministic candidate ranking via ParetoFront
3. **Code generation** → Rust source files from CodeIR
4. **Evaluation** → multi-axis scoring (structural / dependency / constraint / simulation)
5. **Reverse analysis** → infer architecture from existing source code

All operations are **deterministic**: identical inputs always produce identical outputs (FNV-1a hash-based).

---

## Repository Structure / リポジトリ構成

```
Design_BrainModel/
├── apps/
│   ├── arch_gen/       # Architecture Generative AI CLI  ← main user-facing tool
│   ├── cli/            # General CLI frontend
│   ├── desktop/        # Desktop application
│   ├── gui/            # GUI frontend
│   ├── lsp/            # Language Server Protocol integration
│   └── server/         # HTTP server frontend
│
├── crates/             # ~68 library crates (core engine)
│   ├── brain_core/                 # Central reasoning core
│   ├── runtime_vm/                 # HybridVM execution engine
│   ├── world_model/                # World model and state management
│   ├── world_model_core/           # WorldState, EvaluationVector
│   ├── design_search_engine/       # BeamSearch + ParetoFront
│   ├── code_ir/                    # CodeIR — architecture intermediate representation
│   ├── code_language_core/         # Source code parsing and round-trip
│   ├── architecture_reasoner/      # Reverse architecture inference
│   ├── architecture_evaluator/     # Multi-axis architecture scoring
│   ├── math_reasoning_engine/      # Symbolic and numerical reasoning
│   ├── simulation_scheduler/       # Simulation pipeline scheduling
│   └── agent_core/                 # Agent interaction layer
│
├── examples/
│   └── requirements/   # Sample requirement text files for arch-gen
│
└── docs/
    └── architecture/   # Design documents and implementation plans
```

---

## Quick Start / クイックスタート

The primary user-facing tool is **`arch-gen`**, the Architecture Generative AI CLI.

```bash
# Build
cargo build --release -p arch_gen

# Generate architecture candidates from a requirement
./target/release/arch-gen generate "Design a scalable e-commerce platform"
./target/release/arch-gen generate "ECサイトをスケーラブルに設計してください"

# Generate from a requirements file
./target/release/arch-gen generate @examples/requirements/ecommerce.txt -f markdown

# Scan existing source code to infer architecture
./target/release/arch-gen scan ./src -f mermaid

# Interactive design session
./target/release/arch-gen interactive
```

See [`apps/arch_gen/README.md`](apps/arch_gen/README.md) for the full command reference.

---

## Key Features / 主な特徴

| Feature | Description |
|---------|-------------|
| **No external LLM** | All NL processing handled internally by the Phase9 pipeline |
| **Deterministic** | FNV-1a hash-based search guarantees identical output for identical input |
| **Multi-format output** | text / json / mermaid / markdown / plantuml |
| **Reverse analysis** | Infer architecture from existing Rust source code via `scan` |
| **Interactive REPL** | Iterative design refinement with `interactive` command |
| **Output strategies** | new / merge / overwrite / dry-run for flexible code generation |
| **External integration** | stdin / env vars / `--git-add` / `--open` |

---

## Architecture Pipeline / アーキテクチャパイプライン

```
Natural Language Input
        │
        ▼
┌───────────────────────────────────────────────────┐
│  Phase9 Pipeline                                  │
│  RuntimeHybridVm (Reasoning mode)                 │
│    → Phase9RuntimeAdapter                         │
│    → SimpleHypothesisGenerator                    │
│    → DeterministicWorldModel                      │
│    → BeamSearchController                         │
│    → rank_candidates (ParetoFront)                │
└───────────────────────────────────────────────────┘
        │
        ▼  RankedCandidate[]
┌───────────────────────────────────────────────────┐
│  CodeIR Pipeline                                  │
│  ArchitectureState                                │
│    → arch_state_to_architecture()                 │
│    → DeterministicArchitectureToCodeIR            │
│    → DeterministicCodeGenerator                   │
└───────────────────────────────────────────────────┘
        │
        ▼  SourceTree / design.json
   OutputFormatter
   (text / json / mermaid / markdown / plantuml)
```

---

## Build & Test / ビルドとテスト

```bash
# Build the entire workspace
cargo build

# Build arch-gen release binary
cargo build --release -p arch_gen

# Run all arch-gen tests (unit + integration)
cargo test -p arch_gen

# Run unit tests only
cargo test -p arch_gen --lib

# Run integration tests only
cargo test -p arch_gen --test integration_test

# Run the full workspace test suite
cargo test
```

**Current test status:** 68 tests (56 unit + 12 integration) — all passing.

---

## Sample Requirements / サンプル要件ファイル

Ready-to-use requirement files for `arch-gen generate @<file>`:

| File | Description |
|------|-------------|
| [`examples/requirements/ecommerce.txt`](examples/requirements/ecommerce.txt) | E-commerce platform (auth, inventory, orders, payments) |
| [`examples/requirements/microservices_api.txt`](examples/requirements/microservices_api.txt) | Microservices REST API with gateway and message queue |
| [`examples/requirements/simple_webapp.txt`](examples/requirements/simple_webapp.txt) | Simple web app (SPA + REST API + PostgreSQL) |
| [`examples/requirements/event_driven.txt`](examples/requirements/event_driven.txt) | Event-driven system (Kafka + CQRS + event sourcing) |

---

## Documentation / ドキュメント

| Document | Description |
|----------|-------------|
| [`apps/arch_gen/README.md`](apps/arch_gen/README.md) | arch-gen full command reference (EN/JA) |
| [`docs/architecture/`](docs/architecture/) | Architecture design documents and implementation plans |
| [`DESIGN.md`](DESIGN.md) | Core design policy (hash algorithm, determinism gate, API freeze) |

---

## License / ライセンス

See the LICENSE file for details.
