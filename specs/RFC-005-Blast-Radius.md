# RFC-005: 影響波及範囲 (Blast Radius) 評価アルゴリズム

## Purpose
RFC-004（摂動シミュレーション）によって得られる予測データを解析し、特定の変更が設計全体に与える影響の「広さ」と「深さ」を **Blast Radius (影響波及範囲) スコア** として算出するアルゴリズムを定義する。これにより、設計者は変更による予期せぬ副作用のリスクを事前に把握できる。

## Interfaces

### HybridVM API
- `fn evaluate_blast_radius(&self, report: &SimulationReport) -> BlastRadiusScore`

### Data Structures
```rust
pub struct BlastRadiusScore {
    pub coverage: f64,       // 影響を受けた L2 コンセプトの割合 (0.0 - 1.0)
    pub intensity: f64,      // 安定性の変化量の平均強度
    pub structural_risk: f64, // 結合度の高い(Hub)要素への影響度
    pub total_score: f64,    // 総合的なリスク指数
}
```

## Logic

### 1. Coverage (影響広域度)
影響を受けた（安定性が閾値 ε 以上変化した）L2 コンセプトの数 / 全 L2 コンセプト数。

### 2. Intensity (影響強度)
各コンセプトの安定性変化量 `|simulated - original|` の平均値。

### 3. Structural Risk (構造的リスク)
因果グラフにおける「ハブ（結合集中ノード）」の安定性が低下した場合、スコアを重み付けして加算する。

### 4. Total Score の算出
`Total = (Coverage * w1) + (Intensity * w2) + (StructuralRisk * w3)`
※ w1, w2, w3 は調整可能なパラメータ。

## Success Criteria
- [ ] シミュレーション結果から、影響を受けたユニットのリストが正確に抽出されること。
- [ ] 変更が局所的なのか、全体波及的なのかを 0.0 - 1.0 のスコアで区別できること。
- [ ] JSON Schema v1.3 の `simulation` データに `blast_radius` フィールドが含まれること。
