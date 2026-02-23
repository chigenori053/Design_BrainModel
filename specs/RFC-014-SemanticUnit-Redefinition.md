# RFC-014: SemanticUnit 役割・構造の再定義

## Purpose
設計シーケンス（RFC-013）において、「大枠（L1）」と「詳細（L2）」の役割を明確に分離し、それぞれの階層で保持すべき情報を再定義することで、設計プロセスの解像度向上をシステム的にサポートする。

## Interfaces

### HybridVM / semantic_dhm
- `fn create_l1_framework(input: &str) -> SemanticUnitL1`
- `fn derive_l2_detail(l1_id: L1Id) -> SemanticUnitL2`
- `fn update_l2_with_grounding(l2_id: L2Id, knowledge: &str)`

## Data Structures

### SemanticUnitL1 (Framework)
- `title`: 仕様の見出し。
- `objective`: 設計意図。
- `scope`: 対象範囲（In/Out）。
- `l2_refs`: 詳細ユニットへの参照リスト。

### SemanticUnitL2 (Detail)
- `parent_id`: 親L1への参照。
- `metrics`: 具体的な数値・定量的要件。
- `methods`: 実装手段・技術スタック。
- `grounding_data`: 外部知識（WebSearch等）のサマリー。

## Logic
1. **L1 Extraction**: ユーザーの自由入力から、独立した「設計項目（Framework）」を複数抽出する。
2. **L2 Derivation**: 各 L1 に対し、具体化が必要な項目をスキャンし、空の詳細ユニット（L2）を紐付ける。
3. **Hierarchy Sync**: L2 が更新された際、その内容が L1 の `objective` や `scope` と逸脱していないかを常に監視する。

## Success Criteria
- [ ] L1 ユニットが「仕様の大枠」として人間が理解しやすいタイトルと目的を持つこと。
- [ ] L2 ユニットが特定の L1 に明確に所属し、詳細情報を保持できること。
- [ ] 既存の `ConceptUnitV2` との互換性、または移行パスが定義されていること。
