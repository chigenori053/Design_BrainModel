# RFC-003: 設計修正案エンジン (Remediation Engine)

## Purpose
Detect bottlenecks from analysis outputs and provide concrete remediation actions for designers.

## Interfaces
- CLI: `design explain --json` includes `data.remediations[]`.
- CLI: `design explain` prints remediation actions in human-readable form.
- GUI extension point: node-level warning badges can consume remediation entries.

## Data Structures
- `remediations[]`
  - `id: String`
  - `target_id: String` (`L1-*` / `L2-*`)
  - `priority: String` (`high` / `medium`)
  - `issue: String` (`high_ambiguity` / `l2_conflict` / `coupling_hub`)
  - `message: String`
  - `action_type: String` (`refine_text` / `resolve_tradeoff` / `split_hub`)

## 1. 背景と目的
現在の診断システム（RFC-002）は設計全体の健康状態を報告するが、具体的な修正箇所や方法までは示さない。
本仕様では、分析データから「ボトルネック」となっている要素を自動特定し、設計者に対して具体的な「改善アクション」を提示するエンジンを定義する。

## 2. 抽出ロジック (Detection Logic)

### 2.1 曖昧性ボトルネック (L1 Ambiguity Hub)
- **判定基準**: `SemanticUnitL1V2.ambiguity_score` > 0.7
- **推奨アクション**: 「要件の具体化（数値化、境界条件の明示）」を提示。

### 2.2 構造的競合 (L2 Conflict)
- **判定基準**: 同一の `ConceptUnitV2` 内で、正負の強度が混在する `DerivedRequirement` が存在する場合。
  - 例：`Performance` (+0.8) と `Memory` (-0.9) が共存し、`stability_score` が低下している。
- **推奨アクション**: 「トレードオフの解消（優先順位の決定、またはモジュールの分離）」を提示。

### 2.3 過剰結合 (Coupling Hub)
- **判定基準**: `ConceptUnitV2.causal_links` の数が平均の 2倍以上。
- **推奨アクション**: 「ハブ機能の分割」を提示。

## 3. インターフェース拡張 (JSON Schema v1.2)
`explain` コマンドの結果に `remediations` 配列を追加する。

```json
"remediations": [
  {
    "id": "REM-001",
    "target_id": "L1-10",
    "priority": "high",
    "issue": "high_ambiguity",
    "message": "要件『セキュリティをいい感じにしたい』が抽象的です。具体的な実装基準を定義してください。",
    "action_type": "refine_text"
  }
]
```

## 4. ユーザー体験 (CLI/GUI)
- **CLI**: `explain` 実行時に「推奨される改善アクション」としてリスト表示。
- **GUI**: グラフ上の該当ノードに警告バッジを表示し、クリックで修正案をポップアップ。

## 5. 検証条件 (Success Criteria)
- [ ] 曖昧な入力に対して、特定の L1 ID を指した修正案が出力されること。
- [ ] 競合する要件（高可用性 vs 省メモリ等）に対して、トレードオフの指摘が出ること。
- [ ] `explain --json` で v1.2 スキーマに準拠したデータが返ること。
