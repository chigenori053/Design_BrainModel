# MemorySpace Refactor Specification v2

DesignBrainModel / MemorySpace Architecture Refactoring Plan

- Version: 1.0
- Status: Approved for Implementation

## 1. Purpose

本仕様書は MemorySpace の構造的欠陥を解消するための全面的アーキテクチャ再設計を定義する。

既存実装には以下の問題が存在する。

- 記憶構造と検索ロジックの責務混在
- 複素数演算の境界不明確
- 想起結果の評価（real evaluation）未分離
- 推論層とのインターフェース曖昧
- 拡張性不足（multimodal / reasoning integration）

これらを解決するために MemorySpace v2 を新規構築する。

## 2. Refactoring Strategy

本リファクタリングは 破壊的変更 (Breaking Refactor) である。

ただし安全性確保のため Legacy Freeze Strategy を採用する。

### 2.1 Strategy

```text
Legacy Code
   ↓ freeze
MemorySpace v1 (read only)

New Implementation
   ↓
MemorySpace v2
```

- 旧コードは削除しない。
- 最終段階でのみ削除する。

## 3. Repository Refactoring Rules

### 3.1 Do Not Delete

以下は削除してはならない。

- tests/
- .github/
- docs/
- *.md
- Cargo.toml

理由

- CI維持
- テスト資産保持
- 仕様履歴保持

### 3.2 Freeze Legacy Code

既存MemorySpaceは以下へ移動する。

- crates/memory_space_legacy/

- 変更禁止
- read-only policy

## 4. MemorySpace v2 Architecture

MemorySpaceは以下の5層構造に分割する。

MemorySpace v2

- memory_space_core
- memory_space_complex
- memory_space_recall
- memory_space_eval
- memory_space_api

## 5. Module Responsibilities

### 5.1 memory_space_core

基本データ構造を定義する。

Responsibilities

- 型定義
- trait定義
- invariants

Example

```rust
pub trait MemoryStore {
    fn store(&mut self, memory: MemoryField);

    fn recall(&self, query: MemoryField) -> Vec<MemoryCandidate>;
}
```

### 5.2 memory_space_complex

複素演算処理

Responsibilities

- complex vector
- normalization
- interference
- holographic encoding

Example

- ComplexField
- ComplexTensor
- PhaseEncoding

### 5.3 memory_space_recall

共鳴検索

Responsibilities

- resonance calculation
- similarity
- ranking

Resonance intensity

```text
I = | q ⋅ m* |
```

### 5.4 memory_space_eval

実数評価

Complex Recall → Real Score

Responsibilities

- score normalization
- ambiguity estimation
- confidence scoring

Example

- RecallScore
- Confidence
- Ambiguity

### 5.5 memory_space_api

外部インターフェース

Responsibilities

- Reasoning Engine 接続
- Agent 接続
- Multimodal接続

## 6. Mathematical Model

MemorySpace v2は以下の数理構造を前提とする。

### 6.1 Memory Representation

```text
H ∈ ℂ^D
```

- D = spectral dimension

### 6.2 Resonance Recall

```text
I_i = | q ⋅ m_i* |
```

共鳴強度で検索順位決定。

### 6.3 Real Evaluation

Recall結果を実数評価へ変換する。

```text
score = f(resonance, ambiguity, memory_density)
```

## 7. Directory Structure

最終構造

```text
crates/

memory_space_core
memory_space_complex
memory_space_recall
memory_space_eval
memory_space_api

memory_space_legacy
```

## 8. Implementation Phases

### Phase 1

Architecture Skeleton

実装

- core
- complex

Goal

- complex memory representation

### Phase 2

Recall Engine

実装

- resonance search
- ranking

### Phase 3

Evaluation Engine

実装

- confidence
- ambiguity
- real scoring

### Phase 4

Integration

接続

- ReasoningAgent
- MultimodalEngine
- DesignBrainModel

## 9. Test Strategy

テストは以下に分類する。

Structural Tests

- determinism
- normalization
- memory invariants

Mathematical Tests

- resonance correctness
- complex norm preservation

Recall Tests

- ranking stability
- tie-breaking

Integration Tests

- reasoning pipeline

## 10. CI Policy

CIでは以下を検証する。

- cargo check
- cargo test
- cargo clippy
- cargo fmt

## 11. Migration Plan

### Step1

freeze legacy

- memory_space → memory_space_legacy

### Step2

create new crates

- memory_space_core
- memory_space_complex
- memory_space_recall
- memory_space_eval
- memory_space_api

### Step3

minimal implementation

### Step4

test validation

### Step5

switch production

### Step6

remove legacy

## 12. Success Criteria

リファクタリング成功条件

- deterministic recall
- architecture separation
- extensible memory layer
- stable CI

## 13. Risks

主要リスク

- resonance instability
- complex overflow
- memory explosion

対策

- normalization
- bounded memory

## 14. Final Goal

MemorySpace v2 は以下を満たす。

- AI memory substrate
- reasoning integration
- multimodal support
- deterministic recall

## 15. Implementation Authorization

この仕様書に基づき

MemorySpace v2 refactoring

の実装を開始する。
