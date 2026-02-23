# RFC-013: インクリメンタル・カード設計プロトコル (ICDP) v2.0

## Purpose
設計の「大枠（L1）」から「詳細（L2）」への肉付けを、外部知識（WebSearch）の自動統合と、カード型 UI による直感的な編集によって完遂させるプロセスを定義する。

## The Streamlined 6-Step Sequence
1. **Initial Input**: ユーザーが抽象的なアイディアを入力。
2. **Framework Generation**: システムが L1 フレームワーク（カードの骨格）を生成。
3. **Explicit Detail**: ユーザーが特定のカードに対して具体的な仕様を追記。
4. **Knowledge Grounding (Gate)**: システムが知識不足を判定し、ユーザーの許可を得て WebSearch を実行。結果を L2 の `grounding_data` に自動反映。
5. **Visual Cardification**: GUI 上で各設計項目を「仕様カード」として提示。
6. **In-place Refinement**: ユーザーがカード上で L2 詳細を最終調整し、設計を確定させる。

## Interfaces

### New Interfaces (Requirement Step 4 & 6)

### CLI / API
- `design search --card <CARD_ID>` : 特定カードの不足知識を補うための検索を実行（許可制）。
- `design refine --card <CARD_ID> --text <DETAIL_TEXT>` : 特定カードの詳細（L2）を直接更新。

### GUI (G3-E)
- **Interactive Cards**: クリックで詳細エディタが開くカード UI。
- **Grounding Badge**: WebSearch によって知識が補完されたカードに表示されるバッジ。

## Data Structures

- `CardId`: `L2-<u64>` 形式の識別子。
- `SemanticUnitL2Detail`:
  - `id`
  - `parent_id`
  - `metrics`
  - `methods`
  - `grounding_data`
- `Session grounding/refinement state`:
  - `l2_grounding: Vec<(u64, Vec<String>)>`
  - `l2_refinements: Vec<(u64, Vec<String>)>`

## State Transitions (if applicable)

- `search --card ... --allow`:
  - gap 判定 `true` のとき `grounding_data` に検索結果を反映。
  - gap 判定 `false` のとき更新なし。
- `refine --card ... --text ...`:
  - 対象カードに詳細追記し、L2再構築を実行して `stability_score` を再計算。

## Constraints

- Search は明示的許可 (`--allow`) がない限り実行しない。
- Card編集は deterministic に再計算されること。
- 既存の analyze/explain/adopt/reject の互換性を維持する。

## Success Criteria
- [ ] ユーザーの「許可」をトリガーとして、WebSearch の結果が特定の L2 詳細ユニットに自動的に流し込まれること。
- [ ] GUI 上で特定のカードを選択し、その詳細（L2）を編集・保存できること。
- [ ] L2 の更新によって、全体の `stability_score` が再計算されること。
