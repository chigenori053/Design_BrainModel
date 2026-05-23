# DBM CLI Runner Timeout Classification Stabilization Spec v1.0

## 1. 目的 / Objective
並列テスト実行など高負荷な環境下において、`design_cli` の runner タイムアウト/プロセスグループ関連テストが安定してパスすることを保証し、タイムアウトの分類が `"failure"` と誤判定される現象を解消します。

Ensure that the `design_cli` runner timeout/process-group tests pass stably even under high load (such as parallel test execution) and resolve the issue where timeouts are incorrectly classified as `"failure"`.

---

## 2. 背景と課題 / Background and Issues
タイムアウトの制限時間（`timeout_duration`）に到達した後に runner が子プロセスやプロセスグループを強制終了させた場合、OS の終了ステータスが非ゼロ（またはシグナル終了）になります。
従来の設計では、この終了ステータスがタイムアウト判定を上書きして最終的な結果ステータスが `"failure"` に分類されていました。
また、クリーンアップ中に発生したエラーがタイムアウト判定を `"failure"` に上書きしてしまう問題もありました。

When the runner kills the child process or process group after reaching the timeout deadline, the OS exit status becomes non-zero or signal-terminated.
In the previous design, this status would overwrite the timeout decision, resulting in a `"failure"` classification.
Additionally, cleanup errors could also overwrite the `"timeout"` status.

---

## 3. 要件仕様 / Requirements & Specifications

### 3.1 メタデータの拡張 / Metadata Expansion
`ExecutionResult` 構造体に以下の 4 つのメタデータフィールドを追加します。
Add the following 4 metadata fields to the `ExecutionResult` struct:
- `timeout_triggered: bool` (タイムアウトに到達したかどうか / Whether the timeout deadline was reached)
- `kill_sent: bool` (子プロセスに kill を送信したか / Whether kill was sent to the child process)
- `process_group_kill_sent: bool` (プロセスグループ全体に kill を送信したか / Whether kill was sent to the process group)
- `cleanup_error: Option<String>` (クリーンアップ中に発生したエラーメッセージ / Error messages captured during cleanup)

### 3.2 タイムアウト分類の最優先化 / Prioritized Timeout Classification
結果の `status` 分類時、OS の `ExitStatus` や終了コードに関わらず、`timeout_triggered == true` が検出された場合は最優先で `"timeout"` と分類します。
During result status classification, if `timeout_triggered == true` is detected, it must be classified as `"timeout"` with the highest priority, overriding any OS `ExitStatus` or exit code.

### 3.3 クリーンアップエラーの分離 / Cleanup Error Isolation
クリーンアップ処理（kill や wait）でエラーが発生した場合でも、最終ステータスを `"failure"` に上書きせず、`cleanup_error` フィールドにエラー情報を記録します。
Any errors occurring during cleanup (e.g., kill or wait failures) must be logged in `cleanup_error` without altering the final status from `"timeout"` to `"failure"`.

---

## 4. 実装設計 / Implementation Design

### 4.1 `apps/cli/src/runner/types.rs`
- `ExecutionResult` に 3.1 のメタデータフィールドを追加します。
  - Add metadata fields to `ExecutionResult`.

### 4.2 `apps/cli/src/runner/process.rs`
- `ExecutionResultInput` の `status` を `Option<ProcessExitStatus>` とし、タイムアウト時に `None` を許容します。
  - Allow `Option<ProcessExitStatus>` to accept `None` during timeouts.
- `execute_process` のループ処理内で `wait_timeout` の前後に経過時間のチェックを明示的に挟み、確実にタイムアウトを検出します。
  - Detect timeout reliably by checking elapsed time before and after `wait_timeout`.
- `build_result` の分類優先順位を以下のようにします：
  - Classification priority in `build_result`:
    1. `timeout_triggered == true` -> `"timeout"`
    2. `rejected` (if applicable) -> `"rejected"`
    3. `status.is_none()` -> `"failure"`
    4. `exit_code == 0` -> `"success"`
    5. `exit_code != 0` -> `"failure"`

### 4.3 `apps/cli/src/runner/tests.rs`
- タイミング依存を減らすため、テストの sleep 設定などを安定化させます。
  - Stabilize timings for existing runner tests.
- 新たに 4 つのテストを追加して仕様を満たす動作を確認します。
  - Add 4 new test cases to verify the implementation.

---

## 5. 検証 / Verification
1. `design_cli` の全 runner 関連テストが成功すること。
   - All runner tests in `design_cli` must pass.
   - Command: `cargo test -p design_cli runner::tests --lib`
2. Clippy による警告が一切ないこと。
   - Clippy must compile clean without any warnings.
   - Command: `cargo clippy -p design_cli --all-targets -- -D warnings`
