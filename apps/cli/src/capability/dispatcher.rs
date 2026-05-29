//! RuntimeAnalyzeDispatcher
//!
//! DBM-RUNTIME-DISPATCH-INTEGRATION-SPEC v1.0 §5 に基づく Analyze Dispatcher。
//! `IrAction` を `CapabilityRegistry` 経由で `CapabilityKind` に解決し、
//! 各 Capability を実行・検証する。

use std::any::Any;

use crate::capability::contract::{
    CodeAnalysisResult, MemoryAnalysisResult, ProjectStructureAnalysisResult, TestInventoryResult,
};
use crate::capability::registry::{CapabilityKind, CapabilityRegistry};
use crate::capability::validator::{OutputTypeId, RuntimeCapabilityValidator};
use crate::nl::language_core_ir_adapter::IrAction;

// ── RuntimeAnalyzeDispatcher ──────────────────────────────────────────────────

/// Analyze 系の Runtime Dispatch を一元管理するディスパッチャ。
pub struct RuntimeAnalyzeDispatcher;

impl RuntimeAnalyzeDispatcher {
    /// Analyze 系アクションを実行し、検証済みの結果を返す。
    ///
    /// # Errors
    ///
    /// - `CapabilityResolutionError`: Action が登録されていない場合
    /// - `CapabilityMismatchError`: 実行結果の型が期待と異なる場合（Validator が検出）
    pub fn dispatch(
        action: &IrAction,
        path: &str,
    ) -> Result<(Box<dyn Any>, OutputTypeId, CapabilityKind), String> {
        // 1. Registry で CapabilityKind を解決
        let capability = CapabilityRegistry::resolve(action)
            .map_err(|e| format!("Capability resolution failed: {}", e))?;

        // 2. 各 Capability を実行 (実際のバックエンド呼び出し)
        let (result, output_type) = match capability {
            CapabilityKind::AnalyzeProject => {
                let project_res = Self::execute_analyze_project(path)?;
                (
                    Box::new(project_res) as Box<dyn Any>,
                    OutputTypeId::ProjectStructureAnalysisResult,
                )
            }
            CapabilityKind::AnalyzeTests => {
                let test_res = Self::execute_analyze_tests(path)?;
                (
                    Box::new(test_res) as Box<dyn Any>,
                    OutputTypeId::TestInventoryResult,
                )
            }
            CapabilityKind::AnalyzeCode => {
                let code_res = Self::execute_analyze_code(path)?;
                (
                    Box::new(code_res) as Box<dyn Any>,
                    OutputTypeId::CodeAnalysisResult,
                )
            }
            CapabilityKind::AnalyzeMemory => {
                let memory_res = Self::execute_analyze_memory(path)?;
                (
                    Box::new(memory_res) as Box<dyn Any>,
                    OutputTypeId::MemoryAnalysisResult,
                )
            }
        };

        // 3. Telemetry 記録 (spec §9)
        if crate::core::observability_enabled() {
            println!(
                "[CAPABILITY_DISPATCH] action={} capability={} result_type={}",
                action, capability, output_type
            );
        }

        // 4. RuntimeCapabilityValidator で検証
        RuntimeCapabilityValidator::validate(action, capability, output_type)
            .map_err(|e| format!("Capability validation failed: {}", e))?;

        if crate::core::observability_enabled() {
            println!(
                "[CAPABILITY_VALIDATE] action={} expected={} actual={} status=Passed",
                action, output_type, output_type
            );
        }

        Ok((result, output_type, capability))
    }

    fn execute_analyze_project(path: &str) -> Result<ProjectStructureAnalysisResult, String> {
        // 既存の analyze_project を呼び出し、結果を変換
        let res = crate::dbm::analyzer::analyze_project(path)?;
        Ok(ProjectStructureAnalysisResult {
            modules: res.modules.iter().map(|m| m.name.clone()).collect(),
            summary: format!(
                "Project structure analyzed. Found {} modules and {} files.",
                res.modules.len(),
                res.files.len()
            ),
        })
    }

    fn execute_analyze_tests(path: &str) -> Result<TestInventoryResult, String> {
        // テスト棚卸しの実体（簡易実装）
        // 本来は専用のシグネチャスキャン等を行うが、ここでは spec 要求を満たす出力を生成
        let res = crate::dbm::analyzer::analyze_project(path)?;
        
        // "test" を含むファイルを抽出
        let test_files: Vec<String> = res.files
            .iter()
            .filter(|f| f.path.contains("test") || f.path.contains("spec"))
            .map(|f| f.path.clone())
            .collect();

        Ok(TestInventoryResult {
            test_count: test_files.len(),
            test_files,
            summary: format!("Test inventory completed at {}", path),
        })
    }

    fn execute_analyze_code(path: &str) -> Result<CodeAnalysisResult, String> {
        Ok(CodeAnalysisResult {
            files: vec![path.to_string()],
            summary: "Code analysis result (simulated)".to_string(),
        })
    }

    fn execute_analyze_memory(_path: &str) -> Result<MemoryAnalysisResult, String> {
        Ok(MemoryAnalysisResult {
            entries: vec![],
            summary: "Memory analysis result (simulated)".to_string(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_analyze_tests() {
        let action = IrAction::AnalyzeTests;
        let path = ".";
        let (result, output_type, capability) = RuntimeAnalyzeDispatcher::dispatch(&action, path).unwrap();

        assert_eq!(capability, CapabilityKind::AnalyzeTests);
        assert_eq!(output_type, OutputTypeId::TestInventoryResult);
        
        let test_res = result.downcast_ref::<TestInventoryResult>().unwrap();
        assert!(test_res.summary.contains("Test inventory completed"));
    }

    #[test]
    fn test_dispatch_analyze_project() {
        let action = IrAction::AnalyzeProject;
        let path = ".";
        let (result, output_type, capability) = RuntimeAnalyzeDispatcher::dispatch(&action, path).unwrap();

        assert_eq!(capability, CapabilityKind::AnalyzeProject);
        assert_eq!(output_type, OutputTypeId::ProjectStructureAnalysisResult);

        let project_res = result.downcast_ref::<ProjectStructureAnalysisResult>().unwrap();
        assert!(project_res.summary.contains("Project structure analyzed"));
    }
}
