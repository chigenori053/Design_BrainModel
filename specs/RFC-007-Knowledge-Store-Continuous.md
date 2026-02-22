# RFC-007: Holographic Knowledge Store & Continuous Dialogue

## Purpose
1. システム開発の基礎知識をホログラフィック記憶（HKS）として保持し、推論の背景知識（Common Sense）とする。
2. 連続的なテキスト入力に対応し、対話を通じて設計をインクリメンタルに成長させる基盤（CDD）を構築する。

## Interfaces

### HybridVM
- `fn add_knowledge(&mut self, topic: &str, vector: Vec<f32>)` : 定石を記憶。
- `fn analyze_incremental(&mut self, text: &str)` : 既存状態を維持したまま追加分析。
- `fn clear_context(&mut self)` : セッション状態を初期化。

### CLI / GUI
- 連続して `analyze` を呼んだ場合、L1 ユニットが累積して表示されるように変更。

## Data Structures

### KnowledgeStore (New Crate)
```rust
pub struct KnowledgeStore {
    memory: Vec<f32>, // Holographic Superposition
    labels: Vec<String>,
}
```

## Logic

### 1. Holographic Recall (RFC-007)
ユーザーが「教室管理ツール」と入力した際、HKS から「権限管理」「データベース永続化」「ユーザー認証」などの関連概念を連想し、それらが不足している場合に `MissingInfo` として優先的に提示する。

### 2. Cumulative Analysis (RFC-008)
`HybridVM` 内の `semantic_l1_dhm` に新しいユニットを追加し続け、`rebuild_l2` を実行することで、既存概念との結合（Causal Links）を動的に更新する。

## Success Criteria
- [ ] 連続して「要件A」「要件B」を入力した際、両方が L1 ユニット一覧に存在すること。
- [ ] 「開発したい」という入力に対し、HKS に基づく具体的な「システム開発の定石」的な質問（例：DBの選定など）が生成されること。
