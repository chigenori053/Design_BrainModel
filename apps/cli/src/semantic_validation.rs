//! Phase 7: Semantic Validation Layer
//!
//! 「直せるAI」から「壊さないことを証明できるAI」へ
//!
//! Validation Pipeline:
//! ```text
//! Before Snapshot
//! → Apply
//! → Validation Pipeline
//!     → Build Check        (必須: cargo check)
//!     → Test Execution     (必須: cargo test)
//!     → Static Analysis    (必須: cargo clippy)
//!     → Runtime Check      (任意: design_cli analyze)
//! → Result
//! → Commit / Rollback
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::refactor::rollback::{WorkspaceSnapshot, rollback_apply};

// ── ValidationStatus (Phase 7: 4.2) ──────────────────────────────────────────

/// 検証全体の合否ステータス（Phase 7 仕様 4.2）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// 全必須検証項目が成功
    Passed,
    /// 必須検証項目（build/test）が失敗
    Failed,
    /// build/test は成功、static analysis など一部がスキップ or 警告
    Partial,
}

// ── SemanticValidationResult (Phase 7: 4.2) ───────────────────────────────────

/// Phase 7: Semantic Validation の結果（仕様 4.2）
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticValidationResult {
    /// 5.1 Build Validation（必須）: cargo check
    pub build_ok: bool,
    /// 5.2 Test Validation（必須）: cargo test
    pub test_ok: bool,
    /// 5.3 Static Analysis（必須）: cargo clippy
    pub static_ok: bool,
    /// 5.4 Runtime Validation（任意）
    pub runtime_ok: bool,
    /// 全体合否
    pub overall: ValidationStatus,
    /// 失敗分類リスト
    pub failures: Vec<SemanticValidationFailure>,
    /// 各検証ステップの診断サマリ
    pub diagnostics: Vec<String>,
}

impl SemanticValidationResult {
    /// 全項目成功のレスポンスを生成（テスト・成功パス用）
    pub fn passed() -> Self {
        Self {
            build_ok: true,
            test_ok: true,
            static_ok: true,
            runtime_ok: true,
            overall: ValidationStatus::Passed,
            failures: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// build/test/static の結果から overall を決定する
    ///
    /// 判定ルール：
    /// - build && test && static → Passed
    /// - !build || !test         → Failed（必須項目失敗）
    /// - build && test && !static → Partial（任意に近い static のみ失敗）
    pub fn compute_overall(
        build_ok: bool,
        test_ok: bool,
        static_ok: bool,
        _runtime_ok: bool,
    ) -> ValidationStatus {
        if build_ok && test_ok && static_ok {
            ValidationStatus::Passed
        } else if !build_ok || !test_ok {
            ValidationStatus::Failed
        } else {
            // static のみ失敗
            ValidationStatus::Partial
        }
    }

    /// Phase 7: 6.2 Rollback が必要かを判定する
    ///
    /// 必須項目（build/test）の失敗または RuntimePanic が発生した場合にロールバックする
    pub fn requires_rollback(&self) -> bool {
        matches!(self.overall, ValidationStatus::Failed)
            || self
                .failures
                .iter()
                .any(|f| matches!(f, SemanticValidationFailure::RuntimePanic { .. }))
    }

    /// 人間が読める合否サマリを返す
    pub fn summary(&self) -> String {
        let status = match self.overall {
            ValidationStatus::Passed => "PASSED",
            ValidationStatus::Failed => "FAILED",
            ValidationStatus::Partial => "PARTIAL",
        };
        let failure_labels: Vec<_> = self.failures.iter().map(|f| f.class_label()).collect();
        if failure_labels.is_empty() {
            format!(
                "[{status}] build={} test={} static={} runtime={}",
                self.build_ok, self.test_ok, self.static_ok, self.runtime_ok
            )
        } else {
            format!(
                "[{status}] failures: {} | build={} test={} static={}",
                failure_labels.join(", "),
                self.build_ok,
                self.test_ok,
                self.static_ok
            )
        }
    }
}

// ── Failure Classification (Phase 7: 9.) ──────────────────────────────────────

/// Phase 7: 検証失敗の分類（仕様 9.）
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticValidationFailure {
    /// Class J: コンパイルエラー（5.1 Build Validation）
    BuildFailure { message: String },
    /// Class K: テスト失敗（5.2 Test Validation）
    TestFailure {
        failed_count: usize,
        message: String,
    },
    /// Class L: 挙動変化・静的解析回帰（5.3 Static / 5.5 Behavior Preservation）
    SemanticRegression { description: String },
    /// Class M: 実行時クラッシュ（5.4 Runtime Validation）
    RuntimePanic { message: String },
}

impl SemanticValidationFailure {
    pub fn class_label(&self) -> &'static str {
        match self {
            Self::BuildFailure { .. } => "Class J",
            Self::TestFailure { .. } => "Class K",
            Self::SemanticRegression { .. } => "Class L",
            Self::RuntimePanic { .. } => "Class M",
        }
    }
}

// ── SemanticQualityScore (Phase 7: 7.) ────────────────────────────────────────

/// Phase 7: Quality スコア内訳（仕様 7.）
///
/// 重み付け: Structural=20, Design=20, Semantic=20, Behavior=20, Determinism=10, Safety=10
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticQualityScore {
    /// 構造的正しさ（Phase 6.2 からの継続）
    pub structural: f32,
    /// 設計改善（Phase 6.2 からの継続）
    pub design: f32,
    /// 意味的正しさ: static analysis が通るか（Phase 7 新規）
    pub semantic: f32,
    /// 振る舞い保持: テストが通るか（Phase 7 新規）
    pub behavior: f32,
    /// 再現可能性（Phase 6.2 からの継続）
    pub determinism: f32,
    /// 安全性: build成功・panic なし
    pub safety: f32,
}

impl SemanticQualityScore {
    pub fn total(&self) -> f32 {
        self.structural * 0.20
            + self.design * 0.20
            + self.semantic * 0.20
            + self.behavior * 0.20
            + self.determinism * 0.10
            + self.safety * 0.10
    }

