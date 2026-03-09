# DesignBrainModel SearchController & HybridVM Integration Specification

- Version: 1.0
- Date: 2026-03-07

## 1. 目的

本仕様は以下を実装する:
- SearchController による探索制御
- HybridVM runtime pipeline 統合
- ConceptField 導入
- runtime_vm 残タスクの実装着手
- ディレクトリ構造の明確化

## 2. 新推論パイプライン

```text
Input
↓
SemanticAgent
↓
ConceptAgent
↓
ConceptFieldAgent
↓
IntentAgent
↓
MemoryAgent
↓
SearchControllerAgent
↓
ReasoningRuntimeAgent
↓
EvaluationAgent
```

## 3. 新規 crate

- `crates/search_controller`
- `crates/concept_field`

## 4. SearchController

### Config

```rust
pub struct SearchConfig {
    pub beam_width: usize,      // default 5
    pub max_depth: usize,       // default 4
    pub pruning_threshold: f64, // default 0.2
}
```

### SearchState

```rust
pub struct SearchState {
    pub state_vector: ComplexField,
    pub score: f64,
    pub depth: usize,
}
```

### 機能
- beam search
- pruning (`score < threshold`)
- depth control
- heuristic scoring (`memory + concept + intent`)

## 5. ConceptField

### 型

```rust
pub struct ConceptField {
    pub vector: ComplexField,
}
```

### 生成

```rust
pub fn build_field(concepts: &[ConceptVector]) -> ConceptField
```

内部は concept vectors の superposition を行う。

## 6. RuntimeContext 拡張

```rust
pub struct RuntimeContext {
    pub semantic_units: Vec<SemanticUnit>,
    pub concepts: Vec<ConceptId>,
    pub concept_field: Option<ConceptField>,
    pub memory_candidates: Vec<ConceptRecallHit>,
    pub search_state: Option<SearchState>,
}
```

## 7. runtime_vm 追加要素

- `SearchControllerAgent`
- `ConceptFieldAgent`
- `AgentRegistry`
- `pipeline.json`（pipeline config サンプル）

## 8. ConceptGraph activation 拡張

`concept_engine/activation` を追加:
- `propagation.rs` (spread activation)
- `scoring.rs` (top-k ranking)

## 9. CI / Test コマンド

```bash
cargo test -p search_controller
cargo test -p concept_field
cargo test -p runtime_vm
cargo test --workspace
```

## 10. Roadmap

- Phase1: SearchController + ConceptField + runtime integration
- Phase2: ConceptGraph activation強化
- Phase3: ANN memory index

