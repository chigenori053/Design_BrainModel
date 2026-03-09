# DesignBrainModel Phase-R2 DesignSearchEngine Specification

- Version: 1.0
- Date: 2026-03-07
- Phase: R2 (DesignSearchEngine)

## 1. 目的

- architecture hypothesis exploration
- design state search
- constraint guided exploration

DesignBrainModel を reasoning system から design exploration engine へ拡張する。

## 2. 新アーキテクチャ

```text
Intent
↓
Concept Activation
↓
ConceptField
↓
Design State
↓
DesignSearchEngine
↓
Architecture Hypotheses
↓
Evaluation
```

## 3. 新規 crate

- `crates/design_search_engine`
  - `design_state.rs`
  - `hypothesis_graph.rs`
  - `search_strategy.rs`
  - `evaluator.rs`
  - `constraint.rs`
  - `search_config.rs`
  - `engine.rs`

## 4. 主要モデル

- `DesignState { id, design_units, evaluation }`
- `DesignUnit { unit_type, dependencies }`
- `HypothesisGraph { states, edges }`
- `DesignTransition { from, to, operation }`
- `SearchConfig { beam_width=8, max_iterations=20 }`
- `EvaluationScore`:
  - structural
  - dependency
  - concept_alignment
  - total = `0.4*structural + 0.3*dependency + 0.3*concept_alignment`

## 5. Constraint / Determinism

- `ConstraintEngine` が intent nodes から制約判定を実施
- beam search は deterministic sorting により再現可能

## 6. runtime_vm 統合

pipeline 更新:

```text
SemanticAgent
ConceptAgent
IntentAgent
ConceptActivationAgent
ConceptFieldAgent
MemoryAgent
SearchControllerAgent
DesignSearchAgent
EvaluationAgent
```

`RuntimeContext` 拡張:
- `design_state: Option<DesignState>`
- `hypothesis_graph: Option<HypothesisGraph>`

## 7. テスト

- `crates/design_search_engine/tests/design_search_determinism.rs`
- `crates/design_search_engine/tests/constraint_filtering.rs`
- `crates/design_search_engine/tests/evaluation_ranking.rs`
- `crates/design_search_engine/tests/beam_search_selection.rs`

## 8. CI コマンド

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 9. 成功基準

- deterministic design search
- constraint-guided exploration
- architecture hypothesis ranking