    /// SemanticValidationResult から Phase 7 Quality スコアを算出する
    pub fn from_result(result: &SemanticValidationResult) -> Self {
        // Semantic: static analysis が通ればOK
        let semantic = if result.static_ok { 1.0f32 } else { 0.0 };
        // Behavior: テストが通ればOK
        let behavior = if result.test_ok { 1.0f32 } else { 0.0 };
        // Safety: build成功 AND RuntimePanic なし
        let has_panic = result
            .failures
            .iter()
            .any(|f| matches!(f, SemanticValidationFailure::RuntimePanic { .. }));
        let safety = if result.build_ok && !has_panic {
            1.0f32
        } else {
            0.0
        };
        // Structural / Design: overall が Passed なら満点、Partial なら 0.5
        let base = match result.overall {
            ValidationStatus::Passed => 1.0f32,
            ValidationStatus::Partial => 0.5,
            ValidationStatus::Failed => 0.0,
        };
        Self {
            structural: base,
            design: base,
            semantic,
            behavior,
            determinism: 1.0, // 決定論的設計
            safety,
        }
    }
}

// ── ValidationPipelineOptions (Phase 7: 4.1) ──────────────────────────────────

/// Validation Pipeline の実行オプション
#[derive(Clone, Debug)]
pub struct ValidationPipelineOptions {
    /// Build Check をスキップ（テスト・CI 用）
    pub skip_build: bool,
    /// Test Execution をスキップ（テスト・CI 用）
    pub skip_tests: bool,
    /// Static Analysis をスキップ
    pub skip_static: bool,
    /// Runtime Check をスキップ（任意: default true）
    pub skip_runtime: bool,
}

impl Default for ValidationPipelineOptions {
    fn default() -> Self {
        Self {
            skip_build: false,
            skip_tests: false,
            skip_static: false,
            skip_runtime: true, // 任意項目はデフォルトでスキップ
        }
    }
}

impl ValidationPipelineOptions {
    /// ドライラン：全コマンドをスキップ（単体テスト用）
    pub fn dry_run() -> Self {
        Self {
            skip_build: true,
            skip_tests: true,
            skip_static: true,
            skip_runtime: true,
        }
    }
}

// ── ValidationPipeline (Phase 7: 4.1) ─────────────────────────────────────────

/// Phase 7: 4.1 Validation Pipeline
///
/// Build → Test → Static → Runtime の順で実行し、失敗を分類する。
/// `skip_build / skip_tests / skip_static` により各ステップを制御できる。
pub struct ValidationPipeline {
    root: PathBuf,
    options: ValidationPipelineOptions,
}

impl ValidationPipeline {
    pub fn new(root: impl Into<PathBuf>, options: ValidationPipelineOptions) -> Self {
        Self {
            root: root.into(),
            options,
        }
    }

