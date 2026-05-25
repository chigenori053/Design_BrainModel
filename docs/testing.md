# Testing Guide（現行運用）

このガイドは「今必要なテスト項目のみ」を対象にしています。
一時的に使っていた確認項目・現時点で不要な項目は含めません。

## 1) 日常開発での基本テスト（必須）

変更したクレートを中心に、まず以下を実行します。

```bash
cargo test -p <changed_crate>
```

例（Phase1の主要経路）:

```bash
time cargo test -p agent_core
time cargo test -p design_cli
```

## 2) 変更内容に応じた追加テスト（必要時のみ）

### Determinism に影響する変更

同一入力で同一出力が保証されるかを確認するため、determinism 系テストを実行します。

### Integration に影響する変更

複数クレートをまたぐ処理変更時のみ、integration 系テストを追加実行します。

### Heavy 経路に影響する変更

`ci-heavy` 相当のコードに変更がある場合のみ、heavy テストを実行します。

```bash
cargo test -p agent_core --features ci-heavy
cargo test -p design_cli --features ci-heavy
```

## 3) CI 全体確認（リリース前/必要時）

広範囲の影響がある場合にのみ実施します。

```bash
cargo test --all-features
```

## テスト項目の見直し方針

- 常時必要なものだけを「必須」に残す
- 条件があるものは「必要時のみ」に分離する
- 一時調査用の項目は恒常チェックリストに戻さない
