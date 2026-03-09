# DesignBrainModel Reasoning Capability Enhancement Specification

- Version: 1.0
- Phase: R1 (Reasoning Enhancement)
- Date: 2026-03-07

## 1. 目的

- Reasoning quality improvement
- Search stability improvement
- Concept-Intent integration

制約:
- dynamic dimension scaling は実装しない
- `FieldConfig` インターフェースのみ導入

## 2. 変更対象

- `concept_engine`
- `concept_field`
- `search_controller`
- `reasoning_agent`
- `runtime_vm`

## 3. 新推論フロー

```text
IntentGraph
↓
ConceptGraph activation
↓
ConceptField
↓
Memory recall
↓
SearchController
↓
ReasoningAgent
↓
Evaluation
```

## 4. 実装内容

### 4.1 Concept activation
- `ActivationEngine { propagation_steps, decay }` を追加
- Intent concept seeds から ConceptGraph へ活性伝播
- 伝播式: `A(t+1) = decay * Σ neighbor(A(t))`

### 4.2 ConceptField 拡張
- `FieldConfig` を追加
  - `coarse_dim = 0`
  - `medium_dim = 0`
  - `reasoning_dim = 1024`
- `build_field(concepts, registry)` を追加
- weighted superpose + normalize で Field 構築

### 4.3 SearchController v2
- `SearchConfig` default を更新
  - beam_width: 5
  - max_depth: 4
  - pruning_threshold: 0.25
- heuristic を重み付き化
  - `0.5 * memory_resonance + 0.3 * concept_match + 0.2 * intent_alignment`

### 4.4 runtime_vm 統合
- pipeline 順序を更新
  - `SemanticAgent -> ConceptAgent -> IntentAgent -> ConceptActivationAgent -> ConceptFieldAgent -> MemoryAgent -> SearchControllerAgent -> ReasoningRuntimeAgent -> EvaluationAgent`
- `RuntimeContext` 拡張
  - `intent_nodes`
  - `concept_activation`
  - `concept_field`
  - `memory_candidates`
  - `search_state`

## 5. テスト

追加:
- `crates/concept_engine/tests/concept_activation_propagation.rs`
- `crates/search_controller/tests/beam_search_determinism.rs`
- `crates/search_controller/tests/search_pruning_effectiveness.rs`
- `crates/search_controller/tests/intent_alignment_effect.rs`
- `crates/runtime_vm/tests/pipeline_integration.rs`

## 6. 実行コマンド

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=1
```

## 7. 完了基準

- All CI tests pass
- Search determinism confirmed
- Concept activation affects reasoning

