# Phase20-C Design Document: Design Simulation
# Phase20-C 設計書：デザイン・シミュレーション

## 1. Overview / 概要
This phase implements the "Design Simulation" capability for the DesignBrainModel. It allows the agent to predict the impact of a proposed design change without actually modifying the system state (L2, Code, or Memory).
本フェーズでは、DesignBrainModel に「デザイン・シミュレーション」機能を実装します。これにより、エージェントはシステムの状態（L2、コード、メモリ）を実際に変更することなく、提案された設計変更の影響を予測できるようになります。

## 2. Core Concepts / 中核概念
### 2.1 SimulationContext (Shadow World) / シミュレーション・コンテキスト（影の世界）
A transient execution environment where the "what-if" scenario is evaluated. It is strictly isolated from the production environment.
「もし〜なら」のシナリオを評価するための一時的な実行環境です。本番環境からは厳格に分離されています。

### 2.2 Shadow L2 / シャドウ L2
A temporary design space composed of the current L2 state plus the proposed `DraftUnits`.
現在の L2 状態に提案された `DraftUnits` を加えた、一時的な設計空間です。

## 3. Implementation Details / 実装詳細
### 3.1 State Machine / 状態遷移
- New State: `SIMULATING`
- Allowed Transitions: `PROPOSING -> SIMULATING -> RESPONDING`
- 新規状態：`SIMULATING`
- 許可される遷移：`PROPOSING -> SIMULATING -> RESPONDING`

### 3.2 Simulation Engine / シミュレーション・エンジン
- Location: `design_brain_model/brain_model/co_design_kernel/simulation.py`
- Functions:
    - `simulate(proposal)`: Executes the prediction logic.
    - Structural integrity check (mocked for Phase 20-C).
    - Dependency break detection.
- 配置：`design_brain_model/brain_model/co_design_kernel/simulation.py`
- 機能：
    - `simulate(proposal)`: 予測ロジックを実行。
    - 構造的整合性チェック（Phase 20-C ではモック）。
    - 依存関係の破壊検出。

## 4. Constraints / 制約事項
- **Read-Only**: Production L2 and code must remain unchanged.
- **No Code Generation**: Predicts structural changes but does not generate implementation.
- **No Self-Evaluation**: The agent provides data; the human makes the decision.
- **読み取り専用**: 本番の L2 およびコードは変更されないこと。
- **コード生成なし**: 構造的な変化は予測するが、実装コードは生成しない。
- **自己評価なし**: エージェントは材料を提供し、決定は人間が行う。

## 5. Verification / 検証
Verification is performed using `tests/test_phase20_simulation.py`, ensuring isolation and correct issue detection.
`tests/test_phase20_simulation.py` を使用して、分離性および正しい問題検出を確認します。
