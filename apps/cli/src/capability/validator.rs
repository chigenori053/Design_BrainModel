//! RuntimeCapabilityValidator
//!
//! DBM-ANALYZE-CAPABILITY-ISOLATION v1.2 §3 に基づく実行時 Capability 検証器。
//!
//! (Intent, Capability, OutputType) の整合性を実行時に確認し、
//! 不整合が検出された場合は `CapabilityMismatchError` を返す。
//!
//! # 検証ステージ
//!
//! 1. `IrAction` → `CapabilityKind` のマッピングが Registry と一致するか
//! 2. `CapabilityKind` → `OutputTypeId` のマッピングが Contract と一致するか

use std::fmt;

use crate::capability::registry::{CapabilityKind, CapabilityRegistry};
use crate::nl::language_core_ir_adapter::IrAction;

// ── OutputTypeId ─────────────────────────────────────────────────────────────

/// 実行時に識別可能な Output 型 ID。
///
/// `CapabilityContract::type Output` の実行時表現。
/// 型システムのコンパイル時保証を補完する実行時チェックに使用する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTypeId {
    TestInventoryResult,
    ProjectStructureAnalysisResult,
    CodeAnalysisResult,
    MemoryAnalysisResult,
}

impl fmt::Display for OutputTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TestInventoryResult => write!(f, "TestInventoryResult"),
            Self::ProjectStructureAnalysisResult => write!(f, "ProjectStructureAnalysisResult"),
            Self::CodeAnalysisResult => write!(f, "CodeAnalysisResult"),
            Self::MemoryAnalysisResult => write!(f, "MemoryAnalysisResult"),
        }
    }
}

// ── CapabilityMismatchError ───────────────────────────────────────────────────

/// Capability と Output 型の不整合エラー。
///
/// `RuntimeCapabilityValidator::validate` が不整合を検出した際に返す。
#[derive(Debug, Clone)]
pub struct CapabilityMismatchError {
    /// 元の IrAction (intent) の文字列表現。
    pub intent: String,
    /// 使用された Capability 種別。
    pub capability: CapabilityKind,
    /// 実際の Output 型 ID。
    pub result_type: OutputTypeId,
    /// エラーの詳細メッセージ。
    pub message: String,
}

impl fmt::Display for CapabilityMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CapabilityMismatchError: intent={} capability={} result_type={} [{}]",
            self.intent, self.capability, self.result_type, self.message
        )
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// 各 Capability に対応する期待 Output 型 ID。
///
/// `CapabilityContract::type Output` の実行時表現。
fn expected_output_for_capability(capability: CapabilityKind) -> OutputTypeId {
    match capability {
        CapabilityKind::AnalyzeProject => OutputTypeId::ProjectStructureAnalysisResult,
        CapabilityKind::AnalyzeTests => OutputTypeId::TestInventoryResult,
        CapabilityKind::AnalyzeCode => OutputTypeId::CodeAnalysisResult,
        CapabilityKind::AnalyzeMemory => OutputTypeId::MemoryAnalysisResult,
    }
}

// ── RuntimeCapabilityValidator ────────────────────────────────────────────────

/// 実行時 Capability 検証器。
///
/// `(IrAction, CapabilityKind, OutputTypeId)` の整合性を2段階で確認する:
///
/// ## ステージ 1: IrAction → CapabilityKind の整合性
///
/// `CapabilityRegistry::resolve` が返す期待 `CapabilityKind` と
/// 実際に使用された `capability` が一致するか確認する。
///
/// ## ステージ 2: CapabilityKind → OutputTypeId の整合性
///
/// 使用された `capability` に対して期待される `OutputTypeId` と
/// 実際の `output` が一致するか確認する。
pub struct RuntimeCapabilityValidator;

