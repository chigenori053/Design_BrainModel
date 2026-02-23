# RFC-004: 摂動シミュレーションエンジン (Perturbation Engine)

## Purpose
現在の分析は静的（入力された時点の状態）に限定されている。本仕様の目的は、特定の L1 ユニット（要件）のパラメータを仮想的に変動（摂動）させ、その影響が L2 コンセプトの安定性や全体の `ObjectiveVector` にどう波及するかを、実データを書き換えることなくシミュレートする基盤を提供することである。

## Interfaces

### HybridVM API
- `fn simulate_perturbation(&self, target_l1: L1Id, delta_abstraction: f32) -> Result<SimulationReport, SemanticError>`
- `fn simulate_removal(&self, target_l1: L1Id) -> Result<SimulationReport, SemanticError>`

### CLI Command
- `design simulate --target <L1_ID> --delta <VALUE>`
- `design --json simulate ...`

## Data Structures

### Rust Structs
```rust
pub struct SimulationReport {
    pub original_objectives: ObjectiveVector,
    pub simulated_objectives: ObjectiveVector,
    pub affected_concepts: Vec<ConceptImpact>,
}

pub struct ConceptImpact {
    pub concept_id: ConceptId,
    pub original_stability: f64,
    pub simulated_stability: f64,
}
```

### JSON Schema v1.3 (Partial)
```json
{
  "command": "simulate",
  "data": {
    "impact_summary": {
      "stability_delta": 0.05,
      "risk_delta": -0.02
    },
    "affected_concepts": [
      {
        "id": "L2-101",
        "stability_change": -0.1
      }
    ]
  }
}
```

## Logic
1. **仮想状態の構築**: 現行の L1 ユニット群をベースに、指定された摂動を加えた一時的なセットを作成する。
2. **L2 再マッピング**: 一時的な L1 群を用いて `deterministic_grouping` を実行し、仮想的な L2 構造を生成する。
3. **目的関数評価**: 仮想 L2 構造に対して `StructuralEvaluator` を適用し、新しい `ObjectiveVector` を得る。
4. **差分抽出**: 元の状態との比較を行い、影響範囲（Blast Radius）を特定する。

## Success Criteria
- [ ] 既存のデータを破壊せずにシミュレーションが完了すること。
- [ ] 摂動を与えた L1 が属する L2 コンセプトが正しく特定されること。
- [ ] 目的関数の変化（向上/悪化）が数値として取得できること。