    /// パイプラインを実行し SemanticValidationResult を返す
    pub fn run(&self) -> SemanticValidationResult {
        let mut failures: Vec<SemanticValidationFailure> = Vec::new();
        let mut diagnostics: Vec<String> = Vec::new();

        // ── Step 1: Build Check（必須）─────────────────────────────────────
        let build_ok = if self.options.skip_build {
            diagnostics.push("build: skipped".to_string());
            true
        } else {
            match run_build_check(&self.root) {
                Ok(true) => {
                    diagnostics.push("build: ok".to_string());
                    true
                }
                Ok(false) => {
                    let msg = "cargo check failed: compilation errors detected".to_string();
                    diagnostics.push(format!("build: FAILED — {msg}"));
                    failures.push(SemanticValidationFailure::BuildFailure { message: msg });
                    false
                }
                Err(err) => {
                    let msg = format!("cargo check error: {err}");
                    diagnostics.push(format!("build: ERROR — {msg}"));
                    failures.push(SemanticValidationFailure::BuildFailure { message: msg });
                    false
                }
            }
        };

        // ── Step 2: Test Execution（必須）— build 失敗なら実行しない ───────
        let test_ok = if !build_ok {
            diagnostics.push("test: skipped (build failed)".to_string());
            false
        } else if self.options.skip_tests {
            diagnostics.push("test: skipped".to_string());
            true
        } else {
            match run_test_check(&self.root) {
                Ok(true) => {
                    diagnostics.push("test: ok".to_string());
                    true
                }
                Ok(false) => {
                    let msg = "cargo test: test failures detected".to_string();
                    diagnostics.push(format!("test: FAILED — {msg}"));
                    failures.push(SemanticValidationFailure::TestFailure {
                        failed_count: 1,
                        message: msg,
                    });
                    false
                }
                Err(err) => {
                    let msg = format!("cargo test error: {err}");
                    diagnostics.push(format!("test: ERROR — {msg}"));
                    failures.push(SemanticValidationFailure::TestFailure {
                        failed_count: 0,
                        message: msg,
                    });
                    false
                }
            }
        };

        // ── Step 3: Static Analysis（必須）────────────────────────────────
        let static_ok = if self.options.skip_static {
            diagnostics.push("static: skipped".to_string());
            true
        } else {
            match run_static_check(&self.root) {
                Ok(true) => {
                    diagnostics.push("static: ok".to_string());
                    true
                }
                Ok(false) => {
                    let msg = "cargo clippy: critical warnings detected".to_string();
                    diagnostics.push(format!("static: FAILED — {msg}"));
                    failures
                        .push(SemanticValidationFailure::SemanticRegression { description: msg });
                    false
                }
                Err(err) => {
                    let msg = format!("cargo clippy error: {err}");
                    diagnostics.push(format!("static: ERROR — {msg}"));
                    failures
                        .push(SemanticValidationFailure::SemanticRegression { description: msg });
                    false
                }
            }
        };

        // ── Step 4: Runtime Check（任意）──────────────────────────────────
        let runtime_ok = if self.options.skip_runtime {
            diagnostics.push("runtime: skipped".to_string());
            true
        } else {
            match run_runtime_probe(&self.root) {
                Ok(true) => {
                    diagnostics.push("runtime: ok".to_string());
                    true
                }
                Ok(false) => {
                    let msg = "runtime probe: unexpected failure".to_string();
                    diagnostics.push(format!("runtime: FAILED — {msg}"));
                    failures.push(SemanticValidationFailure::RuntimePanic { message: msg });
                    false
                }
                Err(err) => {
                    let msg = format!("runtime probe error: {err}");
                    diagnostics.push(format!("runtime: ERROR — {msg}"));
                    failures.push(SemanticValidationFailure::RuntimePanic { message: msg });
                    false
                }
            }
        };

        let overall =
            SemanticValidationResult::compute_overall(build_ok, test_ok, static_ok, runtime_ok);

        SemanticValidationResult {
            build_ok,
            test_ok,
            static_ok,
            runtime_ok,
            overall,
            failures,
            diagnostics,
        }
    }
}

// ── SemanticApplyGuard (Phase 7: 6.1) ─────────────────────────────────────────

/// Phase 7: 6.1 Apply後フロー — 検証失敗時に自動ロールバックするガード
///
/// ```text
/// apply
/// → validate
/// → if success → commit
/// → else → rollback (Phase 7: 6.2 Rollback仕様)
/// ```
pub struct SemanticApplyGuard {
    snapshot: WorkspaceSnapshot,
    pipeline: ValidationPipeline,
}

impl SemanticApplyGuard {
    /// `snapshot` は Apply 前に `snapshot_workspace()` で取得したもの
    pub fn new(
        snapshot: WorkspaceSnapshot,
        root: impl Into<PathBuf>,
        options: ValidationPipelineOptions,
    ) -> Self {
        Self {
            snapshot,
            pipeline: ValidationPipeline::new(root, options),
        }
    }

