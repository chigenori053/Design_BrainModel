# Phase9-C Memory x WorldModel Integration

- Date: 2026-03-08
- Scope: `memory_space_core`, `runtime_core`, `runtime_vm`, `world_model_core`, `apps/cli`

## Goal

MemorySpace を runtime の正式な前段に統合し、Recall-First Reasoning を成立させる。

## Fixed Flow

`Input -> Normalize -> Recall -> HypothesisGeneration -> TransitionEvaluation -> ConsistencyEvaluation -> Output`

## Contracts

### MemorySpace

- `RecallQuery`
- `RecallConfig`
- `RecallResult`
- `RecallCandidate`
- `MemoryRecord`
- `MemoryStore`
- `MemoryEngine`

### Runtime

`Phase9RuntimeContext` に `recall_result` を追加し、MemorySpace 本体は保持しない。

### WorldModel

`HypothesisGenerator::generate(state, recall)` で recall result を seed として使う。

## Verification

- `cargo check --workspace`
- `cargo test -p memory_space_core -p world_model_core -p runtime_vm`
- `cargo run -p design_cli --bin design -- phase9 --input "Phase9-C recall check"`
