# DBM-LEGACY-SEARCH-DEPRECATION-SPEC v1.0 監査票

- 作成日: 2026-04-22
- 対象リポジトリ: `Design_BrainModel`
- 監査対象: legacy search 無効化仕様
- 判定基準:
  - `実装済み`: 仕様を直接満たす実装と対応テストがある
  - `部分対応`: 一部の性質は満たすが、仕様の必須条件を満たしていない
  - `未実装`: 仕様に対応する実装またはテストが確認できない

## 総合判定

`未完了`

理由:

- `crates/search_controller` と `crates/engine/design_search_engine` は workspace に残っている
- runtime 系 crate がこれら legacy search crate に直接依存している
- panic 化、`#[deprecated]`, `#![deny(deprecated)]`, `legacy-search` feature gate, `compile_error!` は未実装
- したがって「旧ラインを絶対に使えない状態」に達していない

## 監査サマリ

| ID | 監査項目 | 判定 | 根拠 | 不足 |
| --- | --- | --- | --- | --- |
| 0 | 目的: legacy search を無効化し runtime Phase B に統一 | 未実装 | runtime と legacy が混在 | 単一実装原則未達 |
| 1 | 適用範囲整理 | 部分対応 | 対象 crate は明確に存在 | まだ active members / active deps |
| 2 | 基本原則 | 未実装 | runtime 以外の探索実装が実行可能 | fail-fast / observability なし |
| 3 | 非機能要件 | 部分対応 | 現状ビルド可能性はある | deprecation 後の整合性保証は未実施 |
| 4.1 | 参照遮断 | 未実装 | 多数の直接参照あり | runtime/search への統一未完了 |
| 4.2 | 強制デプリケーション(panic) | 未実装 | 旧 API は通常実装のまま export されている | `panic!("FATAL...")` なし |
| 4.3 | Feature Flag 制御 | 未実装 | `legacy-search` feature なし | `compile_error!` なし |
| 4.4 | Cargo構成変更 | 未実装 | workspace members / deps に残存 | 除外・依存削除なし |
| 4.5 | コンパイルレベル検出 | 未実装 | `#![deny(deprecated)]` なし | 旧 API 参照で fail しない |
| 4.6 | 実行時検出 | 未実装 | error log / metrics なし | legacy invocation 検知不可 |
| 5 | テスト要件 | 未実装 | legacy panic test 等なし | runtime only / build integrity test なし |
| 6 | 完了条件 | 未達 | 旧 API がまだ普通に使われている | 1-5 全項目未達 |
| 7 | 禁止事項順守 | 未実装 | runtime と legacy の混在実行状態 | 混在依存を解消していない |
| 8 | 期待状態 | 未実装 | `runtime/search only` になっていない | legacy call 不可状態でない |
| 9 | 次工程前提 | 未達 | Step1 完了条件未達 | Step2 物理削除に進めない |

## 詳細監査

### 0. 目的

判定: `未実装`

根拠:

- workspace に `crates/engine/design_search_engine` と `crates/search_controller` が残っている
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:33)
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:36)
- workspace dependencies にも残っている
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:127)
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:128)
- runtime 側がこれらを直接参照している
  - [crates/runtime/runtime_vm/Cargo.toml](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/Cargo.toml:12)
  - [crates/runtime/runtime_vm/Cargo.toml](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/Cargo.toml:25)
  - [crates/runtime/runtime_core/Cargo.toml](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_core/Cargo.toml:16)

結論:

- 「探索ロジックは runtime/search のみが実行可能」である状態ではない

### 1. 適用範囲

判定: `部分対応`

確認できたこと:

- 対象 crate は存在し、監査対象として明確
  - [crates/search_controller](/Users/chigenori/development/Design_BrainModel/crates/search_controller/Cargo.toml:1)
  - [crates/engine/design_search_engine](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/Cargo.toml:1)

不足:

- 適用範囲の crate がまだ active code path に残っている

