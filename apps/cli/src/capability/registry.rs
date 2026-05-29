//! CapabilityRegistry
//!
//! DBM-ANALYZE-CAPABILITY-ISOLATION v1.2 §1 に基づく Capability Registry。
//!
//! `IrAction` → `CapabilityKind` の解決を一元管理する。
//! 以下を禁止する:
//! - `if intent.contains("analyze")` によるキーワード推測
//! - `match keyword` による暗黙的な分岐
//! - `GenericAnalyzeRuntime` 経由の Capability 推測
//!
//! 明示的なマッピングテーブルのみを許可する。

use std::fmt;

use crate::nl::language_core_ir_adapter::IrAction;

// ── CapabilityKind ────────────────────────────────────────────────────────────

/// Capability 種別。
///
/// 各 `IrAction` は一意の `CapabilityKind` に解決される。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityKind {
    /// プロジェクト構造解析。
    ///
    /// 対応する `IrAction`: `AnalyzeProject`, `AnalyzeWorkspace`,
    /// `AnalyzeDependencies`, `AnalyzeModuleStructure`
    AnalyzeProject,

    /// テスト棚卸し。
    ///
    /// 対応する `IrAction`: `AnalyzeTests`
    AnalyzeTests,

    /// コード解析。
    ///
    /// 対応する `IrAction`: `AnalyzeFile`, `AnalyzeSymbol`
    AnalyzeCode,

    /// メモリ解析（将来拡張用）。
    AnalyzeMemory,
}

impl fmt::Display for CapabilityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnalyzeProject => write!(f, "AnalyzeProjectCapability"),
            Self::AnalyzeTests => write!(f, "AnalyzeTestsCapability"),
            Self::AnalyzeCode => write!(f, "AnalyzeCodeCapability"),
            Self::AnalyzeMemory => write!(f, "AnalyzeMemoryCapability"),
        }
    }
}

// ── CapabilityResolutionError ─────────────────────────────────────────────────

/// Capability 解決エラー。
///
/// 対応する Capability が存在しない `IrAction` に対して返される。
#[derive(Debug, Clone)]
pub struct CapabilityResolutionError {
    /// 解決に失敗した `IrAction` 名。
    pub action: String,
    /// エラーの詳細メッセージ。
    pub message: String,
}

impl fmt::Display for CapabilityResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CapabilityResolutionError: action={} {}",
            self.action, self.message
        )
    }
}

// ── CapabilityRegistry ────────────────────────────────────────────────────────

/// Capability Registry。
///
/// `IrAction` を対応する `CapabilityKind` に解決する。
///
/// ## 解決ルール（明示的マッピングテーブル）
///
/// | IrAction                                                                     | CapabilityKind       |
/// |------------------------------------------------------------------------------|----------------------|
/// | AnalyzeProject, AnalyzeWorkspace, AnalyzeDependencies, AnalyzeModuleStructure | AnalyzeProject       |
/// | AnalyzeTests                                                                  | AnalyzeTests         |
/// | AnalyzeFile, AnalyzeSymbol                                                    | AnalyzeCode          |
/// | (その他)                                                                       | CapabilityResolutionError |
pub struct CapabilityRegistry;

impl CapabilityRegistry {
    /// `IrAction` を `CapabilityKind` に解決する。
    ///
    /// Analyze 系以外の `IrAction` は `CapabilityResolutionError` を返す。
    pub fn resolve(action: &IrAction) -> Result<CapabilityKind, CapabilityResolutionError> {
        match action {
            // プロジェクト構造系 → AnalyzeProjectCapability
            IrAction::AnalyzeProject
            | IrAction::AnalyzeWorkspace
            | IrAction::AnalyzeDependencies
            | IrAction::AnalyzeModuleStructure => Ok(CapabilityKind::AnalyzeProject),

            // テスト棚卸し → AnalyzeTestsCapability
            IrAction::AnalyzeTests => Ok(CapabilityKind::AnalyzeTests),

            // コード解析系 → AnalyzeCodeCapability
            IrAction::AnalyzeFile | IrAction::AnalyzeSymbol => Ok(CapabilityKind::AnalyzeCode),

            // Analyze 系以外は解決不能
            other => Err(CapabilityResolutionError {
                action: format!("{other}"),
                message: "action does not map to any registered Capability".to_string(),
            }),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// spec §Required Tests: analyze_tests_dispatches_to_tests_capability
    #[test]
    fn analyze_tests_dispatches_to_tests_capability() {
        let capability = CapabilityRegistry::resolve(&IrAction::AnalyzeTests).unwrap();
        assert_eq!(capability, CapabilityKind::AnalyzeTests);
    }

    /// spec §Required Tests: capability_registry_resolves_correctly
    #[test]
    fn capability_registry_resolves_correctly() {
        // AnalyzeProject 系 → AnalyzeProject
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeProject).unwrap(),
            CapabilityKind::AnalyzeProject
        );
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeWorkspace).unwrap(),
            CapabilityKind::AnalyzeProject
        );
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeDependencies).unwrap(),
            CapabilityKind::AnalyzeProject
        );
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeModuleStructure).unwrap(),
            CapabilityKind::AnalyzeProject
        );

        // AnalyzeTests → AnalyzeTests
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeTests).unwrap(),
            CapabilityKind::AnalyzeTests
        );

        // コード解析系 → AnalyzeCode
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeFile).unwrap(),
            CapabilityKind::AnalyzeCode
        );
        assert_eq!(
            CapabilityRegistry::resolve(&IrAction::AnalyzeSymbol).unwrap(),
            CapabilityKind::AnalyzeCode
        );
    }

    /// AnalyzeProject は AnalyzeTestsCapability に解決されない。
    #[test]
    fn analyze_project_does_not_dispatch_to_tests_capability() {
        let capability = CapabilityRegistry::resolve(&IrAction::AnalyzeProject).unwrap();
        assert_ne!(capability, CapabilityKind::AnalyzeTests);
    }

    /// AnalyzeTests は AnalyzeProjectCapability に解決されない。
    #[test]
    fn analyze_tests_does_not_dispatch_to_project_capability() {
        let capability = CapabilityRegistry::resolve(&IrAction::AnalyzeTests).unwrap();
        assert_ne!(capability, CapabilityKind::AnalyzeProject);
    }

    /// Analyze 系以外の IrAction は CapabilityResolutionError を返す。
    #[test]
    fn non_analyze_action_returns_resolution_error() {
        assert!(CapabilityRegistry::resolve(&IrAction::Apply).is_err());
        assert!(CapabilityRegistry::resolve(&IrAction::GenerateChangePlan).is_err());
        assert!(CapabilityRegistry::resolve(&IrAction::Refactor).is_err());
        assert!(CapabilityRegistry::resolve(&IrAction::Unknown).is_err());
    }

    /// CapabilityKind の Display が正しいキャパビリティ名を返す。
    #[test]
    fn capability_kind_display_is_correct() {
        assert_eq!(
            CapabilityKind::AnalyzeProject.to_string(),
            "AnalyzeProjectCapability"
        );
        assert_eq!(
            CapabilityKind::AnalyzeTests.to_string(),
            "AnalyzeTestsCapability"
        );
        assert_eq!(
            CapabilityKind::AnalyzeCode.to_string(),
            "AnalyzeCodeCapability"
        );
        assert_eq!(
            CapabilityKind::AnalyzeMemory.to_string(),
            "AnalyzeMemoryCapability"
        );
    }
}
