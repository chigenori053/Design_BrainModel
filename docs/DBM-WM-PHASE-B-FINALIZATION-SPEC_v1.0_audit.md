# DBM-WM-PHASE-B-FINALIZATION-SPEC v1.0 監査票

- 作成日: 2026-04-22
- 対象リポジトリ: `Design_BrainModel`
- 監査対象: Phase B 探索エンジン最終仕様
- 判定基準:
  - `実装済み`: 仕様を直接満たす実装と対応テストがある
  - `部分対応`: 一部の性質は満たすが、仕様の必須条件を満たしていない
  - `未実装`: 仕様に対応する実装またはテストが確認できない

## 総合判定

`未完了`

理由:

- 完全決定性の一部は確認できる
- ただし、`GlobalVisited`/`ActiveLineageSet`/window 安全化、k-best dominance、snapshot-resume 完全再現、step_index/branch_id/transition_signature 契約、quantization 境界、parallel merge 契約が未実装
- そのため、仕様が要求する `Optimality Safety` `Perfect Replayability` `Parallel Safety` を保証できない

## 監査サマリ

| ID | 監査項目 | 判定 | 根拠 | 不足 |
| --- | --- | --- | --- | --- |
| 0 | 5大保証: Optimality / Determinism / Parallel / Long-run / Replayability | 部分対応 | 決定性テストは存在: `crates/search_controller/tests/beam_search_determinism.rs`, `crates/engine/design_search_engine/tests/determinism/beam_search.rs` | Optimality Safety, Perfect Replayability, 並列merge安全、長時間安定性の仕様水準未達 |
| 1 | Window制約の安全化 | 未実装 | `search_controller` は depth ごとに候補をソートして beam を切るのみ | `GlobalVisited`, `ActiveLineageSet`, 安全 eviction がない |
| 2 | Dominance安全化(k-best保持) | 未実装 | `rank_candidates` は pairwise dominance 数を使うのみ | `state_hash -> top-K records` と `dominated by ALL K-best` がない |
| 3 | transition_signature完全正規化 | 未実装 | 該当識別子未検出 | canonical/order-invariant/serialization-stable な signature 契約なし |
| 4 | Snapshot完全性固定 | 未実装 | `SearchTrace` はあるが `SearchSnapshot`/`resume` はない | beam, global_visited, lineage, branch mapping を含む snapshot なし |
| 5 | Quantization境界の厳密化 | 未実装 | quantization 実装なし | `epsilon` 規則、raw tie-break がない |
| 6 | 並列merge完全決定性 | 部分対応 | scheduler 内で deterministic sort/hash はある | 探索全体の merge 契約が未実装 |
| 7 | step_index最終仕様 | 未実装 | 該当識別子未検出 | `(depth, local_order, global_seq)` がない |
| 8 | GlobalVisitedメモリ制御 | 未実装 | `GlobalVisited` 自体がない | `M_max`, eviction priority, lineage retain なし |
| 9 | 最終テスト要件 | 部分対応 | basic determinism/beam width テストはある | window safety / k-best / snapshot replay / thread variance なし |
| 10 | Phase Cへの保証 | 未実装 | 前提仕様の未達により保証不能 | infrastructure 水準の証明になっていない |

## 詳細監査

### 0. 目的と5大保証

判定: `部分対応`

確認できたこと:

- 同一入力に対する deterministic behavior のテストはある
  - [crates/search_controller/tests/beam_search_determinism.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/tests/beam_search_determinism.rs:1)
  - [crates/engine/design_search_engine/tests/determinism/beam_search.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/tests/determinism/beam_search.rs:1)
- scheduler には deterministic hash と sort がある
  - [crates/simulation_scheduler/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/simulation_scheduler/src/lib.rs:380)
  - [crates/simulation_scheduler/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/simulation_scheduler/src/lib.rs:448)

不足:

- 最適解不喪失を保証する visited/lineage 保護がない
- resume 後完全一致の仕組みがない
- thread 数変化を含む並列ストレス試験がない
- 長時間安定性の専用検証がない

### 1. Window制約の安全化

判定: `未実装`

根拠:

- `search_controller` は毎 step で候補を収集し、score 順に並べて beam を切るだけ
  - [crates/search_controller/src/beam_search.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/src/beam_search.rs:17)
- `design_search_engine` も `beam` と `elite_archive` は持つが `GlobalVisited` / `ActiveLineageSet` を持たない
  - [crates/engine/design_search_engine/src/beam_search_controller.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/beam_search_controller.rs:192)

不足:

- `ActiveLineageSet = all ancestors of current beam`
- `state ∉ ActiveLineageSet AND state.depth < current_depth - D_window` の eviction 条件

### 2. Dominance安全化(非推移対策)

判定: `未実装`

根拠:

- 現行 ranking は `dominates(lhs, rhs)` を使って「何個に支配されるか」を数えているだけ
  - [crates/engine/design_search_engine/src/ranking.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/ranking.rs:11)
  - [crates/engine/design_search_engine/src/ranking.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/ranking.rs:43)

