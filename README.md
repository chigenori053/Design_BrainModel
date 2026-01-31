# Design_BrainModel

## Phase 0: Architecture Validation

This repository hosts the **Phase 0** implementation of the Design_BrainModel system.

**Goal:** Validate the Hybrid VM as the single source of truth and the feedback loop between UI, VM, and Design Brain.

**Structure:**
- `hybrid_vm/`: Core state management and orchestration (Python).
- `design_brain/`: Stateless design intelligence (Python).
- `execution_layer/`: Mock execution environment (Python).
- `ui/`: Desktop user interface (Tauri + React).

**Status:** Phase 0 Implementation in progress.

## 概要
意図と言語構造を統合した、自律的な思考・設計モデルの開発プロジェクトです。

## Tests (venv)
全テストを `.venv_phase17` の仮想環境で実行する手順:

```bash
tools/setup_test_env.sh
tools/run_tests_venv.sh
```

特定のテストのみ実行する場合:

```bash
tools/run_tests_venv.sh tests/test_phase17_snapshot_contract.py
```