    /// 検証を実行し、失敗した場合はロールバックする
    ///
    /// Returns `(validation_result, rolled_back)`
    /// - `rolled_back = true` → rollback が実行されたことを示す
    pub fn validate_and_guard(self) -> (SemanticValidationResult, bool) {
        let result = self.pipeline.run();
        let rolled_back = if result.requires_rollback() {
            // Phase 7: 6.2 Rollback: トリガー = build_fail, test_fail, panic
            let _ = rollback_apply(&self.snapshot);
            true
        } else {
            false
        };
        (result, rolled_back)
    }
}

// ── Cargo command runners ──────────────────────────────────────────────────────

/// cargo サブコマンドを実行し (success, stdout, stderr) を返す
pub fn run_cargo_command(
    root: &Path,
    subcommand: &str,
    extra_args: &[&str],
) -> Result<(bool, String, String), String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root);
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd.env("CARGO_NET_OFFLINE", "true");
    cmd.arg(subcommand);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().map_err(|err| err.to_string())?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// 5.1 Build Validation: `cargo check`
pub fn run_build_check(root: &Path) -> Result<bool, String> {
    let (ok, _, _) = run_cargo_command(root, "check", &[])?;
    Ok(ok)
}

/// 5.2 Test Validation: `cargo test`
pub fn run_test_check(root: &Path) -> Result<bool, String> {
    let (ok, stdout, stderr) = run_cargo_command(root, "test", &[])?;
    if !ok {
        // FAILED カウントを stderr/stdout から抽出
        let output = format!("{stdout}{stderr}");
        let _ = parse_test_failure_count(&output);
    }
    Ok(ok)
}

/// 5.3 Static Analysis: `cargo clippy -- -D warnings`
pub fn run_static_check(root: &Path) -> Result<bool, String> {
    let (ok, _, _) = run_cargo_command(root, "clippy", &["--", "-D", "warnings"])?;
    Ok(ok)
}

/// 5.4 Runtime Validation: `cargo run --bin design_cli -- analyze .`（任意）
pub fn run_runtime_probe(root: &Path) -> Result<bool, String> {
    let (ok, _, _) =
        run_cargo_command(root, "run", &["--bin", "design_cli", "--", "analyze", "."])?;
    Ok(ok)
}

