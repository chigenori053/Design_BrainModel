Phase16 仕様書

SemanticUnit 生成テストフェーズ

1. Phase16 の目的（Purpose）

Phase16 の目的は、COHERENT / DesignBrainModel において
SemanticUnit が「意味単位」として成立するかを、実装とテストで検証することである。

本フェーズでは SemanticUnit を以下として扱う：

SemanticUnit =
入力を意味的に表現し、記憶・推論・判断の共通対象として再利用可能な最小単位

UI 完成やスキーマ確定は目的としない。

2. Phase15 までの前提条件（Preconditions）

Phase16 は以下が成立していることを前提とする。

AST Generalization により構文依存性が除去されている

テキスト／画像入力がホログラフィック表現（ℂ^1024）へ変換可能

Optical Holographic Memory が安定動作している

Recall-First 推論ループが機能している

confidence / entropy が算出可能

3. Phase16 のスコープ（In Scope）
3.1 実施対象

テキスト入力からの SemanticUnit 生成

画像入力からの SemanticUnit 生成

マルチモーダル入力からの SemanticUnit 生成

SemanticUnit の Recall / 再利用テスト

SemanticUnit 間の距離・共鳴特性の検証

confidence / entropy との接続確認

テキスト会話UI向け 内部生成物としての妥当性検証

3.2 非対象（Out of Scope）

以下は Phase16 では実施しない。

SemanticUnit の最終スキーマ凍結

UI 表示仕様の確定

SemanticUnit の編集・進化・学習最適化

評価指標のチューニング

マルチモーダル重み最適化

4. SemanticUnit の暫定定義（Mutable）

Phase16 における SemanticUnit は 暫定構造とし、以下の性質のみを要求する。

必須性質

意味を表す内部表現を持つこと

Optical Memory に保存・想起可能であること

入力差分に対し連続的に変化すること

confidence / entropy の評価対象になれること

UI による「観測」が可能であること

最小構成要素（概念）

semantic_representation

ホログラフィック表現（ℂ^1024）

structure_signature

AST / Vision スペクトル構造

origin_context

text / vision / multimodal

confidence

entropy

※ スキーマは 凍結しない

5. テスト項目定義（Test Matrix）
5.1 単一入力 SemanticUnit 生成テスト

目的
SemanticUnit が安定的に生成されることを確認する。

入力

数式テキスト

自然言語テキスト

単純図形画像

検証観点

同一入力 → 高共鳴

微差入力 → 距離が連続的に変化

生成失敗が起きない

5.2 Recall / 再利用テスト

目的
SemanticUnit が「意味単位」として再利用可能かを検証する。

テスト

別表現入力 → 既存 SemanticUnit 想起

Recall-First ループでの利用確認

合格条件

Recall 強度が閾値以上

推論ステップ数が削減される

5.3 SemanticUnit 間距離テスト

目的
意味的近接性が数値に反映されるか確認する。

対象

類似テキスト同士

非類似テキスト同士

類似画像 / 非類似画像

検証

共鳴強度分布

entropy の増減傾向

5.4 マルチモーダル SemanticUnit 生成テスト

目的
Binding 後も SemanticUnit が破壊されないことを確認する。

テスト

テキスト + 画像 → 結合

結合後 Recall テスト

合格条件

単一モダリティに崩壊しない

Recall が可能

6. テキスト会話UIとの関係

Phase16 において SemanticUnit は以下の位置づけとする。

テキスト会話UIは SemanticUnit の生成・観測装置

UI は SemanticUnit を編集しない

UI 主導で SemanticUnit を定義しない

会話処理は内部的に以下を満たす必要がある：

Text Input
 → SemanticUnit 生成
 → Reasoning / Recall
 → SemanticUnit 変換
 → Text Output

7. ログ・成果物（Deliverables）

Phase16 完了時に必須な成果物：

SemanticUnit 生成ログ（JSON）

Recall 成功／失敗ケース一覧

confidence / entropy 分布ログ

マルチモーダル生成テスト結果

SemanticUnit を「意味単位」と呼べる技術的根拠

8. Phase17 への引き渡し条件

以下が満たされた場合、Phase17 に進行可能とする。

SemanticUnit が全入力経路で生成可能

Recall に実利用価値がある

confidence / entropy が破綻していない

UI 非依存で成立している

9. Phase16 の設計思想（重要）

Phase16 は 確定フェーズではなく検証フェーズ

SemanticUnit を「UI都合」で固めない

失敗ケースを積極的に収集する
