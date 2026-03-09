# Phase9-B Runtime Contract Stabilization

- Date: 2026-03-08
- Scope: `runtime_core`, `world_model_core`, `runtime_vm::adapter`, `apps/cli`

## Goal

Phase9-A で追加した vertical slice を、Phase9-B では正式契約へ固定する。

## Fixed Contracts

### Runtime Contract

`Phase9RuntimeContext` は以下の shared state のみに限定する。

- `request_id`
- `modality_input`
- `world_state`
- `hypotheses`
- `evaluation`
- `stage`
- `event_bus`

Agent 内部状態は context に含めない。

### Modality Input

`ModalityInput` は以下で固定する。

- `Text(String)`
- `Image(ImageBuffer)`
- `Audio(AudioBuffer)`
- `Structured(serde_json::Value)`

### Runtime Stage

`RuntimeStage` は以下で固定する。

- `Input`
- `Normalize`
- `Recall`
- `HypothesisGeneration`
- `TransitionEvaluation`
- `ConsistencyEvaluation`
- `Output`

### Event Taxonomy

`RuntimeEvent` は以下で固定する。

- `InputAccepted`
- `ModalityNormalized`
- `MemoryRecallRequested`
- `MemoryRecallCompleted`
- `HypothesisGenerated`
- `TransitionEvaluated`
- `ConsistencyScored`
- `OutputProduced`

### World Model Contract

`world_model_core` は以下で固定する。

- `WorldState { state_id, features }`
- `Hypothesis { hypothesis_id, predicted_state, score }`
- `ConsistencyScore { value }`

## Adapter Rule

`runtime_vm::adapter` は旧 `RuntimeContext` から `Phase9RuntimeContext` への一方向変換のみを提供する。

禁止:

- `Phase9RuntimeContext -> legacy runtime`

## Verification

- `cargo check --workspace`
- `cargo test -p runtime_vm -p world_model_core`
- `cargo run -p design_cli --bin design -- phase9 --input "Phase9-B contract check"`
