# ArchitectureExtractor v2 Test Structure

## Goal

Phase6 に向けて `ArchitectureExtractor v2` の検証入口を整理する。

対象レイヤ:

1. Repository Loader
2. Language Parser Layer
3. Dependency Model Builder
4. Architecture Inference Engine
5. DesignGraph Builder

## Added Assets

- `tools/run_architecture_extractor_v2_validation.py`
- `tests/fixtures/architecture_extractor_v2/`

## Fixture Set

- `rust_layered_service`
  - Rust layered architecture fixture
- `cpp_plugin_host`
  - C/C++ plugin architecture fixture
- `go_event_pipeline`
  - Go pipeline fixture
- `python_worker_optional`
  - Optional Python fixture

## Current Validation Categories

- `AXV2-T1 Fixture Validation`
  - fixture ごとの `DesignGraph` 再構成と抽出精度を評価
- `AXV2-T2 Polyglot Coverage`
  - Rust / C/C++ / Go / Python の fixture coverage を確認
- `AXV2-T3 Scalability Readiness`
  - 1M LOC 60 秒未満のための設計前提をチェック

## Output Format

出力は `architecture_extractor_v2_report.json`。

主要構造:

```json
{
  "design_graph": {
    "nodes": [],
    "edges": []
  },
  "architecture_patterns": [],
  "layers": [],
  "services": [],
  "metrics": {
    "module_count": 0,
    "dependency_count": 0
  }
}
```

## Next Implementation Steps

- Repository Loader を git-aware に拡張
- Rust は `syn`、C/C++ は `clang AST`、Go は `go/parser` に差し替え
- `MultiDependencyGraph` に `call/type/data/build` edge を明示分離
- clustering / layer inference / service boundary detection を本実装へ移行
- incremental analysis と compressed graph を Phase6 実装へ統合
