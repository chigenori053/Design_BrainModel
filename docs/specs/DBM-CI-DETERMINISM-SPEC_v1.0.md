# DBM-CI-DETERMINISM-SPEC v1.0

（CI最適化・固定化仕様）

## 1. 目的
- [x] Determinism を CI で強制
- [x] 非決定性を即時検知
- [x] 再現性を品質基準に昇格
- [x] 既存テストとの整合維持

## 2. CI構成（最適化版）

CIは4層に分ける：

- Layer 1: Build
- Layer 2: Unit / Contract
- Layer 3: Determinism（NEW）
- Layer 4: Regression Guard

## 3. GitHub Actions（完成版）

`.github/workflows/ci.yml`

```yaml
name: DBM CI

on:
  push:
  pull_request:

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --workspace

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p design_cli --all-features

  determinism:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Determinism (No-op)
        run: |
          cargo run -p design_cli --bin verify_cli -- \
            determinism \
            --input " @apps/cli/tests/integration/repl_file_target_routing.rs src/sample.rs を安全に改善して preview" \
            --runs 5 \
            --strict

      - name: Determinism (Composite)
        run: |
          cargo run -p design_cli --bin verify_cli -- \
            determinism \
            --input "src/sample.rs を解析して修正して" \
            --runs 5 \
            --strict

      - name: Determinism (Mutation)
        run: |
          cargo run -p design_cli --bin verify_cli -- \
            determinism \
            --input " @apps/cli/tests/integration/repl_file_target_routing.rs src/sample.rs にログを追加して preview" \
            --runs 5 \
            --strict
```

## 4. 重要設計（ここが本質）
1. **deterministic を “fail条件” にする**
   - deterministic == false → CI FAIL
2. **JSON出力はCIでは不要**
   - CIでは --json 省略（ログ簡潔）
3. **runs=5固定**
   - 理由：
     - 3回 → 偶然一致の可能性
     - 5回 → 実用的に十分

## 5. テスト入力設計（重要）

CIでは必ずこの3カテゴリを使う：

- **A. No-op**
  - ` @apps/cli/tests/integration/repl_file_target_routing.rs ... preview`
- **B. Composite**
  - `解析 → 修正`
- **C. Mutation**
  - `実際に変更を加える`

## 6. 追加最適化（推奨）
1. **並列化**
   - strategy: matrix: case: [noop, composite, mutation]
2. **キャッシュ**
   - uses: Swatinem/rust-cache
3. **fail-fast**
   - fail-fast: true

## 7. Regression Guard（重要）
追加ジョブ
```yaml
  regression:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p design_cli --test contract
```

## 8. ローカル開発用コマンド

CIと同じことをローカルで再現：

`make determinism-check`

**Makefile**
```makefile
determinism-check:
	cargo run -p design_cli --bin verify_cli -- determinism --input " @apps/cli/tests/integration/repl_file_target_routing.rs src/sample.rs を安全に改善して preview" --runs 5 --strict
	cargo run -p design_cli --bin verify_cli -- determinism --input "src/sample.rs を解析して修正して" --runs 5 --strict
	cargo run -p design_cli --bin verify_cli -- determinism --input " @apps/cli/tests/integration/repl_file_target_routing.rs src/sample.rs にログを追加して preview" --runs 5 --strict
```

## 9. 禁止事項
- [ ] Determinismをoptionalにする
- [ ] --strictを外す
- [ ] runsを1にする
- [ ] JSONだけ見てfailしない

## 10. CI成功条件
- [x] build成功
- [x] test成功
- [x] determinism全通過
- [x] contract test安定

## 11. 効果
### 技術的
- [x] 非決定性を完全検出
- [x] 再現性を保証
- [x] バグの早期発見
### プロダクト価値
- [x] 「再現可能AI」を保証できる
- [x] OSSでの信頼性が上がる
- [x] Claude/Codexとの差別化

## 12. 結論

CIにdeterminismを組み込むことで「品質」ではなく「性質」として保証される

## 13. 一言

「毎回同じ結果になる」をCIで強制する状態が完成する
