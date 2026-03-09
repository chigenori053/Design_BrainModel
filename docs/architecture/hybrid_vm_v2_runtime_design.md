# BrainModel Runtime Architecture: HybridVM v2

- Date: 2026-03-07
- Scope: `crates/runtime_vm`
- Goal: HybridVM を domain logic から分離し、execution runtime に限定する

## 1. 設計目標

HybridVM は以下を満たす:
- Domain Layer と Runtime Layer の責務分離
- Agent パイプラインの差し替え可能性
- ExecutionMode による実行戦略切り替え

## 2. レイヤー定義

### Domain Layer
- SemanticEngine
- ConceptEngine
- MemorySpace
- IntentGraph
- ReasoningAgent

### Runtime Layer
- HybridVM
- ExecutionScheduler
- PipelineRuntime
- RuntimeContext

## 3. Runtime 構造

```text
HybridVM
 ├ ExecutionScheduler
 ├ PipelineRuntime
 └ RuntimeContext
```

## 4. Agent インターフェース

```rust
pub trait Agent {
    fn execute(&mut self, ctx: &mut RuntimeContext);
}
```

## 5. RuntimeContext

```rust
pub struct RuntimeContext {
    pub input_text: String,
    pub semantic_units: Vec<SemanticUnit>,
    pub concepts: Vec<ConceptId>,
    pub intent_graph: Option<IntentGraph>,
    pub memories: Vec<ConceptRecallHit>,
    pub hypotheses: Vec<RuntimeHypothesis>,
    pub tick: u64,
}
```

## 6. パイプライン

標準推論パイプライン:

```text
input text
↓
SemanticAgent
↓
ConceptAgent
↓
IntentAgent
↓
MemoryAgent
↓
ReasoningAgent
↓
EvaluationAgent
```

## 7. ExecutionMode

```rust
pub enum ExecutionMode {
    Analysis,
    Reasoning,
    Simulation,
}
```

切替:
- `Analysis` -> 解析中心 pipeline
- `Reasoning` -> 記憶・推論含む pipeline
- `Simulation` -> 仮説展開中心 pipeline

## 8. 実装ファイル構成

```text
crates/runtime_vm/src/
├ agent.rs
├ scheduler.rs
├ pipeline.rs
├ runtime_context.rs
├ execution_mode.rs
├ runtime.rs
└ lib.rs
```

## 9. 拡張性

本設計により以下を VM 変更最小で追加可能:
- Agent追加/削除
- Pipeline変更
- ExecutionMode追加
- 将来の parallel execution / GPU scheduling / distributed runtime

## 10. 設計本質

- Brain = Agents
- Mind = Pipeline
- Body = Runtime