不足:

- `GlobalVisited[state_hash] = top-K records`
- `candidate is dominated by ALL K-best records` の prune 条件
- `K = 2..4` の実装パラメータ

### 3. transition_signature の完全正規化

判定: `未実装`

根拠:

- `transition_signature` 識別子および canonical signature 契約に相当する実装が探索エンジン配下に見当たらない

不足:

- canonical
- order-invariant
- serialization-stable
- map iteration order 非依存

### 4. Snapshot完全性の固定

判定: `未実装`

根拠:

- 探索エンジンには `SearchTrace` はある
  - [crates/engine/design_search_engine/src/beam_search_controller.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/beam_search_controller.rs:31)
- ただし `SearchSnapshot` や `resume(snapshot)` は確認できない

不足:

- `beam`
- `global_visited (full)`
- `step_index`
- `branch_id mapping`
- `ActiveLineageSet`
- `resume(snapshot) must produce identical future results`

### 5. Quantization境界の厳密化

判定: `未実装`

根拠:

- quantization に関する実装は探索エンジン側に見当たらない
- さらに `agent_core` の検査では production code に `quantize` がないことを前提にしている
  - [crates/agent_core/tests/architecture_tests.rs](/Users/chigenori/development/Design_BrainModel/crates/agent_core/tests/architecture_tests.rs:224)

不足:

- `epsilon = quantization_step / 2`
- `score_quantized` 同値時の `score_raw` 比較

### 6. 並列mergeの完全決定性

判定: `部分対応`

確認できたこと:

- scheduler は `architecture_hash` を canonical に作り
  - [crates/simulation_scheduler/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/simulation_scheduler/src/lib.rs:448)
- `ranking_score` 降順、hash 昇順で deterministic に sort している
  - [crates/simulation_scheduler/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/simulation_scheduler/src/lib.rs:380)

不足:

- 探索本体の merge が `collect all -> deduplicate(state_hash) -> apply ordering -> top beam_width` という契約で固定されていない
- `HashMap<String, Vec<SearchState>>` に積んで `remove(0)` しており、仕様の merge API と一致しない
  - [crates/engine/design_search_engine/src/beam_search_controller.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/beam_search_controller.rs:258)

### 7. step_index最終仕様

判定: `未実装`

根拠:

- `step_index = (depth, local_order, global_seq)` に相当する構造が見当たらない
- `SearchState` にも該当フィールドがない
  - [crates/engine/design_search_engine/src/search_state.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/search_state.rs:8)

不足:

- parallel independent
- replay stable

### 8. GlobalVisitedメモリ制御

判定: `未実装`

根拠:

- `GlobalVisited` 自体が探索エンジンに存在しない

不足:

- `|GlobalVisited| <= M_max`
- eviction priority: not lineage, lowest score, oldest depth
- `ActiveLineageSet` の絶対保持

### 9. 最終テスト要件

判定: `部分対応`

確認できたこと:

- basic determinism test
  - [crates/search_controller/tests/beam_search_determinism.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/tests/beam_search_determinism.rs:1)
  - [crates/engine/design_search_engine/tests/determinism/beam_search.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/tests/determinism/beam_search.rs:18)
- scheduler determinism test
  - [crates/simulation_scheduler/tests/phase30_scheduler.rs](/Users/chigenori/development/Design_BrainModel/crates/simulation_scheduler/tests/phase30_scheduler.rs:119)

不足:

- Optimality Safety Test
- Window Safety Test
- k-best Dominance Test
- Transition Determinism Test(branch_id 完全一致)
- Snapshot Perfect Replay Test
- Parallel Stress Test(thread count variance)

### 10. Phase Cへの保証

判定: `未実装`

理由:

- Phase C の前提になる safety contract が未固定
- 現状は「決定的に動くことがある探索実装」であって、仕様が要求する「理論的に破綻しない探索基盤」ではない

## 実行確認

監査時に通過確認したテスト:

- `cargo test -p search_controller --test beam_search_determinism`
- `cargo test -p design_search_engine --test determinism`

補足:

- 上記はあくまで basic determinism の確認
- 本仕様の完了判定に必要な最終テスト群は未整備

## 完了条件

この監査票に対して完了と判定するための最低条件:

1. `GlobalVisited` と `ActiveLineageSet` を導入する
2. window eviction を lineage-safe に固定する
3. dominance を `top-K` 保持へ変更する
4. `transition_signature` を canonical / stable に固定する
5. `SearchSnapshot` と `resume(snapshot)` を導入する
6. `step_index` / `branch_id mapping` を固定する
7. quantization 比較規則を実装する
8. parallel merge を仕様どおり deterministic pipeline に固定する
9. 最終テスト要件 9.1-9.6 を追加し、すべて通す

## 監査結論

現状の Phase B 探索エンジンは:

- `高品質な探索実装`: 一部該当
- `理論的に破綻しない探索基盤`: 未達

したがって、`DBM-WM-PHASE-B-FINALIZATION-SPEC v1.0` の作業は完了していない。
