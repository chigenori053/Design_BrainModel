# DesignBrainModel Phase1 Implementation Specification

- Date: 2026-03-07
- Scope: Cognitive Core
- Objective: 知識表現の安定化

## 1. Phase1 の目的

現状課題:
- SemanticUnit duplication
- Semantic drift
- Reasoning duplication
- Memory recall noise

解決手段:
- Concept Engine を導入し、Concept canonicalization を中核に据える

## 2. 完了条件

- [x] Concept canonicalization が動作する
- [x] SemanticUnit が Concept を参照する
- [x] ConceptGraph が構築可能
- [x] MemorySpace が Concept recall を利用できる
- [x] ReasoningAgent 側で concept bind ユーティリティを利用可能

## 3. 対象コンポーネント

- `concept_engine` (new)
- `semantic_dhm` (semantic_engine 相当の拡張)
- `memory_space_api` (memory_space 相当の拡張)
- `reasoning_agent` (concept bind 補助)

## 4. 追加ディレクトリ

```
crates/concept_engine/src/
├ concept.rs
├ concept_registry.rs
├ concept_graph.rs
├ canonicalizer.rs
├ concept_cluster.rs
└ lib.rs
```

## 5. データモデル

### Concept

```rust
pub struct Concept {
    pub id: ConceptId,
    pub name: String,
    pub embedding: Vec<f32>,
    pub category: ConceptCategory,
}
```

### ConceptId

```rust
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct ConceptId(pub u64);
```

生成方式:
- FNV-1a hash (`normalize_concept_name` 後の文字列)

### ConceptCategory

```rust
pub enum ConceptCategory {
    Component,
    Action,
    Property,
    Constraint,
    Domain,
}
```

## 6. ConceptRegistry

```rust
pub struct ConceptRegistry {
    concepts: HashMap<ConceptId, Concept>,
}
```

提供API:
- `get(&self, id: ConceptId) -> Option<&Concept>`
- `register(&mut self, concept: Concept)`
- `find_similar(&self, embedding: &[f32]) -> Option<ConceptId>`

## 7. Canonicalizer

```rust
pub struct Canonicalizer {
    registry: ConceptRegistry,
}
```

アルゴリズム:
- string normalization
- embedding similarity
- 未登録時に新規Concept生成

API:
- `canonicalize(&mut self, text: &str, embedding: &[f32]) -> ConceptId`

## 8. ConceptGraph

```rust
pub struct ConceptEdge {
    pub source: ConceptId,
    pub relation: RelationType,
    pub target: ConceptId,
}

pub struct ConceptGraph {
    edges: Vec<ConceptEdge>,
}
```

`RelationType`:
- `DependsOn`
- `Optimizes`
- `ConflictsWith`
- `PartOf`

## 9. ConceptCluster

```rust
pub struct ConceptCluster {
    pub domain: String,
    pub concepts: Vec<ConceptId>,
}
```

## 10. SemanticEngine 拡張 (`semantic_dhm`)

追加:
```rust
pub struct SemanticUnit {
    pub concept: concept_engine::ConceptId,
    pub context_vector: Vec<f32>,
}

pub struct SemanticEngine {
    canonicalizer: Canonicalizer,
}
```

パイプライン:
- Text
- Embedding
- Canonicalizer
- Concept
- SemanticUnit

## 11. MemorySpace 拡張 (`memory_space_api`)

追加:
```rust
pub struct MemoryEntry {
    pub concept: ConceptId,
    pub vector: Vec<f32>,
}
```

追加API:
- `recall_concepts(query, top_k)`
- `recall_vectors(concept)`

## 12. ReasoningAgent 影響

`reasoning_agent` に concept bind の基盤として以下を追加:
- `generate_bound_concept_pairs(concepts, max_pairs)`

これにより、`bind(concept_A, concept_B)` の候補生成を deterministic に扱える。

## 13. テスト仕様

### ConceptEngine
- concept canonicalization
- concept similarity
- concept registry insertion
- concept graph integrity

### SemanticEngine
- text → concept mapping
- semantic unit creation

### MemorySpace
- concept recall
- vector recall

### ReasoningAgent
- concept bind pair generation

## 14. 実行コマンド

```bash
cargo test -p concept_engine
cargo test -p semantic_dhm
cargo test -p memory_space_api
cargo test -p reasoning_agent
```

## 15. 次フェーズ

- Phase2: ReasoningAgent 強化
- IntentGraph 完成
- Architecture Grammar

