# arch-gen — Architecture Generative AI CLI

Design_BrainModel Core を使って自然言語の要件テキストからシステムアーキテクチャ候補を自動生成する CLI ツールです。

## 特徴

- **LLM 不要**: Design_BrainModel の Phase9 パイプラインが NL 処理を内部で完結
- **確定性**: 同じ入力は常に同じ出力（FNV-1a ハッシュベースの決定的探索）
- **多形式出力**: text / json / mermaid / markdown / plantuml
- **ゼロ依存**: 外部サービス・API キー不要、オフライン動作

## インストール

```bash
# ワークスペースルートから
cargo install --path apps/arch_gen

# または直接ビルド
cargo build --release -p arch_gen
# → target/release/arch-gen
```

## Quick Start

```bash
# アーキテクチャ生成（テキスト出力）
arch-gen generate "ECサイトをスケーラブルに設計してください"

# Markdown レポート生成（ファイルに出力）
arch-gen generate @examples/requirements/ecommerce.txt -f markdown -o ./output

# JSON 形式で候補を3件生成
arch-gen generate "マイクロサービスAPIを設計する" -n 3 -f json

# 設計評価
arch-gen evaluate ./output/design.json

# Mermaid ダイアグラム出力
arch-gen export ./output/design.json -f mermaid

# PlantUML ダイアグラム出力
arch-gen export ./output/design.json -f plantuml

# 設計の説明を生成
arch-gen explain ./output/design.json

# 追加要件で設計を再探索
arch-gen refine ./output/design.json "OAuth2認証とレート制限を追加してください"
```

## コマンドリファレンス

### `generate`

```
arch-gen generate <REQUIREMENT> [OPTIONS]

引数:
  <REQUIREMENT>    要件テキスト（@ファイルパス 形式でファイル読み込みも可）

オプション:
  -n, --candidates <N>      出力候補数 [default: 3]
  -o, --output <DIR>        出力ディレクトリ [default: ./arch_out]
  -f, --format <FMT>        出力形式: text|json|mermaid|markdown|plantuml [default: text]
      --beam-width <N>      ビームサーチ幅 [default: 10]
      --max-depth <N>       サーチ深度 [default: 5]
      --no-code             コード生成をスキップ（高速化）
      --verbose             詳細ログを出力
```

### `evaluate`

```
arch-gen evaluate <DESIGN_FILE>

保存済み design.json を読み込んで各候補のスコアと品質分析を表示する。
```

### `export`

```
arch-gen export <DESIGN_FILE> -f <FORMAT> [OPTIONS]

オプション:
  -f, --format <FMT>    出力形式: json|mermaid|markdown|plantuml|text
  -o, --output <FILE>   出力先ファイルパス（省略時は stdout）
```

### `explain`

```
arch-gen explain <DESIGN_FILE>

設計パターンの推定と品質スコアの解説テキストを生成する。
```

### `refine`

```
arch-gen refine <DESIGN_FILE> <ADDITIONAL_REQUIREMENT>

既存設計に追加要件を合成して Phase9 パイプラインを再実行し、
design_refined.json として保存する。
```

## 出力形式

| 形式 | 説明 | 用途 |
|------|------|------|
| `text` | テキストサマリー（デフォルト） | ターミナル確認 |
| `json` | 構造化 JSON | プログラム連携 |
| `mermaid` | Mermaid graph TD | GitHub / Notion |
| `markdown` | 統合 Markdown レポート | ドキュメント生成 |
| `plantuml` | PlantUML @startuml | UML ツール連携 |

## サンプル要件ファイル

```
examples/requirements/
  ├── ecommerce.txt          # ECサイト（認証・在庫・注文・決済）
  ├── microservices_api.txt  # マイクロサービス REST API
  ├── simple_webapp.txt      # シンプル Web アプリ（SPA + API）
  └── event_driven.txt       # イベント駆動システム（Kafka + CQRS）
```

## アーキテクチャ

```
arch-gen
├── InputBridge
│   ├── text_parser     — 要件テキスト解析（@ファイル参照解決）
│   ├── file_loader     — design.json 読み書き
│   └── arch_converter  — ArchitectureState → design_domain::Architecture 変換
│
├── Design_BrainModel Core（外部クレート）
│   └── Phase9 パイプライン: VM → HypothesisGenerator → WorldModel → BeamSearch
│
└── OutputFormatter
    ├── text      — テキストサマリー
    ├── mermaid   — Mermaid graph TD
    ├── markdown  — 統合 Markdown レポート
    └── plantuml  — PlantUML @startuml
```

## ライセンス

このプロジェクトの一部として、Design_BrainModel ワークスペースのライセンスに従います。
