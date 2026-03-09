# ReasoningAgent Architecture Specification

DesignBrainModel Cognitive Engine

- Version: 1.0
- Status: Implementation Ready

## 1. Purpose

ReasoningAgent は DesignBrainModel の推論エンジンであり、MemorySpace を利用して 想起優先推論（Recall-First Reasoning） を実行する。

基本原理:

Recall First
↓
Compute if necessary

つまり

memory retrieval
→ solution reuse
→ exploration only when needed

## 2. Cognitive Loop

ReasoningAgent の基本ループ

Input
 ↓
Perception
 ↓
Memory Recall
 ↓
Hypothesis Generation
 ↓
Simulation / Lookahead
 ↓
Evaluation
 ↓
Action / Output

## 3. System Architecture

User Input
   │
   ▼
Perception Layer
   │
   ▼
ReasoningAgent
   │
   ├── MemorySpace (recall)
   │
   ├── Hypothesis Generator
   │
   ├── Simulator
   │
   └── Evaluation Engine

MemorySpace は System-1

ReasoningAgent は System-2

として機能する。

## 4. Crate Structure

新規 crate

crates/reasoning_agent

ディレクトリ

```text
reasoning_agent
 ├─ src
 │   ├─ lib.rs
 │   ├─ agent.rs
 │   ├─ perception.rs
 │   ├─ hypothesis.rs
 │   ├─ simulator.rs
 │   ├─ evaluation.rs
 │   └─ types.rs
```

## 5. Core Types

ReasoningInput

```rust
pub struct ReasoningInput {

    pub semantic_vector: ComplexField,

    pub context: Option<ComplexField>,

}
```

Hypothesis

```rust
pub struct Hypothesis {

    pub action_vector: ComplexField,

    pub predicted_score: f64,

}
```

ReasoningResult

```rust
pub struct ReasoningResult {

    pub solution_vector: ComplexField,

    pub confidence: f64,

}
```

## 6. Agent Structure

```rust
pub struct ReasoningAgent<M: MemoryIndex> {

    memory: MemoryEngine<M>,

}
```

## 7. Reasoning Pipeline

Step 1 — Perception

入力を内部表現へ変換

Input
↓
Semantic Vector

Step 2 — Memory Recall

MemoryQuery

```rust
let memories = memory.query(query);
```

Step 3 — Recall Threshold

Recall強度を判定

if resonance > θ

成功なら

reuse memory

Step 4 — Hypothesis Generation

Recall失敗時

generate candidate actions

Step 5 — Lookahead Simulation

各 hypothesis を評価

simulate future state

Step 6 — Evaluation

評価指標

- goal distance
- consistency
- memory alignment

## 8. Hypothesis Generator

state → candidate actions

例

- symbolic transform
- structural rewrite
- parameter adjustment

Rust API

```rust
pub fn generate_hypotheses(
    state: &ComplexField
) -> Vec<Hypothesis>
```

## 9. Simulator

未来状態を推定

state + action → next_state

Rust API

```rust
pub fn simulate(
    state: &ComplexField,
    action: &Hypothesis
) -> ComplexField
```

## 10. Evaluation Engine

候補評価

```rust
pub fn evaluate(
    state: &ComplexField
) -> f64
```

## 11. Decision Policy

最終選択

argmax(score)

## 12. Integration with MemorySpace

MemorySpace の利用

- recall
- context binding
- associative search

## 13. Determinism Requirement

ReasoningAgent も決定論的である必要がある。

禁止

- random sampling
- stochastic search

## 14. Performance Strategy

Lookahead 深さ

1〜3 steps

理由

exponential explosion

## 15. Minimal Implementation Plan

最初に実装するもの

- agent.rs
- types.rs
- perception.rs

次

memory recall integration

最後

hypothesis + simulator

## 16. Initial Minimal Agent

最初は

Recall Only Agent

にする。

つまり

memory query
→ best match
→ output

## 17. Future Extensions

v2

- planning
- MCTS
- policy learning

## 18. Expected Role

ReasoningAgent は

Memory
+
Search
+
Evaluation

を統合する。

## 19. Final Cognitive Model

DesignBrainModel

MemorySpace
   │
   ▼
ReasoningAgent
   │
   ▼
Action / Code Generation

次に重要になる設計

次の段階で必要になるのは

Perception Engine

です。

つまり

text
code
diagram
math

を

ComplexField

へ変換する部分です。

これは

DesignBrainModel の 入力知覚層になります。
