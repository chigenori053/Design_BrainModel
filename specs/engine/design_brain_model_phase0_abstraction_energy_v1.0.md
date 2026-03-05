# DesignBrainModel Phase 0
Version: 1.0
Status: draft

---

## Purpose
Phase 0 の目的は、設計構造に対して決定論的・非線形エネルギー最小化による抽象度整形エンジンを確立すること。

Phase 0 では以下を行わない。
- GeometryEngine 導入
- 確率探索
- 多体相互作用
- GPU 最適化
- WebSearch 連携

対象は以下の基盤構築に限定する。
- 有向依存構造
- 抽象度ギャップ
- 決定論的収束

---

## Layer Structure
1. `DesignIR`
2. SCC 分解
3. Condensation DAG
4. 抽象度初期化
5. 非線形エネルギー最小化
6. Design 更新

---

## Interfaces
### DesignIR (Phase 0 Scope)
```rust
pub struct DesignNode {
    pub id: NodeId,
    pub abstraction_level: Option<i32>, // Optional
}
```

```rust
pub struct DependencyGraph {
    pub adjacency: HashMap<NodeId, Vec<DependencyEdge>>,
}

pub struct DependencyEdge {
    pub to: NodeId,
    pub kind: DependencyKind,
    pub weight: f64, // optional, default 1.0
}
```

契約:
- 抽象度は整数 (`i32`)、負値許可、`None` 許可。
- 依存は有向。
- 重複エッジ禁止。
- ノード参照は ID ベース（文字列名参照禁止）。
- SCC 許容。
- 自己ループ許容。

---

## Data Structures And Invariants
### SCC 分解
- アルゴリズム: Tarjan（決定論）。
- ノード順は固定ソート済み。
- 隣接順は固定ソート済み。
- SCC 内部ノード順は固定ソート。
- SCC 集合は代表 ID 順で固定ソート。
- SCC は第一級更新単位。

### 抽象度レンジ自動決定
1. Condensation DAG 上の最長パス長 `D` を決定論 DP で算出。
2. 抽象度レンジ `L` を以下で算出:
   - `L = max(1, ceil(beta * D + gamma * log2(N + 1)))`
3. 推奨初期値:
   - `beta = 1.0`
   - `gamma = 0.5`
4. 抽象度範囲:
   - `a(u) in [-L, +L]`

### None ノード初期化
SCC 外エッジのみ使い、重み付き次数を定義:
- `s(u) = (sum of outgoing SCC-external weights) - (sum of incoming SCC-external weights)`
- `DependencyKind` 重みを反映。
- SCC 内部エッジは除外。

写像:
- `s(u)` を min/max 正規化。
- `[-L, +L]` に線形写像。
- 整数化は `floor`。
- 全ノード同値の場合は `0`。

---

## Energy Model (EBM)
全体エネルギー:
- `E = W_global * (sum_i w_i * phi_i + sum_(u->v) w_tilde_(u->v) * psi_(u->v))`

### Unary Term `phi`
- Phase 0 は最小実装で可（将来拡張前提）。

### Pair Term `psi` (AbstractionGap)
依存 `u -> v` に対し:
- `psi_(u->v) = max(0, m - (a(v) - a(u)))^2`

制約:
- 有向評価。
- 依存エッジ上のみ評価。
- 同一 SCC 内のみ評価。

### Edge-Local Normalization
- 同一エッジ上の複数制約は `w_tilde_k = w_k / sum_j |w_j|` で正規化。

### Global Normalization
- 全重みの絶対値総和で割る。

---

## Update Algorithm
### Unit
- 更新単位は SCC。

### Small SCC
- `|SCC| <= threshold` は全探索。

### Large SCC
- 擬似勾配による局所改善。

### Delta Candidate
- `Delta in {+/-1 ... +/-K(t)}`

### Initial Radius
- `K0 = floor(alpha * (2L))`

### Alpha
- `alpha = clamp(alpha_min, alpha_max, c / E_max_scc)`
- `E_max_scc` は最大 SCC 内部エッジ数ベース。

### Radius Decay
- `K(t) = max(1, floor(K0 * (1 - t / T)))`

### Delta-E Evaluation Scope
- 再計算対象は以下に限定:
  - ノード単体項
  - 同一 SCC 内の入出エッジ項
- フル再計算禁止。

### Acceptance Rule
- グローバル `E` 基準。
- `DeltaE < 0` の中で最大改善のみ採用。
- best-so-far を保持。

### Convergence
- `|E_(t+1) - E_t| <= epsilon` または反復上限 `T` 到達で停止。

---

## Hard Rules
- 抽象度範囲外の値を禁止。
- SCC 外 SoftEdge を禁止。
- 非存在ノード参照を禁止。
- 依存エッジ外ペア項を禁止。

---

## Determinism Guarantees
- ノード順固定。
- エッジ順固定。
- SCC 順固定。
- 候補順固定。
- 乱数禁止。
- 並列禁止（Phase 0）。

---

## Acceptance Criteria
- 設計依存構造から抽象度が自動整形される。
- 完全決定論で同一入力は同一出力に収束する。
- ペア項を含む非線形相互作用が機能する。
- 局所改善と大域収束条件が実装される。
- Diff で抽象度変更が可視化できる。

---

## Conclusion
Phase 0 は「抽象度を持つ設計構造に対し、決定論的非線形エネルギー最小化を実装するフェーズ」である。  
本仕様達成により DesignBrainModel の数理基盤を確立する。