impl RuntimeCapabilityValidator {
    /// `(action, capability, output)` の整合性を検証する。
    ///
    /// すべての整合性が確認できた場合 `Ok(())` を返す。
    /// 不整合があれば `CapabilityMismatchError` を返す。
    pub fn validate(
        action: &IrAction,
        capability: CapabilityKind,
        output: OutputTypeId,
    ) -> Result<(), CapabilityMismatchError> {
        // ── Stage 1: IrAction → CapabilityKind ───────────────────────────────
        match CapabilityRegistry::resolve(action) {
            Ok(expected_cap) if expected_cap != capability => {
                return Err(CapabilityMismatchError {
                    intent: format!("{action}"),
                    capability,
                    result_type: output,
                    message: format!(
                        "action {action} maps to {expected_cap} but capability {capability} was used"
                    ),
                });
            }
            Err(e) => {
                return Err(CapabilityMismatchError {
                    intent: format!("{action}"),
                    capability,
                    result_type: output,
                    message: e.message,
                });
            }
            Ok(_) => {}
        }

        // ── Stage 2: CapabilityKind → OutputTypeId ────────────────────────────
        let expected_output = expected_output_for_capability(capability);
        if output != expected_output {
            return Err(CapabilityMismatchError {
                intent: format!("{action}"),
                capability,
                result_type: output,
                message: format!(
                    "capability {capability} expects output {expected_output} but got {output}"
                ),
            });
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常系: AnalyzeTests + AnalyzeTestsCapability + TestInventoryResult → Ok
    #[test]
    fn runtime_accepts_correct_tests_combination() {
        assert!(
            RuntimeCapabilityValidator::validate(
                &IrAction::AnalyzeTests,
                CapabilityKind::AnalyzeTests,
                OutputTypeId::TestInventoryResult,
            )
            .is_ok()
        );
    }

    /// 正常系: AnalyzeProject + AnalyzeProjectCapability + ProjectStructureAnalysisResult → Ok
    #[test]
    fn runtime_accepts_correct_project_combination() {
        assert!(
            RuntimeCapabilityValidator::validate(
                &IrAction::AnalyzeProject,
                CapabilityKind::AnalyzeProject,
                OutputTypeId::ProjectStructureAnalysisResult,
            )
            .is_ok()
        );
    }

    /// spec §Required Tests: analyze_tests_cannot_return_project_structure
    ///
    /// AnalyzeTests → ProjectStructureAnalysisResult は Runtime Validator が拒否する。
    #[test]
    fn analyze_tests_cannot_return_project_structure() {
        let err = RuntimeCapabilityValidator::validate(
            &IrAction::AnalyzeTests,
            CapabilityKind::AnalyzeTests,
            OutputTypeId::ProjectStructureAnalysisResult,
        )
        .unwrap_err();
        assert_eq!(
            err.result_type,
            OutputTypeId::ProjectStructureAnalysisResult,
            "result_type should be ProjectStructureAnalysisResult"
        );
        assert_eq!(err.capability, CapabilityKind::AnalyzeTests);
    }

    /// spec §Required Tests: analyze_project_cannot_return_test_inventory
    ///
    /// AnalyzeProject → TestInventoryResult は Runtime Validator が拒否する。
    #[test]
    fn analyze_project_cannot_return_test_inventory() {
        let err = RuntimeCapabilityValidator::validate(
            &IrAction::AnalyzeProject,
            CapabilityKind::AnalyzeProject,
            OutputTypeId::TestInventoryResult,
        )
        .unwrap_err();
        assert_eq!(
            err.result_type,
            OutputTypeId::TestInventoryResult,
            "result_type should be TestInventoryResult"
        );
        assert_eq!(err.capability, CapabilityKind::AnalyzeProject);
    }

    /// spec §Required Tests: runtime_rejects_capability_mismatch
    ///
    /// 正しくない (Action, Capability, Output) の組み合わせを拒否する。
    #[test]
    fn runtime_rejects_capability_mismatch() {
        // AnalyzeProject + AnalyzeProject + TestInventoryResult → エラー（Stage 2）
        let err = RuntimeCapabilityValidator::validate(
            &IrAction::AnalyzeProject,
            CapabilityKind::AnalyzeProject,
            OutputTypeId::TestInventoryResult,
        )
        .unwrap_err();
        assert_eq!(err.result_type, OutputTypeId::TestInventoryResult);
        assert_eq!(err.capability, CapabilityKind::AnalyzeProject);
        // エラーメッセージに不整合の詳細が含まれる
        assert!(
            err.message.contains("TestInventoryResult")
                || err.message.contains("ProjectStructureAnalysisResult"),
            "message should mention the type mismatch: {}",
            err.message
        );
    }

    /// Stage 1 検証: AnalyzeTests に AnalyzeProjectCapability を使うと即座に拒否される。
    #[test]
    fn wrong_capability_for_action_is_rejected_at_stage1() {
        let err = RuntimeCapabilityValidator::validate(
            &IrAction::AnalyzeTests,
            CapabilityKind::AnalyzeProject,
            OutputTypeId::ProjectStructureAnalysisResult,
        )
        .unwrap_err();
        // Stage 1 エラー: action は AnalyzeTestsCapability を期待するが AnalyzeProjectCapability が使われた
        assert!(
            err.message.contains("AnalyzeTests") || err.message.contains("AnalyzeTestsCapability"),
            "message should reference AnalyzeTests: {}",
            err.message
        );
    }

    /// Stage 1 検証: AnalyzeProject に AnalyzeTestsCapability を使うと即座に拒否される。
    #[test]
    fn wrong_capability_for_project_action_is_rejected_at_stage1() {
        let err = RuntimeCapabilityValidator::validate(
            &IrAction::AnalyzeProject,
            CapabilityKind::AnalyzeTests,
            OutputTypeId::TestInventoryResult,
        )
        .unwrap_err();
        assert!(
            err.message.contains("AnalyzeProject")
                || err.message.contains("AnalyzeProjectCapability"),
            "message should reference AnalyzeProject: {}",
            err.message
        );
    }

    /// Non-Analyze アクションは Stage 1 で解決エラーとなる。
    #[test]
    fn non_analyze_action_fails_at_stage1() {
        let err = RuntimeCapabilityValidator::validate(
            &IrAction::Apply,
            CapabilityKind::AnalyzeProject,
            OutputTypeId::ProjectStructureAnalysisResult,
        )
        .unwrap_err();
        assert!(
            err.message.contains("does not map to any registered"),
            "message should indicate no registered capability: {}",
            err.message
        );
    }
}
