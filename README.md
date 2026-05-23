# DesignBrainModel (DBM)

**DBM is a safety-oriented reasoning system for AI-assisted software development.**  
**DBM は、AIを活用したソフトウェア開発のための、安全指向の推論システムです。**

Claude Code・Codex CLI などのコーディングエージェントは、コード生成において強力です。  
しかし、設計意図の保持・実行安全性・構造的整合性を継続的に維持することは苦手です。

DBM は、これらのエージェントを補完する推論・制御レイヤーとして設計されています。

Frontier coding agents like Claude Code and Codex CLI excel at code generation.  
However, they do not reliably preserve design intent, execution safety, or structural consistency across iterative development.

DBM is designed as a reasoning and control layer that complements such agents.

> **"DBM doesn't compete with Claude Code — it provides the structure and safety that Claude Code tends to lack."**

---

## Architecture

```
┌─────────────────────────────────────┐
│          Design Intent              │
│      design.md / DesignUnit         │
└──────────────────┬──────────────────┘
                   │ anchored to
┌──────────────────▼──────────────────┐
│        DesignBrainModel (DBM)       │
│                                     │
│  Planner → Executor → Validation    │
│  Repair Loop → Convergence Control  │
│  Execution Safety                   │
└──────────────────┬──────────────────┘
                   │ operates on
┌──────────────────▼──────────────────┐
│       Development Environment       │
│    Source Code / Cargo / Git / REPL │
└─────────────────────────────────────┘
```

---

## Why DBM?

| 課題 / Problem | DBM のアプローチ / DBM's Approach |
|---|---|
| コーディングエージェントは設計意図を保持しない | `design.md` → `DesignUnit` による意図の永続化 |
| LLM 出力は非決定的 | 構造化された実行・検証フローによる制御 |
| AI 生成の変更がアーキテクチャを壊す可能性がある | 設計・コード・実行を横断した推論 |
| 修正ループがアドホックになりやすい | Repair を推論プロセスの一部として扱う |
| 安全でない実行がプロジェクトを破壊しうる | コマンド分類による実行安全制御 |
| Agents don't preserve design intent | Intent persistence via `design.md` → `DesignUnit` |
| LLM outputs are non-deterministic | Structured execution and validation flow |
| AI changes may break architecture | Reasoning over design, code, and execution together |
| Repair loops are ad hoc | Repair as part of the structured reasoning process |
| Unsafe execution can damage the project | Command classification and execution safety control |

---

## Implementation Status

| Feature | Status |
|---|---|
| Autonomous execution loop | ✅ Implemented |
| REPL-based interaction | ✅ Implemented |
| Planner → Executor flow | ✅ Implemented |
| Convergence control | ✅ Implemented |
| Execution Safety (command classification) | ✅ Implemented |
| Debugging flow | ✅ Implemented |
| Git read-only integration | 🔧 In Progress |
| Restricted git add / commit | 🔧 In Progress |
| Design intent integration (DesignUnit) | 🔧 In Progress |
| Architecture-aware refactor targeting | 📋 Planned |
| GitHub / PR integration | 📋 Planned |
| UI / dashboard | 📋 Planned |

---

## Getting Started

### 動作環境 / Environment

| Item | Recommended |
|---|---|
| OS | macOS (Apple Silicon optimized) |
| Language | Rust |
| Build System | Cargo |
| Shell | zsh |
| Primary Interface | DBM CLI / REPL |

### ビルド / Build

```bash
cargo build -p design_cli --bin dbm
```

```bash
# CLI オプション確認 / Check CLI options
cargo run -p design_cli -- --help

# テスト実行 / Run tests
cargo test -p design_cli
```

### REPL 起動 / Start REPL

```bash
cargo run -p design_cli -- --repl
```

### 基本的な使い方 / Basic Usage

```
> analyze current project structure
> inspect target module
> propose repair plan
> apply safe modification
> validate result
```

開発ループの想定 / Intended development loop:

```
User Input
→ Planner
→ Executor
→ Validation
→ Result
→ Next Reasoning Step
```

---

## Safety Policy

DBM はデフォルトで安全な操作のみを実行します。  
DBM restricts execution by default and classifies commands into safety levels.

| Category | Examples |
|---|---|
| `SafeRead` | `git status`, `git diff`, `git log` |
| `SafeWrite` | `git add <file>`, `git commit` |
| `Dangerous` | `git push --force`, `rm -rf`, repository deletion |

```
SafeRead  → allowed
SafeWrite → restricted
Dangerous → denied or requires explicit control
```

---

## Roadmap

| Phase | Status | Goals |
|---|---|---|
| Phase 0: Core Engine | ✅ Completed | Autonomous execution, convergence control, REPL |
| Phase 1: Local Integration | 🔧 In Progress | Git integration, local dev workflow |
| Phase 2: Remote Integration | 📋 Planned | GitHub CLI, PR creation |
| Phase 3: Productization | 📋 Planned | UI, packaging, documentation |

---

## License

TBD