### 2. 基本原則

判定: `未実装`

#### 2.1 単一実装原則

判定: `未実装`

根拠:

- `runtime_vm` は `design_search_engine` と `search_controller` を直接 import している
  - [crates/runtime/runtime_vm/src/agent.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/src/agent.rs:3)
  - [crates/runtime/runtime_vm/src/agent.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/src/agent.rs:12)

#### 2.2 フェイルファスト

判定: `未実装`

根拠:

- `search_controller` は通常 API を export している
  - [crates/search_controller/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/src/lib.rs:1)
- `design_search_engine` も通常 API を export している
  - [crates/engine/design_search_engine/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/lib.rs:1)

不足:

- `#[deprecated(note = "Use runtime::search")]`
- `panic!("FATAL: legacy search is disabled. Use runtime::search")`

#### 2.3 可観測性

判定: `未実装`

根拠:

- `legacy search invoked` 相当のログ出力は確認できない
- legacy 呼び出し回数を観測するメトリクスも確認できない

### 3. 非機能要件

判定: `部分対応`

確認できたこと:

- 現状では legacy 実装込みでビルド・テスト系が成立している可能性が高い

不足:

- legacy 無効化後の決定性維持と build integrity の検証は未実施
- 「副作用なし」で deprecation を導入する移行設計がまだない

### 4. 実装仕様

#### 4.1 参照遮断

判定: `未実装`

根拠:

- 直接参照が多数残っている
  - [apps/cli/src/design_main.rs](/Users/chigenori/development/Design_BrainModel/apps/cli/src/design_main.rs:15)
  - [apps/cli/src/design_main.rs](/Users/chigenori/development/Design_BrainModel/apps/cli/src/design_main.rs:1269)
  - [apps/cli/src/app.rs](/Users/chigenori/development/Design_BrainModel/apps/cli/src/app.rs:12)
  - [crates/runtime/runtime_core/src/stable_v03.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_core/src/stable_v03.rs:20)
  - [crates/runtime/runtime_vm/src/agent.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/src/agent.rs:3)
  - [crates/runtime/runtime_vm/src/agent.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/src/agent.rs:12)
  - [crates/search_verification/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/search_verification/src/lib.rs:4)
  - [tests/pipeline/tests/determinism_pipeline.rs](/Users/chigenori/development/Design_BrainModel/tests/pipeline/tests/determinism_pipeline.rs:3)

結論:

- `旧検索API -> runtime::search API` の統一は未完了

#### 4.2 強制デプリケーション

判定: `未実装`

根拠:

- legacy crate の公開 API は通常実装のまま
  - [crates/search_controller/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/src/lib.rs:1)
  - [crates/engine/design_search_engine/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/lib.rs:1)

不足:

- deprecated attribute
- panic stub
- trait impl の panic 化

#### 4.3 Feature Flag 制御

判定: `未実装`

根拠:

- `legacy-search` feature は見つからない
- `compile_error!("legacy-search is forbidden in Phase B finalization")` も見つからない

#### 4.4 Cargo構成変更

判定: `未実装`

根拠:

- workspace members に対象 crate が残っている
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:33)
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:36)
- workspace dependencies に対象 crate が残っている
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:127)
  - [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:128)
- runtime 系 crate に依存が残っている
  - [crates/runtime/runtime_vm/Cargo.toml](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/Cargo.toml:12)
  - [crates/runtime/runtime_vm/Cargo.toml](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/Cargo.toml:25)
  - [crates/runtime/runtime_core/Cargo.toml](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_core/Cargo.toml:16)

#### 4.5 コンパイルレベル検出

判定: `未実装`

根拠:

- `#![deny(deprecated)]` は確認できない
- むしろ一部で `#[allow(deprecated)]` が残っている
  - [apps/cli/src/executor.rs](/Users/chigenori/development/Design_BrainModel/apps/cli/src/executor.rs:90)
  - [apps/cli/src/nl/executor.rs](/Users/chigenori/development/Design_BrainModel/apps/cli/src/nl/executor.rs:3046)
  - [apps/cli/src/nl/mod.rs](/Users/chigenori/development/Design_BrainModel/apps/cli/src/nl/mod.rs:24)
  - [crates/hybrid_vm/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/hybrid_vm/src/lib.rs:1834)

#### 4.6 実行時検出

判定: `未実装`

根拠:

- `eprintln!("ERROR: legacy search invoked")` 相当のコードは検出できない
- `legacy_call_count` 相当のメトリクスもない

### 5. テスト要件

判定: `未実装`

確認できたこと:

- legacy 実装そのものを使うテストは多数ある
  - [crates/search_controller/tests/beam_search_determinism.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/tests/beam_search_determinism.rs:1)
  - [crates/engine/design_search_engine/tests/determinism/beam_search.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/tests/determinism/beam_search.rs:1)

不足:

- Legacy Call Test: 旧 API 呼び出しで panic
- Runtime Only Test: runtime/search のみ使用
- Determinism Regression Test: legacy 無効化後の一致
- Build Integrity Test: deprecation 反映後の `cargo build`

### 6. 完了条件

判定: `未達`

未達理由:

1. 旧検索 API がまだ呼ばれている
2. 呼ばれても panic しない
3. runtime/search のみにはなっていない
4. legacy 無効化後の `cargo test 全成功` は未確認
5. legacy 無効化後の `cargo build 成功` は未確認

### 7. 禁止事項

判定: `未実装`

理由:

- runtime と legacy の混在状態が残っている
- 条件分岐ではなく直接依存で legacy が使われている
- 仕様上禁止している「混在実行」をまだ解消していない

### 8. 出力(期待状態)

判定: `未実装`

期待状態との差分:

- 現在: `runtime + design_search_engine + search_controller`
- 期待: `runtime/search only`

### 9. 次工程

判定: `未着手`

理由:

- Step1 の「旧ラインを絶対に使えない状態にする」が未完了
- よって Step2 の物理削除へ進めない

## 実装上の主要証跡

### legacy search が有効なまま export されている

- [crates/search_controller/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/search_controller/src/lib.rs:1)
- [crates/engine/design_search_engine/src/lib.rs](/Users/chigenori/development/Design_BrainModel/crates/engine/design_search_engine/src/lib.rs:1)

### runtime が legacy crate に直接依存している

- [crates/runtime/runtime_vm/src/agent.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/src/agent.rs:3)
- [crates/runtime/runtime_vm/src/agent.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_vm/src/agent.rs:12)
- [crates/runtime/runtime_core/src/stable_v03.rs](/Users/chigenori/development/Design_BrainModel/crates/runtime/runtime_core/src/stable_v03.rs:20)

### workspace / Cargo 構成が deprecation 前のまま

- [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:33)
- [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:36)
- [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:127)
- [Cargo.toml](/Users/chigenori/development/Design_BrainModel/Cargo.toml:128)

## 完了条件に向けた最低修正

1. `search_controller` と `design_search_engine` の public search API を deprecated + panic 化する
2. runtime / apps / tests / auxiliary crates の直接参照をすべて `runtime/search` に置換する
3. `legacy-search` feature を追加し、使われたら `compile_error!` にする
4. `#![deny(deprecated)]` を導入し、`#[allow(deprecated)]` を精査・削除する
5. workspace 依存から対象 crate を段階的に外す
6. legacy panic test / runtime only test / build integrity test を追加する

## 監査結論

`DBM-LEGACY-SEARCH-DEPRECATION-SPEC v1.0` の Step1 は未完了。

現状は:

- legacy search をまだ使える
- runtime と legacy が混在している
- 呼び出し禁止・ビルド検出・実行時検出のどれも入っていない

したがって、仕様が要求する「旧ラインを絶対に使えない状態」には到達していない。
