# RFC-010: 生成的設計誘発 (Generative Design Provocation)

## Purpose
ユーザーの不完全な入力に対し、システム開発の定石（KnowledgeStore）を基にした「仮の設計案」をシミュレーションによって生成し、設計者に具体的な Yes/No の判断を仰ぐことで、設計の具体化を高速化する。

## Interfaces

### HybridVM / design_reasoning
- `fn generate_drafts(&self) -> Vec<DesignDraft>`
- `fn commit_draft(&mut self, draft_id: &str)` : 提案を採用し、実ユニットに変換。

### GUI (G3-D)
- グラフ上の「Ghost Node」表示。
- 提案内容の言語化表示（「〜と仮定すると、安定性が 15% 向上します」）。

## Data Structures

```rust
pub struct DesignDraft {
    pub draft_id: String,
    pub added_units: Vec<SemanticUnitL1V2>,
    pub stability_impact: f64,
    pub prompt: String, // 誘発文
}
```

## Logic
1. **Trigger**: `analyze` 完了時に `stability_score` が閾値以下、または `MissingInfo` が存在する場合に発動。
2. **Synthesis**: `KnowledgeStore` から関連トピック（例：Security, Scaling）を連想し、それらを `role: Constraint` または `role: Optimization` の L1 として仮想的に注入。
3. **Evaluation**: シミュレーター（RFC-004）を呼び出し、注入後のスコアが向上するかを確認。
4. **Presentation**: スコアが向上する組み合わせを「設計案」としてパッケージ化し、言語化エンジンで説明文を生成。

## Success Criteria
- [ ] 「ダッシュボードを作りたい」という入力に対し、「権限管理機能を追加した設計案」が自動で生成されること。
- [ ] 生成された案を採用（Adopt）すると、実際に L1 ユニットが増加し、安定性が向上すること。
- [ ] GUI 上で、確定した要件とシステム提案の要件が視覚的に区別できること。
