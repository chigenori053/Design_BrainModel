# RFC-006: 設計情報欠落抽出・対話プロトコル (G3-C)

## Purpose
分析された設計構造から「論理的に不足している情報」を特定し、設計者に対して具体的な問いかけ（プロンプト）を行うことで、設計の解像度を段階的に高める対話的基盤を定義する。

## Interfaces

### HybridVM / design_reasoning
- `fn extract_missing_information(&self) -> Vec<MissingInfo>`
- `MissingInfo` 構造体: カテゴリ、対象ID、プロンプト文、重要度を保持。

### CLI / JSON Schema v1.4
- `explain` コマンドの出力に `missing_info` 配列を統合。

## Data Structures

```rust
pub enum InfoCategory {
    Constraint,
    Boundary,
    Metric,
    Objective,
}

pub struct MissingInfo {
    pub target_id: Option<L1Id>,
    pub category: InfoCategory,
    pub prompt: String,
    pub importance: f64,
}
```

## Logic
1. **L1制約チェック**: `constraints` が空の L1 ユニットをスキャン。
2. **抽象度相関**: 抽象度が高い（>0.6）かつ、特定のキーワード（「開発したい」「構築する」等）を含むユニットに対し、具体的手段や制約を問うプロンプトを生成。
3. **トレードオフ未解決チェック**: 競合が発生している L2 において、優先順位（Priority）が明示されていない場合に「どちらを優先するか」を問う。

## Success Criteria
- [ ] 「〜を開発したい」という入力に対し、「制約は何ですか？」といった問いかけが生成されること。
- [ ] 抽出された問いかけが GUI のアドバイザー・パネルに表示されること。
- [ ] 重要度（Importance）に基づいて問いかけがソートされていること。
