use std::collections::BTreeMap;

use integration_layer::{
    CycleReport, Issue, LayerModel, LayerViolation, Pattern, RoleAssignment, SemanticLayer,
};
use serde::{Deserialize, Serialize};

use crate::coding::{CodeChangeSet, CodingExecutionResult};
use crate::runner::{CpuReleaseTelemetry, MemoryUsage, OutputMeta, SandboxMode};
use crate::source_index::{ApplyTargetResolution, QualifiedModuleId};

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeResultDTO {
    pub summary: String,
    pub issues: Vec<IssueDTO>,
    pub root_cause: Option<RootCause>,
    pub plan: Vec<RefactorStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueDTO {
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RootCause {
    pub label: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RefactorStep {
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisReport {
    pub root: String,
    pub total_files: usize,
    pub source_files: usize,
    pub avg_complexity: String,
    pub manifests: Vec<String>,
    pub languages: BTreeMap<String, usize>,
    pub top_level_entries: Vec<String>,
    pub architecture_hints: Vec<String>,
    pub modules: Vec<AnalysisModule>,
    pub graph_nodes: Vec<ModuleNode>,
    pub dependencies: Vec<AnalysisDependency>,
    pub todo_files: usize,
    pub cycles: CycleReport,
    pub layers: LayerModel,
    pub violations: Vec<LayerViolation>,
    pub roles: Vec<RoleAssignment>,
    pub semantic_layers: Vec<SemanticLayer>,
    pub data_flow: Vec<DataFlowEdgeReport>,
    pub issues: Vec<Issue>,
    pub code_issues: Vec<CodeIssue>,
    pub summary: AnalysisSummary,
    pub next_action: String,
    pub root_cause: Option<RootCause>,
    pub refactor_plan: Vec<RefactorStep>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AnalysisSummary {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisModule {
    pub name: String,
    pub file_count: usize,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModuleNode {
    pub qualified_id: QualifiedModuleId,
    pub logical_name: String,
    pub source_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisDependency {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignSnapshot {
    pub nodes: Vec<DesignNode>,
    pub edges: Vec<DesignEdge>,
    pub cycles: Vec<Vec<String>>,
    pub violations: Vec<DesignViolation>,
    #[serde(default)]
    pub analyze_legacy_binding_hits: u64,
    #[serde(default)]
    pub analyze_fallback_hits: u64,
    #[serde(default)]
    pub fixture_binding_detected: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignNode {
    pub id: String,
    pub logical_name: String,
    pub source_path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignEdge {
    pub id: String,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignViolation {
    pub violation_type: String,
    pub edge_id: Option<String>,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutationPlan {
    pub edge_id: String,
    pub operation: MutationOperation,
    pub strategy: MutationStrategy,
    pub constraints: MutationConstraints,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub snapshot_version: Option<String>,
    #[serde(default)]
    pub resolver_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperation {
    RemoveDependency,
    ExtractInterface,
    MoveDependency,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationStrategy {
    ExtractInterface,
    ImportRebinding,
    BoundaryMove,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutationConstraints {
    pub preserve_public_api: bool,
    pub no_new_cycles: bool,
    pub target_scope_locked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DesignReport {
    pub root: String,
    pub inferred_style: String,
    pub components: Vec<String>,
    pub design_units: Vec<String>,
    pub recommended_next_steps: Vec<String>,
    pub cycles: CycleReport,
    pub layers: LayerModel,
    pub violations: Vec<LayerViolation>,
    pub roles: Vec<RoleAssignment>,
    pub semantic_layers: Vec<SemanticLayer>,
    pub patterns: Vec<Pattern>,
    pub suggestions: Vec<integration_layer::RefactorSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub root: String,
    pub valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub cycles: CycleReport,
    pub layers: LayerModel,
    pub violations: Vec<LayerViolation>,
    pub patterns: Vec<Pattern>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefactorReport {
    pub root: String,
    pub plan: integration_layer::RefactorPlan,
    pub patches: Vec<integration_layer::CodePatch>,
    pub simulation: integration_layer::SimulationResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodingReport {
    pub root: String,
    pub dry_run: bool,
    pub execution: CodingExecutionResult,
    pub patches: Vec<integration_layer::CodePatch>,
    pub changes: CodeChangeSet,
    pub apply_resolutions: Vec<ApplyTargetResolution>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataFlowEdgeReport {
    pub from: String,
    pub to: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodeIssue {
    pub severity: String,
    pub category: String,
    pub file: String,
    pub line: usize,
    pub title: String,
    pub issue: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub root: String,
    pub status: String,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub command: String,
    pub args: Vec<String>,
    pub telemetry: RunTelemetry,
    pub sandbox: RunSandbox,
    pub output_meta: OutputMeta,
    pub stderr_meta: OutputMeta,
    pub sandbox_mode: SandboxMode,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTelemetry {
    pub duration_ms: u128,
    pub exit_code: i32,
    pub stdout_size: usize,
    pub stderr_size: usize,
    pub memory_usage_kb: MemoryUsage,
    pub cpu_release: CpuReleaseTelemetry,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSandbox {
    pub max_execution_time_ms: u64,
    pub allow_network: bool,
    pub allow_fs_write: bool,
    pub allowed_paths: Vec<String>,
    pub working_dir: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RulesReport {
    pub language: String,
    pub action: String,
    pub active: Vec<RuleReport>,
    pub candidate: Vec<RuleReport>,
    pub validated: Vec<ValidatedRuleReport>,
    pub deprecated: Vec<RuleReport>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleReport {
    pub id: String,
    pub priority: u32,
    pub confidence: f32,
    pub usage_count: u32,
    pub source: String,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidatedRuleReport {
    pub id: String,
    pub validation_score: f32,
    pub passed_checks: Vec<String>,
    pub source: String,
}
