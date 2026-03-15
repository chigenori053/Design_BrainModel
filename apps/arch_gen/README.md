# arch_gen — Architecture Generative AI CLI

> Automatically generate system architecture candidates from natural language requirements.
> 自然言語の要件テキストからシステムアーキテクチャ候補を自動生成する CLI ツールです。

---

## Features / 特徴

| | EN | JA |
|---|---|---|
| **No LLM** | Design_BrainModel's Phase9 pipeline handles all NL processing internally | 外部 LLM 不要。Phase9 パイプラインが NL 処理を内部で完結 |
| **Deterministic** | Same input always produces the same output (FNV-1a hash-based search) | 同じ入力は常に同じ出力（FNV-1a ハッシュベースの決定的探索）|
| **Multi-format** | text / json / mermaid / markdown / plantuml | 5形式の出力に対応 |
| **Zero dependency** | No external services or API keys required, works offline | 外部サービス・API キー不要、オフライン動作 |
| **Reverse analysis** | `/scan` command infers architecture from existing source code | `/scan` コマンドで既存コードからアーキテクチャを逆解析 |
| **Interactive** | `/interactive` command for iterative design refinement in a REPL | `/interactive` コマンドで対話的に設計を精緻化 |

---

## Installation / インストール

```bash
# From workspace root / ワークスペースルートから
cargo install --path apps/arch_gen

# Or build directly / または直接ビルド
cargo build --release -p arch_gen --bin arch_gen
# → target/release/arch_gen
```

---

## Quick Start

```bash
# Generate architecture candidates (text output)
# アーキテクチャ候補を生成（テキスト出力）
arch_gen /generate "Design a scalable e-commerce platform"
arch_gen /generate "ECサイトをスケーラブルに設計してください"

# Generate from a requirements file
# 要件ファイルから生成
arch_gen /generate @examples/requirements/ecommerce.txt -f markdown -o ./output

# Read requirement from stdin
# stdin から要件を渡す
echo "Design a microservices API" | arch_gen /generate -

# Evaluate a saved design
# 保存済み設計を評価
arch_gen /evaluate ./output/design.json

# Export to various formats
# 各フォーマットでエクスポート
arch_gen /export ./output/design.json -f mermaid
arch_gen /export ./output/design.json -f markdown -o ./report.md

# Explain the design pattern and quality
# 設計パターンと品質を解説
arch_gen /explain ./output/design.json

# Refine with additional requirements
# 追加要件で設計を再探索
arch_gen /refine ./output/design.json "Add OAuth2 authentication and rate limiting"
arch_gen /refine ./output/design.json "OAuth2認証とレート制限を追加してください"

# Scan existing source code
# 既存ソースコードを逆解析
arch_gen /scan ./src -f mermaid
arch_gen /scan ./src --include "**/*.rs" -f markdown -o ./report/arch.md

# Interactive mode
# 対話型モード
arch_gen /interactive
arch_gen /i --from ./output/design.json
```

---

## Commands / コマンドリファレンス

### `/generate`

Generate architecture candidates from a requirement text.
要件テキストからアーキテクチャ候補を生成し、コードと design.json を出力します。

```
arch_gen /generate <REQUIREMENT> [OPTIONS]

Arguments:
  <REQUIREMENT>    Requirement text, @file path, or "-" to read from stdin
                   要件テキスト、@ファイルパス、または "-" で stdin から読み込み

Options:
  -n, --candidates <N>           Number of candidates to output [default: 3]
  -o, --output <DIR>             Output directory [default: ./arch_out]  $ARCH_GEN_OUTPUT
  -f, --format <FMT>             Output format: text|json|mermaid|markdown|plantuml [default: text]  $ARCH_GEN_FORMAT
      --beam-width <N>           Beam search width [default: 10]  $ARCH_GEN_BEAM_WIDTH
      --max-depth <N>            Search depth [default: 5]
      --no-code                  Skip code generation (faster)
      --verbose                  Print detailed logs
      --output-strategy <S>      new | merge | overwrite | dry-run [default: new]
      --output-layout <L>        flat | module [default: flat]
      --git-add                  Run git add on the output directory after generation
      --open                     Open output with the OS default application
```

### `/evaluate`

Load a saved `design.json` and display per-candidate scores and quality analysis.
保存済み `design.json` を読み込んで各候補のスコアと品質分析を表示します。

```
arch_gen /evaluate <DESIGN_FILE>
```

### `/export`

Export a saved design file in the specified format.
保存済み設計ファイルを指定フォーマットで出力します。

```
arch_gen /export <DESIGN_FILE> -f <FORMAT> [OPTIONS]

Options:
  -f, --format <FMT>    json | mermaid | markdown | plantuml | text  $ARCH_GEN_FORMAT
  -o, --output <FILE>   Output file path (stdout if omitted)
      --open            Open the output file after export
```

### `/explain`

