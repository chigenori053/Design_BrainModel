# 未検証懸念事項レポート（Phase別 / design_brain_model 配下）

## Phase 0/基盤（全フェイズ共通）
- 共有ミュータブルデフォルトによる状態汚染の可能性
  - 想定影響: VM/状態オブジェクト間で履歴や候補が混線し、結果が非決定的になる。
  - 検証観点: 新規インスタンスを複数生成し、片方の state 変更が他方に影響しないことを確認。
  - 対象: `design_brain_model/hybrid_vm/control_layer/state.py:19`, `design_brain_model/hybrid_vm/control_layer/state.py:47`, `design_brain_model/hybrid_vm/control_layer/state.py:51`, `design_brain_model/hybrid_vm/control_layer/state.py:81`, `design_brain_model/hybrid_vm/control_layer/state.py:82`, `design_brain_model/hybrid_vm/control_layer/state.py:120`, `design_brain_model/hybrid_vm/control_layer/state.py:123`, `design_brain_model/hybrid_vm/control_layer/state.py:134`, `design_brain_model/hybrid_vm/control_layer/state.py:137`

- グローバル VM による並行アクセス時の状態競合の可能性（PoC 設計のため要確認）
  - 想定影響: 複数リクエストの状態が混線し、決定履歴や出力が壊れる。
  - 検証観点: 並行リクエスト（同時 USER_INPUT）を複数発行し、state が汚染されないか確認。
  - 対象: `design_brain_model/hybrid_vm/interface_layer/api_server.py:21`

## Phase 1（Semantic Unit / イベント処理）
- `EXECUTION_RESULT` がイベントループで処理されず拡張時に取りこぼす可能性
  - 想定影響: 実行結果の統合処理を追加しても呼ばれず、ログや状態反映が欠落する。
  - 検証観点: EXECUTION_RESULT 用のハンドラ追加後にイベントが通るか確認。
  - 対象: `design_brain_model/hybrid_vm/core.py:91`, `design_brain_model/hybrid_vm/core.py:262`

## Phase 2（Decision Pipeline）
- `external_evaluations` の参照共有で呼び出し元リストが変更される可能性
  - 想定影響: 呼び出し側の評価リストが意図せず増える、テストの副作用が出る。
  - 検証観点: 同一リストを渡した後に元リスト内容が変更されないことを確認。
  - 対象: `design_brain_model/hybrid_vm/control_layer/decision_pipeline.py:94`

## Phase 3（Consensus / Human Override / Reevaluation）
- Human Override の decision が反映されず常に高ユーティリティ評価になる可能性
  - 想定影響: REJECT でも ACCEPT 相当になるなど、人間判断が無効化される。
  - 検証観点: HUMAN_OVERRIDE で REJECT/ACCEPT を投げ、consensus_status と explanation が期待通り変化するか確認。
  - 対象: `design_brain_model/hybrid_vm/control_layer/human_override.py:15`

- 再評価リクエストが再評価処理へ到達せず別イベントに流れる可能性
  - 想定影響: 再評価フローが実行されず、同じ結果が繰り返し返る。
  - 検証観点: REQUEST_REEVALUATION が想定ハンドラに届くこと、lineage が付くことを確認。
  - 対象: `design_brain_model/hybrid_vm/interface_layer/api_server.py:131`
