# MemorySpace v2 Extension Specification

Additional Components for Cognitive Memory

- Version: 1.0
- Status: Implementation Ready

## 1. Purpose

本仕様は MemorySpace v2 の最小実装に対して不足している以下の機能を追加する。

追加対象:

- Memory Algebra（記憶結合演算）
- Memory Index（検索高速化）
- API拡張（ScoredCandidate）
- Queryモデル

これにより MemorySpace を

Vector Storage
↓
Cognitive Memory System

へ進化させる。

## 2. Scope

対象クレート

- memory_space_complex
- memory_space_recall
- memory_space_api

新規追加クレート

- memory_space_index

## 3. Memory Algebra

MemorySpace の核心機能。

ホログラフィック記憶は以下の演算を持つ。

- bind
- unbind
- superpose

### 3.1 bind

2つの記憶を結合する。

数式

H = A ⊙ B

Hadamard product

Rust API

```rust
pub fn bind(
    a: &ComplexField,
    b: &ComplexField
) -> ComplexField
```

### 3.2 unbind

結合された記憶から元の要素を取り出す。

数式

A ≈ H ⊙ B*

Rust API

```rust
pub fn unbind(
    bound: &ComplexField,
    key: &ComplexField
) -> ComplexField
```

### 3.3 superpose

複数記憶の重ね合わせ。

数式

H = Σ H_i

Rust API

```rust
pub fn superpose(
    memories: &[ComplexField]
) -> ComplexField
```

## 4. Memory Index

MemorySpaceの検索高速化。

現状

O(N)

将来

O(log N)

### 4.1 New Crate

memory_space_index

directory

```text
memory_space_index
 ├─ src
 │   ├─ lib.rs
 │   ├─ index.rs
 │   ├─ cluster.rs
 │   └─ search.rs
```

### 4.2 MemoryIndex Trait

```rust
pub trait MemoryIndex {

    fn insert(
        &mut self,
        memory: MemoryField
    );

    fn search(
        &self,
        query: &ComplexField,
        k: usize
    ) -> Vec<MemoryCandidate>;
}
```

### 4.3 Initial Implementation

v2では単純実装

- LinearIndex

将来

- HNSW
- Spectral Index
- Cluster Index

## 5. API Extension

現状の問題

query() -> Vec<RecallScore>

memory_id が失われる。

### 5.1 ScoredCandidate

```rust
pub struct ScoredCandidate {

    pub memory_id: MemoryId,

    pub resonance: f64,

    pub score: f64,

    pub confidence: f64,

    pub ambiguity: f64,
}
```

### 5.2 API変更

旧

Vec<RecallScore>

新

Vec<ScoredCandidate>

## 6. Query Model

ReasoningAgent 接続用。

### 6.1 MemoryQuery

```rust
pub struct MemoryQuery {

    pub vector: ComplexField,

    pub context: Option<ComplexField>,

    pub k: usize,
}
```

### 6.2 API

```rust
pub fn query(
    &self,
    query: MemoryQuery
) -> Vec<ScoredCandidate>
```

## 7. Determinism Rules

MemorySpaceは決定論的でなければならない。

条件

- stable ranking
- deterministic sort
- no randomness

同率処理

- memory_id ascending

## 8. Tests

追加テスト

Algebra Tests

- bind/unbind inverse test
- superpose stability

Index Tests

- search determinism
- search accuracy

API Tests

- query result determinism

## 9. Implementation Order

推奨順

- 1 memory algebra
- 2 scored candidate
- 3 memory query
- 4 memory index

## 10. Directory Changes

追加構造

```text
crates/

memory_space_core
memory_space_complex
memory_space_recall
memory_space_eval
memory_space_api
memory_space_index
```

## 11. Success Criteria

実装成功条件

- bind/unbind correctness
- deterministic recall
- API compatibility
- index ready

## 12. Future Extensions

v3

- multimodal memory
- tensor memory
- optical simulation

## 13. Final Objective

MemorySpace v2 は

Cognitive Memory Substrate

として以下を提供する。

- associative recall
- memory algebra
- deterministic retrieval
- reasoning integratio
