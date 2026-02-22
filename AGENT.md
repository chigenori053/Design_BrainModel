AGENT.md
Design_BrainModel – Agent Operational Policy
1. 基本方針 (Core Principles)
1.1 言語要件

すべての対話・説明・設計議論は 日本語 で出力する。

コードコメント・コミットメッセージは原則日本語。

docs/ 内の仕様書・設計書は 日英併記 (Japanese / English) とする。

1.2 レイヤ構造の厳守 (Architectural Integrity)

本プロジェクトは以下のレイヤ構造を厳守する：

lib (Facade)
 ↓
runtime (Execution Control)
 ↓
capability (Algorithms)
 ↓
engine (Pure Computation)
 ↓
domain (Data Model)

I/O境界：

runtime → ports → adapters
禁止事項

lib.rs に実装ロジックを書くこと

capability から I/O を直接呼び出すこと

engine 内で副作用を持つこと

レイヤ逆流

グローバル mutable static

unsafe の乱用（engine 以外禁止）

2. セキュリティ・保安性ポリシー
2.1 セキュア実装原則

unwrap / expect を業務経路で使用しない

panic を本番経路に残さない

JSON 入力制限（サイズ・深さ・配列長）を維持

File I/O は必ず adapters 経由

原子更新 + 排他制御を維持

2.2 計算爆発防止 (Execution Safety)

runtime 層で以下を統制する：

最大探索ノード数

最大ループ回数

最大時間

最大メモリ使用量

engine / capability に直接制御を埋め込まない。

2.3 静的構造監視

CI で以下を検出する：

lib.rs に impl 定義

lib.rs に loop / for / while

lib.rs に数理関数

capability → ports 参照

runtime 以外の std::fs

legacy 文字列

3. 最適化ポリシー
3.1 レイヤ別最適化範囲
最適化種別	許可レイヤ
数理最適化	engine
アルゴリズム改善	capability
実行制御最適化	runtime
I/O最適化	adapters
禁止

最適化のためにレイヤを跨ぐこと

lib.rs にショートカット実装を追加すること

3.2 パフォーマンス監視

ベンチマーク baseline を固定

golden test と整合確認

性能劣化は CI で検出

4. テスト・修正フロー
4.1 原則

まず実装コードを修正

テストコード修正は例外措置のみ

4.2 構造違反は即修正

テストが失敗した場合、構造違反の可能性を最優先で疑う。

5. ドキュメント管理
5.1 docs/ 必須構成

Architecture.md

Security.md

OptimizationPolicy.md

ExecutionBudget.md

Incidents/

5.2 日英併記義務

全設計文書は Japanese / English 両方を記載する。

6. インシデント管理
6.1 重大インシデント定義

以下は重大事故とする：

レイヤ逆流

セキュリティ不備

データ破損

panic停止

DoS的挙動

docs と実装の乖離

6.2 事故対応手順
Step1: 記録

docs/incidents/ に以下形式で記録：

Incident ID:
Date:
Layer:
Category:
Description:
Root Cause:
Impact:
Resolution:
Preventive Measure:
Step2: 原因分析

技術的原因

設計的原因

プロセス的原因

防止できなかった理由

Step3: 再発防止策の制度化

必ず以下のいずれかを追加：

静的ガード追加

テスト追加

レイヤ制約強化

CI監視追加

ドキュメント更新

Step4: AGENT.md 更新

事故内容と再発防止策を AGENT.md に追記する。

7. 不可逆変更ポリシー

以下は慎重審査対象：

public API変更

型削除

レイヤ移動

unsafe導入

並列化導入

変更前に：

docs更新

影響範囲明示

rollback戦略明示

を必須とする。

8. 定期構造監査

定期的に確認：

lib.rs 行数

runtime 肥大化

capability 肥大化

engine 副作用混入

ports 境界違反

9. 定義：健全な状態 (Definition of Structural Health)

以下を満たす状態を健全とする：

lib.rs は Facade のみ

engine は純関数のみ

capability は探索のみ

runtime は統制のみ

adapters は I/O のみ

CI が構造違反を検出可能

10. 最終原則

構造を守ることが最優先
安全は速度より優先
最適化は境界を壊さない範囲で行う
