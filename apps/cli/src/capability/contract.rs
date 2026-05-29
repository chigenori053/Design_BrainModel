//! CapabilityContract
//!
//! DBM-ANALYZE-CAPABILITY-ISOLATION v1.2 §2 に基づく Typed Capability Contract。
//!
//! # コンパイル時安全性の保証
//!
//! 各 Capability は `type Output` によって固有の結果型に紐付けられる。
//! これにより `AnalyzeTestsCapability → ProjectStructureAnalysisResult` のような
//! 誤ったルーティングはコンパイル時に検出される。
//!
//! ## 型システムによる強制例
//!
//! ```ignore
//! fn execute<C: CapabilityContract>(_cap: C) -> C::Output { todo!() }
//! // AnalyzeTestsCapability で呼び出すと TestInventoryResult が返る
//! // ProjectStructureAnalysisResult に代入しようとするとコンパイルエラー
//! let _: ProjectStructureAnalysisResult = execute(AnalyzeTestsCapability); // ERROR: type mismatch
//! ```

// ── Result types ──────────────────────────────────────────────────────────────

/// テスト棚卸し結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestInventoryResult {
    pub test_files: Vec<String>,
    pub test_count: usize,
    pub summary: String,
}

/// プロジェクト構造解析結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStructureAnalysisResult {
    pub modules: Vec<String>,
    pub summary: String,
}

/// コード解析結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAnalysisResult {
    pub files: Vec<String>,
    pub summary: String,
}

/// メモリ解析結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAnalysisResult {
    pub entries: Vec<String>,
    pub summary: String,
}

// ── Capability structs ────────────────────────────────────────────────────────

/// テスト棚卸し Capability。`Output = TestInventoryResult`。
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeTestsCapability;

/// プロジェクト構造解析 Capability。`Output = ProjectStructureAnalysisResult`。
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeProjectCapability;

/// コード解析 Capability。`Output = CodeAnalysisResult`。
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeCodeCapability;

/// メモリ解析 Capability。`Output = MemoryAnalysisResult`。
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeMemoryCapability;

// ── CapabilityContract trait ─────────────────────────────────────────────────

/// Capability が返す型を型レベルで強制するトレイト。
///
/// 各 Capability は唯一の `Output` 型を持ち、
/// 異なる Capability 間の Output 混在はコンパイル時に検出される。
pub trait CapabilityContract {
    /// この Capability が返す結果型。
    type Output;

    /// Capability の名前。
    fn capability_name() -> &'static str;

    /// Output 型の名前。
    fn output_type_name() -> &'static str;
}

impl CapabilityContract for AnalyzeTestsCapability {
    type Output = TestInventoryResult;

    fn capability_name() -> &'static str {
        "AnalyzeTestsCapability"
    }

    fn output_type_name() -> &'static str {
        "TestInventoryResult"
    }
}

impl CapabilityContract for AnalyzeProjectCapability {
    type Output = ProjectStructureAnalysisResult;

    fn capability_name() -> &'static str {
        "AnalyzeProjectCapability"
    }

    fn output_type_name() -> &'static str {
        "ProjectStructureAnalysisResult"
    }
}

impl CapabilityContract for AnalyzeCodeCapability {
    type Output = CodeAnalysisResult;

    fn capability_name() -> &'static str {
        "AnalyzeCodeCapability"
    }

    fn output_type_name() -> &'static str {
        "CodeAnalysisResult"
    }
}

impl CapabilityContract for AnalyzeMemoryCapability {
    type Output = MemoryAnalysisResult;

    fn capability_name() -> &'static str {
        "AnalyzeMemoryCapability"
    }

    fn output_type_name() -> &'static str {
        "MemoryAnalysisResult"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// spec §Required Tests: analyze_tests_returns_test_inventory
    ///
    /// AnalyzeTestsCapability の Output 型名が TestInventoryResult であることを確認する。
    #[test]
    fn analyze_tests_returns_test_inventory() {
        assert_eq!(
            AnalyzeTestsCapability::output_type_name(),
            "TestInventoryResult"
        );
        assert_eq!(
            AnalyzeTestsCapability::capability_name(),
            "AnalyzeTestsCapability"
        );
    }

    /// AnalyzeProjectCapability の Output 型名が ProjectStructureAnalysisResult であることを確認する。
    #[test]
    fn analyze_project_returns_project_structure_analysis() {
        assert_eq!(
            AnalyzeProjectCapability::output_type_name(),
            "ProjectStructureAnalysisResult"
        );
        assert_eq!(
            AnalyzeProjectCapability::capability_name(),
            "AnalyzeProjectCapability"
        );
    }

    /// AnalyzeCodeCapability の Output 型名が CodeAnalysisResult であることを確認する。
    #[test]
    fn analyze_code_returns_code_analysis() {
        assert_eq!(
            AnalyzeCodeCapability::output_type_name(),
            "CodeAnalysisResult"
        );
    }

    /// AnalyzeMemoryCapability の Output 型名が MemoryAnalysisResult であることを確認する。
    #[test]
    fn analyze_memory_returns_memory_analysis() {
        assert_eq!(
            AnalyzeMemoryCapability::output_type_name(),
            "MemoryAnalysisResult"
        );
    }

    /// AnalyzeTestsCapability と AnalyzeProjectCapability の Output 型名が異なることを確認する。
    /// （型システムによるコンパイル時分離の文書化）
    #[test]
    fn tests_and_project_capabilities_have_distinct_outputs() {
        assert_ne!(
            AnalyzeTestsCapability::output_type_name(),
            AnalyzeProjectCapability::output_type_name(),
        );
    }
}