Generate a human-readable explanation of the design pattern and quality scores.
設計パターンの推定と品質スコアの解説テキストを生成します。

```
arch_gen /explain <DESIGN_FILE>
```

### `/refine`

Combine the original requirement with an additional requirement and re-run the pipeline.
元の要件に追加要件を合成して Phase9 パイプラインを再実行し、`design_refined.json` として保存します。

```
arch_gen /refine <DESIGN_FILE> <ADDITIONAL_REQUIREMENT>
```

### `/scan`

Walk a local directory, parse source files, and infer the architecture via reverse analysis.
ローカルディレクトリのソースコードを読み込み、アーキテクチャを逆解析します。

```
arch_gen /scan <DIR> [OPTIONS]

Options:
  -f, --format <FMT>     text | mermaid | markdown | json | plantuml [default: text]
  -o, --output <FILE>    Output file path (stdout if omitted)
      --depth <N>        Scan depth [default: 3]
      --include <GLOB>   File pattern [default: **/*.rs]
      --verbose
```

### `/interactive` / `/i`

Start an interactive REPL session for iterative architecture design.
対話型セッションを起動して設計を段階的に精緻化します。

```
arch_gen /interactive [--from <DESIGN_FILE>]
arch_gen /i           [--from <DESIGN_FILE>]

Session commands:
  <requirement>   Generate candidates from a new requirement
  s <N>           Select candidate N
  r               Refine with an additional requirement
  list            Show all candidates
  m               Show Mermaid diagram for selected candidate
  e [fmt]         Export selected candidate (text | mermaid | markdown)
  save [path]     Save session to design JSON  [default: design_session.json]
  help            Show this help
  q / quit        Exit
```

---

## Output Formats / 出力形式

| Format | Description | Use case |
|--------|-------------|----------|
| `text` | Text summary (default) | Terminal review / ターミナル確認 |
| `json` | Structured JSON | Programmatic use / プログラム連携 |
| `mermaid` | Mermaid `graph TD` | GitHub / Notion |
| `markdown` | Integrated Markdown report | Documentation / ドキュメント生成 |
| `plantuml` | PlantUML `@startuml` | UML tools / UML ツール連携 |

---

## Output Strategy / 出力戦略

`--output-strategy` controls how existing files are handled during code generation.

| Strategy | Behavior |
|----------|----------|
| `new` | Write to a new directory (default) |
| `merge` | Skip existing files, write new ones only |
| `overwrite` | Overwrite existing files |
| `dry-run` | Print what would be written without writing anything |

```bash
# Preview what would be generated
arch_gen /generate "API server" --output-strategy dry-run

# Merge into an existing project
arch_gen /generate "API server" -o ./my-project/src --output-strategy merge
```

---

## Environment Variables / 環境変数

| Variable | Equivalent option |
|----------|-------------------|
| `ARCH_GEN_FORMAT` | `-f` / `--format` |
| `ARCH_GEN_OUTPUT` | `-o` / `--output` |
| `ARCH_GEN_CANDIDATES` | `-n` / `--candidates` |
| `ARCH_GEN_BEAM_WIDTH` | `--beam-width` |

```bash
export ARCH_GEN_FORMAT=mermaid
export ARCH_GEN_OUTPUT=./arch_out
arch_gen /generate "ECサイト"   # uses env var defaults
```

---

## Sample Requirements / サンプル要件ファイル

```
examples/requirements/
  ├── ecommerce.txt          # E-commerce site (auth, inventory, orders, payments)
  ├── microservices_api.txt  # Microservices REST API
  ├── simple_webapp.txt      # Simple web app (SPA + API + DB)
  └── event_driven.txt       # Event-driven system (Kafka + CQRS)
```

---

## Architecture Overview / 内部アーキテクチャ

```
arch_gen
├── InputBridge
│   ├── text_parser     — Requirement text parsing (@file reference resolution)
│   ├── file_loader     — design.json read/write
│   └── arch_converter  — ArchitectureState → design_domain::Architecture
│
├── Design_BrainModel Core  (external crates, no LLM)
│   └── Phase9 Pipeline: RuntimeHybridVm → HypothesisGenerator → WorldModel → BeamSearch
│
├── OutputFormatter
│   ├── text      — Text summary
│   ├── mermaid   — Mermaid graph TD
│   ├── markdown  — Integrated Markdown report
│   └── plantuml  — PlantUML @startuml
│
└── Commands
    ├── /generate     — Full pipeline execution + code generation
    ├── /evaluate     — Score display from design.json
    ├── /export       — Format conversion from design.json
    ├── /explain      — Design pattern analysis
    ├── /refine       — Re-search with additional requirements
    ├── /scan         — Reverse architecture analysis from source code
    └── /interactive  — REPL-based iterative design session
```

---

## License / ライセンス

Part of the Design_BrainModel workspace. See the root license file for details.
Design_BrainModel ワークスペースの一部です。ライセンスはルートのライセンスファイルに従います。
