# RFC-011: 意思決定フィードバック学習 (Decision Feedback Learning)

## Purpose
設計者の意思決定（採用・棄却）を学習し、提案エンジンのパーソナライズと適合精度の向上を実現するためのフィードバックループを定義する。

## Interfaces

### HybridVM / knowledge_store
- `fn record_feedback(&mut self, draft_id: &str, action: FeedbackAction)`
- `fn adjust_weights(&mut self)` : 蓄積されたフィードバックに基づき内部重みを更新。

### CLI / JSON Schema v1.5
- `adopt` コマンドの裏側で自動的にフィードバックを記録。
- `reject --draft-id <ID>` コマンドの新設。

## Data Structures

```rust
pub enum FeedbackAction {
    Adopt,
    Reject,
}

pub struct FeedbackEntry {
    pub context_hash: u64,
    pub applied_pattern_id: String,
    pub action: FeedbackAction,
    pub timestamp: u64,
}
```

## Logic
1. **信号の蓄積**: 各意思決定を `FeedbackEntry` として保存。
2. **重み更新アルゴリズム**: 
   - 採用時: 提案に使用されたパターンの `relevance_score` を `+delta` 増強。
   - 棄却時: 同パターンの `relevance_score` を `-delta` 減衰。
3. **提案フィルタリング**: 低スコアのパターンは次回の `generate_drafts` において優先順位を下げ、ノイズを削減する。

## Success Criteria
- [ ] 提案を Reject した後、同種の提案が下位に沈む（または表示されない）こと。
- [ ] 複数のプロジェクトを通じて、設計者の好む「定石」が優先的に提案されること。
- [ ] フィードバックデータがセッション JSON に保存され、再開後も維持されること。