/// テスト失敗数を出力から抽出（"N failed" パターン）
fn parse_test_failure_count(output: &str) -> usize {
    output
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            // "test result: FAILED. N passed; M failed;"
            if trimmed.starts_with("test result: FAILED") {
                trimmed.split(';').find_map(|part| {
                    let part = part.trim();
                    if part.ends_with("failed") {
                        part.split_whitespace().next()?.parse::<usize>().ok()
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .unwrap_or(1)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Phase 7-1: Build検証 ─────────────────────────────────────────────────

    /// Phase 7-1: Build 成功時の SemanticValidationResult が正しい
    #[test]
    fn phase_7_1_build_ok_produces_passed_result() {
        let result = SemanticValidationResult::passed();
        assert!(result.build_ok, "build_ok must be true");
        assert_eq!(result.overall, ValidationStatus::Passed);
        assert!(result.failures.is_empty());
    }

    /// Phase 7-1: Build 失敗時は Class J + Failed overall
    #[test]
    fn phase_7_1_build_failure_class_j() {
        let result = SemanticValidationResult {
            build_ok: false,
            test_ok: false,
            static_ok: true,
            runtime_ok: true,
            overall: ValidationStatus::Failed,
            failures: vec![SemanticValidationFailure::BuildFailure {
                message: "compile error: unresolved import".to_string(),
            }],
            diagnostics: vec!["build: FAILED".to_string()],
        };
        assert!(!result.build_ok);
        assert_eq!(result.overall, ValidationStatus::Failed);
        assert_eq!(result.failures[0].class_label(), "Class J");
        assert!(result.requires_rollback());
    }

    // ── Phase 7-2: Test検証 ──────────────────────────────────────────────────

    /// Phase 7-2: Test 成功時は Passed
    #[test]
    fn phase_7_2_test_ok_produces_passed() {
        let result = SemanticValidationResult::passed();
        assert!(result.test_ok, "test_ok must be true");
        assert_eq!(result.overall, ValidationStatus::Passed);
    }

    /// Phase 7-2: Test 失敗時は Class K + Failed overall
    #[test]
    fn phase_7_2_test_failure_class_k() {
        let result = SemanticValidationResult {
            build_ok: true,
            test_ok: false,
            static_ok: true,
            runtime_ok: true,
            overall: ValidationStatus::Failed,
            failures: vec![SemanticValidationFailure::TestFailure {
                failed_count: 3,
                message: "3 tests failed".to_string(),
            }],
            diagnostics: vec!["test: FAILED".to_string()],
        };
        assert!(result.build_ok);
        assert!(!result.test_ok);
        assert_eq!(result.overall, ValidationStatus::Failed);
        assert_eq!(result.failures[0].class_label(), "Class K");
        assert!(result.requires_rollback());
    }

    // ── Phase 7-3: Rollback検証 ──────────────────────────────────────────────

    /// Phase 7-3: 失敗結果はロールバックを要求する
    #[test]
    fn phase_7_3_failed_result_requires_rollback() {
        let result = SemanticValidationResult {
            build_ok: false,
            test_ok: false,
            static_ok: true,
            runtime_ok: true,
            overall: ValidationStatus::Failed,
            failures: vec![SemanticValidationFailure::BuildFailure {
                message: "compile error".to_string(),
            }],
            diagnostics: Vec::new(),
        };
        assert!(
            result.requires_rollback(),
            "Failed overall must trigger rollback"
        );
    }

    /// Phase 7-3: 成功結果はロールバック不要
    #[test]
    fn phase_7_3_passed_result_no_rollback() {
        let result = SemanticValidationResult::passed();
        assert!(
            !result.requires_rollback(),
            "Passed result must NOT trigger rollback"
        );
    }

    /// Phase 7-3: RuntimePanic（Class M）はロールバックをトリガーする
    #[test]
    fn phase_7_3_runtime_panic_triggers_rollback() {
        let result = SemanticValidationResult {
            build_ok: true,
            test_ok: true,
            static_ok: true,
            runtime_ok: false,
            // overall は Passed だが RuntimePanic があるのでロールバック必要
            overall: ValidationStatus::Passed,
            failures: vec![SemanticValidationFailure::RuntimePanic {
                message: "thread panicked at 'overflow'".to_string(),
            }],
            diagnostics: Vec::new(),
        };
        assert!(
            result.requires_rollback(),
            "RuntimePanic must trigger rollback regardless of overall"
        );
        assert_eq!(result.failures[0].class_label(), "Class M");
    }

    /// Phase 7-3: dry_run ガードは成功・ロールバックなし
    #[test]
    fn phase_7_3_dry_run_guard_passes_without_rollback() {
        let snapshot = WorkspaceSnapshot {
            entries: Vec::new(),
        };
        let guard = SemanticApplyGuard::new(
            snapshot,
            std::env::temp_dir(),
            ValidationPipelineOptions::dry_run(),
        );
        let (result, rolled_back) = guard.validate_and_guard();
        assert_eq!(
            result.overall,
            ValidationStatus::Passed,
            "dry_run must pass"
        );
        assert!(!rolled_back, "No rollback on success");
    }

    // ── Phase 7-4: Deterministic Validation ──────────────────────────────────

    /// Phase 7-4: 同一入力×3で検証結果が一致する（決定論性）
    #[test]
    fn phase_7_4_dry_run_is_deterministic() {
        let root = std::env::temp_dir();
        let opts = ValidationPipelineOptions::dry_run();
        let r1 = ValidationPipeline::new(root.clone(), opts.clone()).run();
        let r2 = ValidationPipeline::new(root.clone(), opts.clone()).run();
        let r3 = ValidationPipeline::new(root.clone(), opts.clone()).run();
        // 全実行で overall が一致
        assert_eq!(r1.overall, r2.overall, "Run 1 vs 2 overall must match");
        assert_eq!(r2.overall, r3.overall, "Run 2 vs 3 overall must match");
        // 必須項目フラグも一致
        assert_eq!(r1.build_ok, r2.build_ok);
        assert_eq!(r1.test_ok, r2.test_ok);
        assert_eq!(r1.static_ok, r2.static_ok);
        assert_eq!(r1.failures.len(), r2.failures.len());
    }

    // ── ValidationStatus logic ────────────────────────────────────────────────

    /// compute_overall: build+test+static OK → Passed
    #[test]
    fn compute_overall_all_ok_is_passed() {
        assert_eq!(
            SemanticValidationResult::compute_overall(true, true, true, true),
            ValidationStatus::Passed
        );
    }

    /// compute_overall: build OK, test OK, static FAIL → Partial
    #[test]
    fn compute_overall_static_fail_is_partial() {
        assert_eq!(
            SemanticValidationResult::compute_overall(true, true, false, true),
            ValidationStatus::Partial
        );
    }

    /// compute_overall: build FAIL → Failed
    #[test]
    fn compute_overall_build_fail_is_failed() {
        assert_eq!(
            SemanticValidationResult::compute_overall(false, false, true, true),
            ValidationStatus::Failed
        );
    }

    /// compute_overall: test FAIL → Failed
    #[test]
    fn compute_overall_test_fail_is_failed() {
        assert_eq!(
            SemanticValidationResult::compute_overall(true, false, true, true),
            ValidationStatus::Failed
        );
    }

    // ── ValidationPipeline dry_run ────────────────────────────────────────────

    /// dry_run パイプラインは全項目 skipped + Passed を返す
    #[test]
    fn pipeline_dry_run_all_skipped_passed() {
        let pipeline =
            ValidationPipeline::new(std::env::temp_dir(), ValidationPipelineOptions::dry_run());
        let result = pipeline.run();
        assert_eq!(result.overall, ValidationStatus::Passed);
        assert!(result.build_ok);
        assert!(result.test_ok);
        assert!(result.static_ok);
        assert!(result.runtime_ok);
        assert!(result.failures.is_empty());
        assert!(
            result.diagnostics.iter().all(|d| d.contains("skipped")),
            "All steps must be marked skipped in dry_run"
        );
    }

    /// build 失敗後は test が自動的にスキップされる
    #[test]
    fn pipeline_test_skipped_when_build_fails() {
        // build=false(実際には skip=true だが、手動で結果を構築して検証)
        let result = SemanticValidationResult {
            build_ok: false,
            test_ok: false, // build failed → test skipped (=false)
            static_ok: true,
            runtime_ok: true,
            overall: SemanticValidationResult::compute_overall(false, false, true, true),
            failures: vec![SemanticValidationFailure::BuildFailure {
                message: "test".to_string(),
            }],
            diagnostics: vec![
                "build: FAILED".to_string(),
                "test: skipped (build failed)".to_string(),
            ],
        };
        assert_eq!(result.overall, ValidationStatus::Failed);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.contains("skipped (build failed)"))
        );
    }

    // ── SemanticQualityScore ──────────────────────────────────────────────────

    /// Passed 結果から算出したスコアは高い
    #[test]
    fn quality_score_high_on_passed_result() {
        let result = SemanticValidationResult::passed();
        let score = SemanticQualityScore::from_result(&result);
        let total = score.total();
        assert!(
            total > 0.9,
            "Passed result must yield quality > 0.9, got {total:.3}"
        );
        assert!(
            (score.behavior - 1.0).abs() < f32::EPSILON,
            "behavior must be 1.0"
        );
        assert!(
            (score.semantic - 1.0).abs() < f32::EPSILON,
            "semantic must be 1.0"
        );
        assert!(
            (score.safety - 1.0).abs() < f32::EPSILON,
            "safety must be 1.0"
        );
    }

    /// Failed 結果からのスコアは低い
    #[test]
    fn quality_score_low_on_failed_result() {
        let result = SemanticValidationResult {
            build_ok: false,
            test_ok: false,
            static_ok: false,
            runtime_ok: true,
            overall: ValidationStatus::Failed,
            failures: vec![
                SemanticValidationFailure::BuildFailure {
                    message: "error".to_string(),
                },
                SemanticValidationFailure::TestFailure {
                    failed_count: 5,
                    message: "5 tests failed".to_string(),
                },
            ],
            diagnostics: Vec::new(),
        };
        let score = SemanticQualityScore::from_result(&result);
        let total = score.total();
        assert!(
            total < 0.5,
            "Failed result must yield quality < 0.5, got {total:.3}"
        );
        assert!(
            (score.behavior - 0.0).abs() < f32::EPSILON,
            "behavior must be 0.0"
        );
        assert!(
            (score.safety - 0.0).abs() < f32::EPSILON,
            "safety must be 0.0"
        );
    }

    /// 重み付けが仕様通りか: total = Σ(weight * score)
    #[test]
    fn quality_score_weights_sum_to_one() {
        let score = SemanticQualityScore {
            structural: 1.0,
            design: 1.0,
            semantic: 1.0,
            behavior: 1.0,
            determinism: 1.0,
            safety: 1.0,
        };
        let total = score.total();
        assert!(
            (total - 1.0).abs() < 1e-6,
            "All-1.0 scores must total 1.0, got {total}"
        );
    }

    // ── Failure classification labels ─────────────────────────────────────────

    #[test]
    fn failure_class_labels_match_spec() {
        assert_eq!(
            SemanticValidationFailure::BuildFailure {
                message: String::new()
            }
            .class_label(),
            "Class J"
        );
        assert_eq!(
            SemanticValidationFailure::TestFailure {
                failed_count: 0,
                message: String::new()
            }
            .class_label(),
            "Class K"
        );
        assert_eq!(
            SemanticValidationFailure::SemanticRegression {
                description: String::new()
            }
            .class_label(),
            "Class L"
        );
        assert_eq!(
            SemanticValidationFailure::RuntimePanic {
                message: String::new()
            }
            .class_label(),
            "Class M"
        );
    }

    // ── summary output ────────────────────────────────────────────────────────

    #[test]
    fn summary_reflects_status() {
        let passed = SemanticValidationResult::passed();
        assert!(passed.summary().contains("PASSED"));
        let failed = SemanticValidationResult {
            build_ok: false,
            test_ok: false,
            static_ok: true,
            runtime_ok: true,
            overall: ValidationStatus::Failed,
            failures: vec![SemanticValidationFailure::BuildFailure {
                message: "err".to_string(),
            }],
            diagnostics: Vec::new(),
        };
        let summary = failed.summary();
        assert!(summary.contains("FAILED"), "summary: {summary}");
        assert!(
            summary.contains("Class J"),
            "summary must include class label"
        );
    }

    // ── parse_test_failure_count ──────────────────────────────────────────────

    #[test]
    fn parses_test_failure_count_from_cargo_output() {
        let output = "test result: FAILED. 10 passed; 3 failed; 0 ignored";
        assert_eq!(parse_test_failure_count(output), 3);
    }

    #[test]
    fn parse_test_failure_count_returns_one_on_unknown() {
        assert_eq!(parse_test_failure_count("unknown output"), 1);
    }
}
