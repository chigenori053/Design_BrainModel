# DesignBrainModel Phase9 Implementation Plan

- Date: 2026-03-08
- Scope: Phase9 以降の実装フェーズ再定義
- Status: draft v0.1 をコードへ反映した最小実装

## 1. 目的

Phase9 以降を「実装・統合・構造安定化・製品化フェーズ」として扱い、既存コードベースを維持しながら段階的に責務分離を進める。

## 2. 実装原則

- 前進型開発を維持し、機能開発を止めない
- 全面改修ではなく adapter を介した移行を優先する
- crate 間契約を先に安定化する
- 完了条件は `cargo check` / `cargo test` / 最小 CLI 動作確認のいずれかを満たす

## 3. Phase9 で追加した責務境界

### `crates/runtime_core`

Phase9 以降で固定したい契約面を集約する。

- `Phase9RuntimeContext`
- `ModalityEnvelope`
- `RuntimeEventBus`
- `RuntimeAgent`
- `MemoryRecallEngine`
- `ReasoningEngine`
- `DecisionPolicy`
- `LanguageRenderer`
- `GeometryEvaluator`

これにより GUI / CLI / 将来の Server から同じ runtime 契約を共有できる。

### `crates/world_model_core`

WorldModel 責務を HybridVM から切り離す。

- `WorldState`
- `WorldHypothesis`
- `TransitionPrediction`
- `WorldModel`
- `HypothesisGenerator`
- `ConsistencyEvaluator`

既存資料でいう Hypothesis Generator / Lookahead Simulator の責務をこの層に寄せる。

### `crates/runtime_vm::adapter`

既存 `RuntimeContext` と Phase9 契約の橋渡しを担う。

- `Phase9RuntimeAdapter::from_legacy`
- `Phase9RuntimeSnapshot`

既存 pipeline を維持したまま新しい runtime 契約へ移行できる。

## 4. 依存方向

```text
apps/cli -> runtime_vm -> runtime_core
runtime_vm -> reasoning_agent / search_controller / memory_space_*
runtime_core -> memory_space_core / world_model_core
world_model_core -> no UI dependency
memory_space_core -> no UI dependency
```

## 5. モダリティ非依存入力

Phase9 では `ModalityEnvelope` を導入し、以下を同じ受け口に通す。

- text
- image
- audio
- structured

現時点では複素表現への完全写像までは実装していないが、runtime 契約上は modality 非依存の入力境界を確保した。

## 6. CLI 動作確認

`design phase9 --input "Phase9 architecture check"`

このコマンドは以下を確認する。

- 既存 `runtime_vm` pipeline の実行
- legacy context から Phase9 runtime 契約への変換
- `world_model_core` による仮説生成と状態遷移
- Phase9 レポートの JSON 出力

## 7. 次段階

- `ReasoningAgent` を `runtime_core::RuntimeAgent` に段階移行する
- `MemorySpaceCore` と `MemoryRecallEngine` を adapter 経由で接続する
- event bus を GUI / telemetry と統合する
- `RuntimeContext` を apps 側から直接参照しない構成へ縮退させる
