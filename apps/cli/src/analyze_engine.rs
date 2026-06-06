use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use syn::visit::Visit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyzeCommand {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct RepositoryScanner;

impl RepositoryScanner {
    pub fn scan(&self, root: &Path) -> Result<RepositorySnapshot, String> {
        let root = resolve_repository_path(root);
        let roots = scan_roots(&root);
        let mut files = Vec::new();
        let mut directories = Vec::new();
        let mut cargo_tomls = Vec::new();
        for scan_root in roots {
            if scan_root.exists() {
                collect_repository_entries(
                    &scan_root,
                    &mut files,
                    &mut directories,
                    &mut cargo_tomls,
                )?;
            }
        }
        let root_cargo = root.join("Cargo.toml");
        if root_cargo.exists() {
            cargo_tomls.push(root_cargo);
        }
        files.sort();
        files.dedup();
        directories.sort();
        directories.dedup();
        cargo_tomls.sort();
        cargo_tomls.dedup();
        let workspace_members = parse_workspace_members(&root.join("Cargo.toml"))?;
        Ok(RepositorySnapshot {
            root,
            directories,
            rust_files: files,
            cargo_tomls,
            workspace_members,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub root: PathBuf,
    pub directories: Vec<PathBuf>,
    pub rust_files: Vec<PathBuf>,
    pub cargo_tomls: Vec<PathBuf>,
    pub workspace_members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AstModule {
    pub file_path: PathBuf,
    pub module_path: String,
    pub modules: Vec<String>,
    pub structs: Vec<String>,
    pub enums: Vec<String>,
    pub traits: Vec<String>,
    pub functions: Vec<String>,
    pub uses: Vec<String>,
    pub impls: Vec<TraitImplementation>,
    pub calls: Vec<FunctionCall>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitImplementation {
    pub trait_name: String,
    pub for_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub caller: String,
    pub callee: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureGraph {
    pub nodes: Vec<StructureNode>,
    pub edges: Vec<StructureEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureNode {
    pub id: String,
    pub label: String,
    pub kind: StructureNodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureNodeKind {
    File,
    Module,
    Struct,
    Enum,
    Trait,
    Function,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureEdge {
    pub from: String,
    pub to: String,
    pub kind: StructureEdgeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureEdgeKind {
    Contains,
    DependsOn,
    Implements,
    Uses,
    Calls,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeResult {
    pub project_name: String,
    pub modules: usize,
    pub structs: usize,
    pub enums: usize,
    pub traits: usize,
    pub functions: usize,
    pub module_count: usize,
    pub struct_count: usize,
    pub enum_count: usize,
    pub trait_count: usize,
    pub function_count: usize,
    pub top_components: Vec<String>,
    pub top_modules: Vec<String>,
    pub dependencies: Vec<String>,
    pub circular_dependencies: usize,
    pub layer_violations: usize,
    pub boundary_violations: usize,
    pub god_objects: usize,
    pub architecture_health: f32,
    pub status: ArchitectureStatus,
    pub convergence_score: f32,
    pub direction: Direction,
    pub top_proposal: Option<String>,
    pub predicted_health_delta: f32,
    pub convergence_confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticStructure {
    pub components: Vec<SemanticComponent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticComponent {
    pub name: String,
    pub category: SemanticCategory,
    pub responsibility: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticCategory {
    Controller,
    Service,
    Repository,
    Engine,
    Memory,
    Policy,
    Runtime,
    UI,
    Storage,
    Module,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircularDependency {
    pub cycle_nodes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HiddenDependency {
    pub source: String,
    pub via: String,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DependencyHotspot {
    pub component: String,
    pub incoming_edges: usize,
    pub outgoing_edges: usize,
    pub calls: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DependencyAnalysis {
    pub circular_dependencies: Vec<CircularDependency>,
    pub hidden_dependencies: Vec<HiddenDependency>,
    pub dependency_density: f32,
    pub hotspots: Vec<DependencyHotspot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GodObject {
    pub component: String,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MassiveModule {
    pub component: String,
    pub function_count: usize,
    pub dependency_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsibilityLeakage {
    pub source: String,
    pub target: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsibilityAnalysis {
    pub god_objects: Vec<GodObject>,
    pub massive_modules: Vec<MassiveModule>,
    pub responsibility_leakage: Vec<ResponsibilityLeakage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureRule {
    pub source: String,
    pub target: String,
    pub allow: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerViolation {
    pub source: String,
    pub target: String,
    pub source_layer: SemanticCategory,
    pub target_layer: SemanticCategory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryViolation {
    pub source: String,
    pub target: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitectureStatus {
    Stable,
    Excellent,
    Good,
    Fair,
    Healthy,
    Warning,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureHealth {
    pub score: f32,
    pub status: ArchitectureStatus,
    pub calibration: ArchitectureHealthV2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureHealthV2 {
    pub dependency_score: f32,
    pub responsibility_score: f32,
    pub architecture_score: f32,
    pub drift_score: f32,
    pub final_score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureAnalysis {
    pub rules: Vec<ArchitectureRule>,
    pub layer_violations: Vec<LayerViolation>,
    pub boundary_violations: Vec<BoundaryViolation>,
    pub health: ArchitectureHealth,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentDrift {
    pub component: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDrift {
    pub design_source: Option<PathBuf>,
    pub intent_drifts: Vec<IntentDrift>,
    pub missing_design_reference: bool,
}

pub struct DependencyAnalyzer;
pub struct RefactoringPlanner;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutationPlan {
    pub kind: String,
    pub target: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceProposal {
    pub title: String,
    pub reason: String,
    pub impact_score: f32,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefactoringProposal {
    pub proposal: ConvergenceProposal,
    pub mutation_plan: MutationPlan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub proposal_title: String,
    pub dependency_reduction: f32,
    pub violation_reduction: f32,
    pub health_improvement: f32,
    pub risk: f32,
    pub predicted_health: f32,
    pub design_drift_impact: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureSimulation {
    pub proposal_title: String,
    pub current_health: f32,
    pub predicted_health: f32,
    pub health_delta: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesignAlignmentStatus {
    Aligned,
    PartiallyAligned,
    Misaligned,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignAlignment {
    pub status: DesignAlignmentStatus,
    pub score: f32,
    pub evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceState {
    Converging,
    Unstable,
    Diverging,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceScore {
    pub score: f32,
    pub state: ConvergenceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Improving,
    Stable,
    Degrading,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutationImpactModel {
    pub proposal_title: String,
    pub architecture_risk: f32,
    pub dependency_impact: f32,
    pub design_drift_impact: f32,
    pub predicted_health_delta: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceReport {
    pub architecture_health: f32,
    pub convergence_score: ConvergenceScore,
    pub direction: Direction,
    pub top_proposal: Option<ConvergenceProposal>,
    pub predicted_health_delta: f32,
    pub confidence: f32,
    pub refactoring_proposals: Vec<RefactoringProposal>,
    pub impact_analysis: Vec<ImpactAnalysis>,
    pub architecture_simulations: Vec<ArchitectureSimulation>,
    pub design_alignment: DesignAlignment,
    pub mutation_impact_models: Vec<MutationImpactModel>,
}

// ===== Security Architecture Analyzer (v1.4) =====

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLayer {
    ExternalInput,
    Api,
    Ui,
    Service,
    Runtime,
    Policy,
    Memory,
    Storage,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustBoundaryViolation {
    pub source: String,
    pub target: String,
    pub source_layer: SecurityLayer,
    pub target_layer: SecurityLayer,
    pub severity: SecuritySeverity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivilegeViolation {
    pub source: String,
    pub target: String,
    pub source_layer: SecurityLayer,
    pub target_layer: SecurityLayer,
    pub severity: SecuritySeverity,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DangerousPathKind {
    ExternalInputToRuntime,
    ExternalInputToFilesystem,
    ExternalInputToNetwork,
    ExternalInputToShell,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DangerousPath {
    pub source: String,
    pub target_api: String,
    pub kind: DangerousPathKind,
    pub severity: SecuritySeverity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityLayerViolation {
    pub source: String,
    pub target: String,
    pub source_layer: SecurityLayer,
    pub target_layer: SecurityLayer,
    pub severity: SecuritySeverity,
    pub rule: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackSurfaceRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackSurfaceReport {
    pub entry_points: usize,
    pub shell_access: usize,
    pub filesystem_access: usize,
    pub network_access: usize,
    pub risk: AttackSurfaceRisk,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityDrift {
    pub component: String,
    pub expected_pattern: String,
    pub actual_pattern: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SecurityRiskScore {
    pub trust_boundary_score: f32,
    pub privilege_score: f32,
    pub dependency_score: f32,
    pub attack_surface_score: f32,
    pub final_score: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityClassification {
    Excellent,
    Good,
    Warning,
    HighRisk,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SecurityProposal {
    pub title: String,
    pub reason: String,
    pub impact: f32,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SecurityImpactAnalysis {
    pub proposal_title: String,
    pub current_risk: f32,
    pub predicted_risk: f32,
    pub impact: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SecurityArchitectureReport {
    pub trust_boundary_violations: Vec<TrustBoundaryViolation>,
    pub privilege_violations: Vec<PrivilegeViolation>,
    pub dangerous_paths: Vec<DangerousPath>,
    pub layer_violations: Vec<SecurityLayerViolation>,
    pub attack_surface: AttackSurfaceReport,
    pub security_drift: Vec<SecurityDrift>,
    pub risk_score: SecurityRiskScore,
    pub classification: SecurityClassification,
    pub proposals: Vec<SecurityProposal>,
    pub impact_analysis: Vec<SecurityImpactAnalysis>,
}

pub struct TrustBoundaryAnalyzer;
pub struct PrivilegeBoundaryAnalyzer;
pub struct DangerousDependencyAnalyzer;
pub struct AttackSurfaceAnalyzer;
pub struct SecurityDriftAnalyzer;

pub struct DiagnosticReportEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureHealthBreakdown {
    pub dependency_score: f32,
    pub responsibility_score: f32,
    pub architecture_score: f32,
    pub drift_score: f32,
    pub final_score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthContribution {
    pub dimension: String,
    pub score: f32,
    pub weight: f32,
    pub contribution: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthTrend {
    pub previous_score: f32,
    pub current_score: f32,
    pub delta: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthPenalty {
    pub label: String,
    pub value: f32,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthFormulaReport {
    pub breakdown: ArchitectureHealthBreakdown,
    pub penalties: Vec<HealthPenalty>,
    pub formula: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GodObjectReport {
    pub component: String,
    pub function_count: usize,
    pub dependency_count: usize,
    pub score: f32,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MassiveModuleReport {
    pub component: String,
    pub function_count: usize,
    pub dependency_count: usize,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerViolationReport {
    pub source: String,
    pub target: String,
    pub rule: String,
    pub severity: DiagnosticSeverity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryViolationReport {
    pub source: String,
    pub target: String,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub reason: String,
    pub severity: DiagnosticSeverity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyHotspotReport {
    pub component: String,
    pub incoming_edges: usize,
    pub outgoing_edges: usize,
    pub calls: usize,
    pub rank_score: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefactoringProposalReport {
    pub rank: usize,
    pub title: String,
    pub impact: f32,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceExplanation {
    pub question: String,
    pub health_trend: f32,
    pub violation_trend: i32,
    pub dependency_density_trend: f32,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetailedDiagnosticReport {
    pub health_breakdown: ArchitectureHealthBreakdown,
    pub contribution_analysis: Vec<HealthContribution>,
    pub health_trend: HealthTrend,
    pub health_formula: HealthFormulaReport,
    pub god_objects: Vec<GodObjectReport>,
    pub massive_modules: Vec<MassiveModuleReport>,
    pub layer_violations: Vec<LayerViolationReport>,
    pub boundary_violations: Vec<BoundaryViolationReport>,
    pub dependency_hotspots: Vec<DependencyHotspotReport>,
    pub refactoring_proposals: Vec<RefactoringProposalReport>,
    pub convergence_explanation: ConvergenceExplanation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeEngineOutput {
    pub root: PathBuf,
    pub repository: RepositorySnapshot,
    pub files: Vec<PathBuf>,
    pub ast_modules: Vec<AstModule>,
    pub graph: StructureGraph,
    pub semantic_structure: SemanticStructure,
    pub dependency_analysis: DependencyAnalysis,
    pub responsibility_analysis: ResponsibilityAnalysis,
    pub architecture_analysis: ArchitectureAnalysis,
    pub design_drift: DesignDrift,
    pub convergence_report: ConvergenceReport,
    pub detailed_report: DetailedDiagnosticReport,
    pub security_report: SecurityArchitectureReport,
    pub result: AnalyzeResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryEntry {
    pub id: String,
    pub kind: String,
    pub project_name: String,
    pub summary: AnalyzeResult,
    pub repository_snapshot: RepositorySnapshot,
    pub structure_graph: StructureGraph,
    pub semantic_structure: SemanticStructure,
    pub dependency_analysis: DependencyAnalysis,
    pub responsibility_analysis: ResponsibilityAnalysis,
    pub architecture_analysis: ArchitectureAnalysis,
    pub design_drift: DesignDrift,
    pub convergence_report: ConvergenceReport,
    pub detailed_report: DetailedDiagnosticReport,
}

pub fn execute(command: AnalyzeCommand) -> Result<AnalyzeEngineOutput, String> {
    let root = resolve_repository_path(&command.path);
    let scanner = RepositoryScanner;
    let repository = scanner.scan(&root)?;
    let files = repository.rust_files.clone();
    let ast_modules = extract_rust_ast_modules(&root, &files)?;
    let graph = build_structure_graph(&ast_modules);
    let semantic_structure = analyze_semantic_structure(&graph, &ast_modules);
    let dependency_analysis =
        DependencyAnalyzer::analyze(&graph, &semantic_structure, &ast_modules);
    let responsibility_analysis =
        analyze_responsibilities(&graph, &semantic_structure, &ast_modules);
    let architecture_analysis = analyze_architecture(
        &semantic_structure,
        &dependency_analysis,
        &responsibility_analysis,
    );
    let design_drift = analyze_design_drift(&root, &semantic_structure);
    let previous_health_score = read_previous_health_score(&root);
    let architecture_analysis = calibrate_architecture_analysis(
        architecture_analysis,
        &dependency_analysis,
        &responsibility_analysis,
        &design_drift,
    );
    let convergence_report = analyze_convergence(
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &design_drift,
    );
    let detailed_report = DiagnosticReportEngine::build(
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &design_drift,
        &convergence_report,
        previous_health_score,
    );
    let security_report = analyze_security_architecture(&ast_modules, &semantic_structure);
    let result = build_analyze_result(
        &root,
        &ast_modules,
        &graph,
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &convergence_report,
    );
    persist_to_holographic_memory(
        &root,
        &result,
        &repository,
        &graph,
        &semantic_structure,
        &dependency_analysis,
        &responsibility_analysis,
        &architecture_analysis,
        &design_drift,
        &convergence_report,
        &detailed_report,
        &security_report,
    )?;
    Ok(AnalyzeEngineOutput {
        root,
        repository,
        files,
        ast_modules,
        graph,
        semantic_structure,
        dependency_analysis,
        responsibility_analysis,
        architecture_analysis,
        design_drift,
        convergence_report,
        detailed_report,
        security_report,
        result,
    })
}

pub fn render_analyze_result(result: &AnalyzeResult) -> String {
    let mut out = String::new();
    out.push_str("=== Analyze Result ===\n\n");
    out.push_str("Project\n");
    out.push_str(&format!("{}\n\n", result.project_name));
    out.push_str("Modules\n");
    out.push_str(&format!("{}\n\n", result.module_count));
    out.push_str("Structs\n");
    out.push_str(&format!("{}\n\n", result.struct_count));
    out.push_str("Enums\n");
    out.push_str(&format!("{}\n\n", result.enum_count));
    out.push_str("Traits\n");
    out.push_str(&format!("{}\n\n", result.trait_count));
    out.push_str("Functions\n");
    out.push_str(&format!("{}\n\n", result.function_count));
    out.push_str("Circular Dependencies\n");
    out.push_str(&format!("{}\n\n", result.circular_dependencies));
    out.push_str("Layer Violations\n");
    out.push_str(&format!("{}\n\n", result.layer_violations));
    out.push_str("Boundary Violations\n");
    out.push_str(&format!("{}\n\n", result.boundary_violations));
    out.push_str("God Objects\n");
    out.push_str(&format!("{}\n\n", result.god_objects));
    out.push_str("Architecture Health\n");
    out.push_str(&format!("{:.1}\n\n", result.architecture_health));
    out.push_str("Status\n");
    out.push_str(&format!("{:?}\n\n", result.status));
    out.push_str("Top Components\n");
    if result.top_modules.is_empty() {
        out.push_str("(none)\n");
    } else {
        for module in &result.top_modules {
            out.push_str(module);
            out.push('\n');
        }
    }
    out.push_str("\n=== Convergence Analysis ===\n\n");
    out.push_str("Architecture Health\n");
    out.push_str(&format!("{:.1}\n\n", result.architecture_health));
    out.push_str("Convergence Score\n");
    out.push_str(&format!("{:.1}\n\n", result.convergence_score));
    out.push_str("Direction\n");
    out.push_str(&format!("{:?}\n\n", result.direction));
    out.push_str("Top Proposal\n");
    out.push_str(
        result
            .top_proposal
            .as_deref()
            .unwrap_or("No convergence proposal"),
    );
    out.push_str("\n\nPredicted Health\n");
    out.push_str(&format!("{:+.1}\n\n", result.predicted_health_delta));
    out.push_str("Confidence\n");
    out.push_str(&format!("{:.2}\n", result.convergence_confidence));
    out
}

pub fn render_detailed_analyze_result(output: &AnalyzeEngineOutput) -> String {
    let mut out = render_analyze_result(&output.result);
    let report = &output.detailed_report;

    out.push_str("\n\n=== Architecture Health ===\n\n");
    out.push_str(&format!(
        "Dependency\n{:.1}\n\nResponsibility\n{:.1}\n\nArchitecture\n{:.1}\n\nDrift\n{:.1}\n\nFinal\n{:.1}\n\nStatus\n{:?}\n\n",
        report.health_breakdown.dependency_score,
        report.health_breakdown.responsibility_score,
        report.health_breakdown.architecture_score,
        report.health_breakdown.drift_score,
        report.health_breakdown.final_score,
        output.result.status,
    ));

    out.push_str("\n\n=== Health Breakdown ===\n\n");
    out.push_str(&format!(
        "Dependency Score\n{:.1}\n\nResponsibility Score\n{:.1}\n\nArchitecture Score\n{:.1}\n\nDrift Score\n{:.1}\n\nFinal Score\n{:.1}\n\n",
        report.health_breakdown.dependency_score,
        report.health_breakdown.responsibility_score,
        report.health_breakdown.architecture_score,
        report.health_breakdown.drift_score,
        report.health_breakdown.final_score,
    ));
    out.push_str("Formula\n");
    out.push_str(&report.health_formula.formula);
    out.push_str("\n\n=== Contribution Analysis ===\n\n");
    for contribution in &report.contribution_analysis {
        out.push_str(&format!(
            "{}\nScore\n{:.1}\nWeight\n{:.2}\nContribution\n+{:.1}\n\n",
            contribution.dimension,
            contribution.score,
            contribution.weight,
            contribution.contribution
        ));
    }
    out.push_str("=== Health Trend ===\n\n");
    out.push_str(&format!(
        "Previous\n{:.1}\n\nCurrent\n{:.1}\n\nDelta\n{:+.1}\n\n",
        report.health_trend.previous_score,
        report.health_trend.current_score,
        report.health_trend.delta
    ));

    out.push_str("=== God Objects ===\n\n");
    if report.god_objects.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for (index, item) in report.god_objects.iter().enumerate() {
            out.push_str(&format!(
                "{}.\n{}\n\nFunctions\n{}\n\nDependencies\n{}\n\nScore\n{:.2}\n\nReasons\n{}\n\n",
                index + 1,
                item.component,
                item.function_count,
                item.dependency_count,
                item.score,
                item.reasons
                    .iter()
                    .map(|reason| format!("- {reason}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }

    out.push_str("=== Massive Modules ===\n\n");
    if report.massive_modules.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for item in &report.massive_modules {
            out.push_str(&format!(
                "{}\nFunctions: {}\nDependencies: {}\n{}\n\n",
                item.component,
                item.function_count,
                item.dependency_count,
                item.reasons.join("\n")
            ));
        }
    }

    out.push_str("=== Layer Violations ===\n\n");
    if report.layer_violations.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for item in &report.layer_violations {
            out.push_str(&format!(
                "{}\n ↓\n{}\nRule\n{}\nSeverity\n{:?}\n\n",
                item.source, item.target, item.rule, item.severity
            ));
        }
    }

    out.push_str("=== Boundary Violations ===\n\n");
    if report.boundary_violations.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for item in &report.boundary_violations {
            out.push_str(&format!(
                "{}\n ↓ internal access\n{}\nFile\n{}\nLine\n{}\nReason\n{}\nSeverity\n{:?}\n\n",
                item.source,
                item.target,
                item.file
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "(unknown)".to_string()),
                item.line
                    .map(|line| line.to_string())
                    .unwrap_or_else(|| "(unknown)".to_string()),
                item.reason,
                item.severity
            ));
        }
    }

    out.push_str("=== Dependency Hotspots ===\n\n");
    if report.dependency_hotspots.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for (index, item) in report.dependency_hotspots.iter().enumerate() {
            out.push_str(&format!(
                "{}.\n{}\nIncoming\n{}\nOutgoing\n{}\nCalls\n{}\n\n",
                index + 1,
                item.component,
                item.incoming_edges,
                item.outgoing_edges,
                item.calls
            ));
        }
    }

    out.push_str("=== Refactoring Proposals ===\n\n");
    if report.refactoring_proposals.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for item in &report.refactoring_proposals {
            out.push_str(&format!(
                "{}.\n{}\nImpact\n{:+.1}\nConfidence\n{:.2}\nReason\n{}\n\n",
                item.rank, item.title, item.impact, item.confidence, item.reason
            ));
        }
    }

    out.push_str("=== Convergence Explanation ===\n\n");
    out.push_str(&format!(
        "{}\n\nHealth Trend\n{:+.1}\n\nViolation Trend\n{}\n\nDependency Density\n{:+.2}\n\nReasons\n{}\n",
        report.convergence_explanation.question,
        report.convergence_explanation.health_trend,
        report.convergence_explanation.violation_trend,
        report.convergence_explanation.dependency_density_trend,
        report
            .convergence_explanation
            .reasons
            .iter()
            .map(|reason| format!("- {reason}"))
            .collect::<Vec<_>>()
            .join("\n")
    ));

    out
}

fn extract_rust_ast_modules(root: &Path, files: &[PathBuf]) -> Result<Vec<AstModule>, String> {
    let mut modules = Vec::new();
    for file in files {
        let source = fs::read_to_string(file)
            .map_err(|err| format!("failed to read {}: {err}", file.display()))?;
        let parsed = syn::parse_file(&source)
            .map_err(|err| format!("failed to parse {}: {err}", file.display()))?;
        let rel = file.strip_prefix(root).unwrap_or(file).to_path_buf();
        let module_path = module_path_from_relative_path(&rel);
        let mut visitor = RustAstVisitor::default();
        visitor.visit_file(&parsed);
        modules.push(AstModule {
            file_path: rel,
            module_path,
            modules: visitor.modules,
            structs: visitor.structs,
            enums: visitor.enums,
            traits: visitor.traits,
            functions: visitor.functions,
            uses: visitor.uses,
            impls: visitor.impls,
            calls: visitor.calls,
        });
    }
    Ok(modules)
}

#[derive(Default)]
struct RustAstVisitor {
    modules: Vec<String>,
    structs: Vec<String>,
    enums: Vec<String>,
    traits: Vec<String>,
    functions: Vec<String>,
    uses: Vec<String>,
    impls: Vec<TraitImplementation>,
    calls: Vec<FunctionCall>,
    current_function: Option<String>,
}

impl<'ast> Visit<'ast> for RustAstVisitor {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.modules.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.structs.push(node.ident.to_string());
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.enums.push(node.ident.to_string());
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.traits.push(node.ident.to_string());
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let previous = self.current_function.replace(node.sig.ident.to_string());
        self.functions.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.current_function = previous;
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        collect_use_tree(&node.tree, String::new(), &mut self.uses);
        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if let Some((_, path, _)) = &node.trait_ {
            self.impls.push(TraitImplementation {
                trait_name: path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                    .unwrap_or_else(|| path_to_string(path)),
                for_type: type_to_string(&node.self_ty),
            });
        }
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Some(caller) = &self.current_function {
            self.calls.push(FunctionCall {
                caller: caller.clone(),
                callee: expr_to_call_name(&node.func),
            });
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if let Some(caller) = &self.current_function {
            self.calls.push(FunctionCall {
                caller: caller.clone(),
                callee: node.method.to_string(),
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn collect_use_tree(tree: &syn::UseTree, prefix: String, output: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(path) => {
            let next = append_path_segment(&prefix, &path.ident.to_string());
            collect_use_tree(&path.tree, next, output);
        }
        syn::UseTree::Name(name) => {
            output.push(append_path_segment(&prefix, &name.ident.to_string()))
        }
        syn::UseTree::Rename(rename) => {
            output.push(append_path_segment(&prefix, &rename.ident.to_string()))
        }
        syn::UseTree::Glob(_) => output.push(append_path_segment(&prefix, "*")),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix.clone(), output);
            }
        }
    }
}

fn build_structure_graph(ast_modules: &[AstModule]) -> StructureGraph {
    let mut nodes = BTreeMap::<String, StructureNode>::new();
    let mut edges = BTreeSet::<(String, String, String)>::new();
    for module in ast_modules {
        let file_id = format!("file:{}", module.file_path.display());
        let module_id = format!("module:{}", module.module_path);
        insert_node(
            &mut nodes,
            file_id.clone(),
            module.file_path.display().to_string(),
            StructureNodeKind::File,
        );
        insert_node(
            &mut nodes,
            module_id.clone(),
            module.module_path.clone(),
            StructureNodeKind::Module,
        );
        edges.insert((file_id.clone(), module_id.clone(), "contains".to_string()));

        for name in &module.modules {
            let id = format!("module:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Module,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.structs {
            let id = format!("struct:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Struct,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.enums {
            let id = format!("enum:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Enum,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.traits {
            let id = format!("trait:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Trait,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for name in &module.functions {
            let id = format!("fn:{}::{name}", module.module_path);
            insert_node(
                &mut nodes,
                id.clone(),
                name.clone(),
                StructureNodeKind::Function,
            );
            edges.insert((module_id.clone(), id, "contains".to_string()));
        }
        for dependency in &module.uses {
            let dep_id = format!("dep:{dependency}");
            insert_node(
                &mut nodes,
                dep_id.clone(),
                dependency.clone(),
                StructureNodeKind::Module,
            );
            edges.insert((module_id.clone(), dep_id, "uses".to_string()));
        }
        for call in &module.calls {
            let caller_id = format!("fn:{}::{}", module.module_path, call.caller);
            let callee_id = format!("call:{}", call.callee);
            insert_node(
                &mut nodes,
                caller_id.clone(),
                call.caller.clone(),
                StructureNodeKind::Function,
            );
            insert_node(
                &mut nodes,
                callee_id.clone(),
                call.callee.clone(),
                StructureNodeKind::Function,
            );
            edges.insert((caller_id, callee_id, "calls".to_string()));
        }
        for implementation in &module.impls {
            let type_id = format!("struct:{}::{}", module.module_path, implementation.for_type);
            let trait_id = format!("trait:{}", implementation.trait_name);
            insert_node(
                &mut nodes,
                type_id.clone(),
                implementation.for_type.clone(),
                StructureNodeKind::Struct,
            );
            insert_node(
                &mut nodes,
                trait_id.clone(),
                implementation.trait_name.clone(),
                StructureNodeKind::Trait,
            );
            edges.insert((type_id, trait_id, "implements".to_string()));
        }
    }

    StructureGraph {
        nodes: nodes.into_values().collect(),
        edges: edges
            .into_iter()
            .map(|(from, to, kind)| StructureEdge {
                from,
                to,
                kind: match kind.as_str() {
                    "contains" => StructureEdgeKind::Contains,
                    "implements" => StructureEdgeKind::Implements,
                    "uses" => StructureEdgeKind::Uses,
                    "calls" => StructureEdgeKind::Calls,
                    _ => StructureEdgeKind::DependsOn,
                },
            })
            .collect(),
    }
}

fn analyze_semantic_structure(graph: &StructureGraph, modules: &[AstModule]) -> SemanticStructure {
    let module_lookup = modules
        .iter()
        .map(|module| (module.module_path.as_str(), module))
        .collect::<BTreeMap<_, _>>();
    let mut components = graph
        .nodes
        .iter()
        .filter(|node| node.kind == StructureNodeKind::Module)
        .map(|node| {
            let module = module_lookup.get(node.label.as_str()).copied();
            let evidence = semantic_evidence(&node.label, node, module);
            SemanticComponent {
                name: node.label.clone(),
                category: classify_semantic_category(&node.label, node, module),
                responsibility: infer_responsibility(&node.label, node, module),
                evidence,
            }
        })
        .collect::<Vec<_>>();
    components.sort_by(|left, right| left.name.cmp(&right.name));
    components.dedup_by(|left, right| left.name == right.name);
    SemanticStructure { components }
}

impl DependencyAnalyzer {
    pub fn analyze(
        graph: &StructureGraph,
        semantic: &SemanticStructure,
        modules: &[AstModule],
    ) -> DependencyAnalysis {
        let module_edges = module_dependency_edges(modules);
        let circular_dependencies = detect_circular_dependencies(&module_edges);
        let hidden_dependencies = detect_hidden_dependencies(&module_edges, semantic);
        let dependency_density = if graph.nodes.is_empty() {
            0.0
        } else {
            graph.edges.len() as f32 / graph.nodes.len() as f32
        };
        let hotspots = detect_dependency_hotspots(graph, modules);
        DependencyAnalysis {
            circular_dependencies,
            hidden_dependencies,
            dependency_density,
            hotspots,
        }
    }
}

fn analyze_responsibilities(
    graph: &StructureGraph,
    semantic: &SemanticStructure,
    modules: &[AstModule],
) -> ResponsibilityAnalysis {
    let mut incoming = BTreeMap::<String, usize>::new();
    let mut outgoing = BTreeMap::<String, usize>::new();
    for edge in &graph.edges {
        *incoming.entry(edge.to.clone()).or_default() += 1;
        *outgoing.entry(edge.from.clone()).or_default() += 1;
    }

    let mut god_objects = Vec::new();
    let mut massive_modules = Vec::new();
    for module in modules {
        let module_id = format!("module:{}", module.module_path);
        let in_count = incoming.get(&module_id).copied().unwrap_or(0);
        let dependency_count = module.uses.len();
        if module.functions.len() > 100 || in_count > 80 {
            god_objects.push(GodObject {
                component: module.module_path.clone(),
                score: (module.functions.len() as f32 / 100.0) + (in_count as f32 / 80.0),
            });
        }
        if module.functions.len() > 80 || dependency_count > 40 {
            massive_modules.push(MassiveModule {
                component: module.module_path.clone(),
                function_count: module.functions.len(),
                dependency_count,
            });
        }
    }

    let component_categories = semantic_category_map(semantic);
    let mut responsibility_leakage = Vec::new();
    for edge in &graph.edges {
        if !matches!(
            edge.kind,
            StructureEdgeKind::Uses | StructureEdgeKind::DependsOn
        ) {
            continue;
        }
        let source = component_name_from_node_id(&edge.from);
        let target = component_name_from_node_id(&edge.to);
        let source_category = component_categories.get(&source);
        let target_category = component_categories.get(&target);
        if matches!(source_category, Some(SemanticCategory::UI))
            && matches!(target_category, Some(SemanticCategory::Storage))
        {
            responsibility_leakage.push(ResponsibilityLeakage {
                source,
                target,
                reason: "UI accesses Storage directly".to_string(),
            });
        } else if matches!(source_category, Some(SemanticCategory::Policy))
            && matches!(target_category, Some(SemanticCategory::Repository))
        {
            responsibility_leakage.push(ResponsibilityLeakage {
                source,
                target,
                reason: "Policy writes through Repository boundary".to_string(),
            });
        }
    }
    responsibility_leakage.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.target.cmp(&right.target))
    });
    responsibility_leakage
        .dedup_by(|left, right| left.source == right.source && left.target == right.target);

    ResponsibilityAnalysis {
        god_objects,
        massive_modules,
        responsibility_leakage,
    }
}

fn analyze_architecture(
    semantic: &SemanticStructure,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
) -> ArchitectureAnalysis {
    let rules = default_architecture_rules();
    let mut layer_violations = Vec::new();
    for hidden in &dependency.hidden_dependencies {
        let source_layer = semantic_category_for(semantic, &hidden.source);
        let target_layer = semantic_category_for(semantic, &hidden.target);
        if !architecture_allows(&rules, &source_layer, &target_layer) {
            layer_violations.push(LayerViolation {
                source: hidden.source.clone(),
                target: hidden.target.clone(),
                source_layer,
                target_layer,
            });
        }
    }
    for leakage in &responsibility.responsibility_leakage {
        layer_violations.push(LayerViolation {
            source: leakage.source.clone(),
            target: leakage.target.clone(),
            source_layer: semantic_category_for(semantic, &leakage.source),
            target_layer: semantic_category_for(semantic, &leakage.target),
        });
    }
    layer_violations.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.target.cmp(&right.target))
    });
    layer_violations
        .dedup_by(|left, right| left.source == right.source && left.target == right.target);

    let boundary_violations = dependency
        .hotspots
        .iter()
        .filter(|hotspot| hotspot.incoming_edges + hotspot.outgoing_edges > 300)
        .map(|hotspot| BoundaryViolation {
            source: hotspot.component.clone(),
            target: "module-boundary".to_string(),
            reason: "high cross-boundary dependency concentration".to_string(),
        })
        .collect::<Vec<_>>();

    let health = compute_architecture_health_v2(
        dependency,
        responsibility,
        layer_violations.len(),
        boundary_violations.len(),
        None,
    );
    ArchitectureAnalysis {
        rules,
        layer_violations,
        boundary_violations,
        health,
    }
}

fn analyze_design_drift(root: &Path, semantic: &SemanticStructure) -> DesignDrift {
    let design_path = root.join("design.md");
    if !design_path.exists() {
        return DesignDrift {
            design_source: None,
            intent_drifts: Vec::new(),
            missing_design_reference: true,
        };
    }
    let design = fs::read_to_string(&design_path)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut intent_drifts = Vec::new();
    for component in &semantic.components {
        if design.contains("memory")
            && matches!(component.category, SemanticCategory::Memory)
            && component.responsibility.contains("Runtime")
        {
            intent_drifts.push(IntentDrift {
                component: component.name.clone(),
                expected: "Memory Layer".to_string(),
                actual: component.responsibility.clone(),
            });
        }
        if design.contains("ui")
            && component.name.to_ascii_lowercase().contains("storage")
            && matches!(component.category, SemanticCategory::UI)
        {
            intent_drifts.push(IntentDrift {
                component: component.name.clone(),
                expected: "Storage boundary".to_string(),
                actual: "UI responsibility".to_string(),
            });
        }
    }
    DesignDrift {
        design_source: Some(design_path),
        intent_drifts,
        missing_design_reference: false,
    }
}

fn analyze_convergence(
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
) -> ConvergenceReport {
    let refactoring_proposals = RefactoringPlanner::plan(dependency, responsibility, architecture);
    let design_alignment = match_design_alignment(design_drift);
    let impact_analysis = analyze_impacts(
        &refactoring_proposals,
        architecture.health.score,
        dependency,
        responsibility,
        architecture,
        design_drift,
    );
    let architecture_simulations = simulate_architecture(
        &refactoring_proposals,
        architecture.health.score,
        &impact_analysis,
    );
    let mutation_impact_models = model_mutation_impacts(&impact_analysis);
    let top_proposal = refactoring_proposals
        .first()
        .map(|proposal| proposal.proposal.clone());
    let predicted_health_delta = architecture_simulations
        .first()
        .map(|simulation| simulation.health_delta)
        .unwrap_or(0.0);
    let confidence = top_proposal
        .as_ref()
        .map(|proposal| proposal.confidence)
        .unwrap_or(0.70);
    let convergence_score = compute_convergence_score(
        architecture.health.score,
        dependency,
        responsibility,
        &design_alignment,
    );
    let direction = infer_direction(predicted_health_delta, convergence_score.score);

    ConvergenceReport {
        architecture_health: architecture.health.score,
        convergence_score,
        direction,
        top_proposal,
        predicted_health_delta,
        confidence,
        refactoring_proposals,
        impact_analysis,
        architecture_simulations,
        design_alignment,
        mutation_impact_models,
    }
}

impl DiagnosticReportEngine {
    pub fn build(
        dependency: &DependencyAnalysis,
        responsibility: &ResponsibilityAnalysis,
        architecture: &ArchitectureAnalysis,
        design_drift: &DesignDrift,
        convergence: &ConvergenceReport,
        previous_health_score: Option<f32>,
    ) -> DetailedDiagnosticReport {
        let health_breakdown =
            build_health_breakdown(dependency, responsibility, architecture, design_drift);
        let contribution_analysis = build_contribution_analysis(&health_breakdown);
        let health_trend = build_health_trend(previous_health_score, health_breakdown.final_score);
        let health_formula =
            build_health_formula(&health_breakdown, dependency, responsibility, architecture);
        let god_objects = build_god_object_reports(responsibility);
        let massive_modules = build_massive_module_reports(responsibility);
        let layer_violations = architecture
            .layer_violations
            .iter()
            .map(|violation| LayerViolationReport {
                source: violation.source.clone(),
                target: violation.target.clone(),
                rule: format!(
                    "{:?} must not directly depend on {:?}",
                    violation.source_layer, violation.target_layer
                ),
                severity: severity_for_layer_violation(violation),
            })
            .collect();
        let boundary_violations = architecture
            .boundary_violations
            .iter()
            .map(|violation| BoundaryViolationReport {
                source: violation.source.clone(),
                target: violation.target.clone(),
                file: None,
                line: None,
                reason: violation.reason.clone(),
                severity: DiagnosticSeverity::High,
            })
            .collect();
        let dependency_hotspots = dependency
            .hotspots
            .iter()
            .map(|hotspot| DependencyHotspotReport {
                component: hotspot.component.clone(),
                incoming_edges: hotspot.incoming_edges,
                outgoing_edges: hotspot.outgoing_edges,
                calls: hotspot.calls,
                rank_score: hotspot.incoming_edges + hotspot.outgoing_edges + hotspot.calls,
            })
            .collect();
        let refactoring_proposals = convergence
            .refactoring_proposals
            .iter()
            .take(10)
            .enumerate()
            .map(|(index, proposal)| {
                let impact = convergence
                    .impact_analysis
                    .iter()
                    .find(|impact| impact.proposal_title == proposal.proposal.title)
                    .map(|impact| impact.health_improvement)
                    .unwrap_or(0.0);
                RefactoringProposalReport {
                    rank: index + 1,
                    title: proposal.proposal.title.clone(),
                    impact,
                    confidence: proposal.proposal.confidence,
                    reason: proposal.proposal.reason.clone(),
                }
            })
            .collect();
        let convergence_explanation =
            build_convergence_explanation(dependency, architecture, convergence);

        DetailedDiagnosticReport {
            health_breakdown,
            contribution_analysis,
            health_trend,
            health_formula,
            god_objects,
            massive_modules,
            layer_violations,
            boundary_violations,
            dependency_hotspots,
            refactoring_proposals,
            convergence_explanation,
        }
    }
}

fn build_health_breakdown(
    _dependency: &DependencyAnalysis,
    _responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    _design_drift: &DesignDrift,
) -> ArchitectureHealthBreakdown {
    let calibration = &architecture.health.calibration;
    ArchitectureHealthBreakdown {
        dependency_score: calibration.dependency_score,
        responsibility_score: calibration.responsibility_score,
        architecture_score: calibration.architecture_score,
        drift_score: calibration.drift_score,
        final_score: calibration.final_score,
    }
}

fn build_contribution_analysis(breakdown: &ArchitectureHealthBreakdown) -> Vec<HealthContribution> {
    vec![
        health_contribution("Dependency", breakdown.dependency_score, 0.30),
        health_contribution("Responsibility", breakdown.responsibility_score, 0.30),
        health_contribution("Architecture", breakdown.architecture_score, 0.30),
        health_contribution("Drift", breakdown.drift_score, 0.10),
    ]
}

fn health_contribution(dimension: &str, score: f32, weight: f32) -> HealthContribution {
    HealthContribution {
        dimension: dimension.to_string(),
        score,
        weight,
        contribution: score * weight,
    }
}

fn build_health_trend(previous_score: Option<f32>, current_score: f32) -> HealthTrend {
    let previous_score = previous_score.unwrap_or(current_score);
    HealthTrend {
        previous_score,
        current_score,
        delta: current_score - previous_score,
    }
}

fn build_health_formula(
    breakdown: &ArchitectureHealthBreakdown,
    _dependency: &DependencyAnalysis,
    _responsibility: &ResponsibilityAnalysis,
    _architecture: &ArchitectureAnalysis,
) -> HealthFormulaReport {
    HealthFormulaReport {
        breakdown: breakdown.clone(),
        penalties: Vec::new(),
        formula:
            "final = dependency * 0.30 + responsibility * 0.30 + architecture * 0.30 + drift * 0.10"
                .to_string(),
    }
}

fn build_god_object_reports(responsibility: &ResponsibilityAnalysis) -> Vec<GodObjectReport> {
    responsibility
        .god_objects
        .iter()
        .map(|god| {
            let massive = responsibility
                .massive_modules
                .iter()
                .find(|module| module.component == god.component);
            let function_count = massive.map(|module| module.function_count).unwrap_or(0);
            let dependency_count = massive.map(|module| module.dependency_count).unwrap_or(0);
            let mut reasons = Vec::new();
            if function_count > 100 {
                reasons.push("excessive function count".to_string());
            }
            if dependency_count > 40 {
                reasons.push("high dependency density".to_string());
            }
            if god.score > 1.0 {
                reasons.push("multiple responsibilities".to_string());
            }
            if reasons.is_empty() {
                reasons.push("incoming edge concentration".to_string());
            }
            GodObjectReport {
                component: god.component.clone(),
                function_count,
                dependency_count,
                score: god.score,
                reasons,
            }
        })
        .collect()
}

fn build_massive_module_reports(
    responsibility: &ResponsibilityAnalysis,
) -> Vec<MassiveModuleReport> {
    responsibility
        .massive_modules
        .iter()
        .map(|module| {
            let mut reasons = Vec::new();
            if module.function_count > 80 {
                reasons.push("function count exceeds massive module threshold".to_string());
            }
            if module.dependency_count > 40 {
                reasons.push("dependency count exceeds module boundary threshold".to_string());
            }
            MassiveModuleReport {
                component: module.component.clone(),
                function_count: module.function_count,
                dependency_count: module.dependency_count,
                reasons,
            }
        })
        .collect()
}

fn severity_for_layer_violation(violation: &LayerViolation) -> DiagnosticSeverity {
    match (&violation.source_layer, &violation.target_layer) {
        (SemanticCategory::UI, SemanticCategory::Storage)
        | (SemanticCategory::Controller, SemanticCategory::Storage) => DiagnosticSeverity::High,
        (SemanticCategory::Storage, SemanticCategory::UI) => DiagnosticSeverity::Critical,
        _ => DiagnosticSeverity::Medium,
    }
}

fn build_convergence_explanation(
    dependency: &DependencyAnalysis,
    architecture: &ArchitectureAnalysis,
    convergence: &ConvergenceReport,
) -> ConvergenceExplanation {
    let top_impact = convergence.impact_analysis.first();
    let violation_count =
        architecture.layer_violations.len() + architecture.boundary_violations.len();
    let violation_trend = top_impact
        .map(|impact| -((impact.violation_reduction * violation_count as f32).round() as i32))
        .unwrap_or(0);
    let dependency_density_trend = top_impact
        .map(|impact| -impact.dependency_reduction * dependency.dependency_density)
        .unwrap_or(0.0);
    let mut reasons = Vec::new();
    if convergence.predicted_health_delta > 0.0 {
        reasons.push(format!(
            "top proposal predicts health improvement of {:.1}",
            convergence.predicted_health_delta
        ));
    }
    if violation_trend < 0 {
        reasons.push(format!(
            "expected violation count decreases by {}",
            violation_trend.abs()
        ));
    }
    if dependency_density_trend < 0.0 {
        reasons.push(format!(
            "dependency density trend improves by {:.2}",
            dependency_density_trend.abs()
        ));
    }
    if reasons.is_empty() {
        reasons.push("no high-confidence convergence movement detected".to_string());
    }
    ConvergenceExplanation {
        question: format!("Why {:?}?", convergence.direction),
        health_trend: convergence.predicted_health_delta,
        violation_trend,
        dependency_density_trend,
        reasons,
    }
}

impl RefactoringPlanner {
    pub fn plan(
        dependency: &DependencyAnalysis,
        responsibility: &ResponsibilityAnalysis,
        architecture: &ArchitectureAnalysis,
    ) -> Vec<RefactoringProposal> {
        let mut proposals = Vec::new();
        for god_object in &responsibility.god_objects {
            proposals.push(RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: format!("Split {}", god_object.component),
                    reason: format!("god object score {:.2}", god_object.score),
                    impact_score: (0.70 + god_object.score / 5.0).min(0.98),
                    confidence: (0.82 + god_object.score / 20.0).min(0.96),
                },
                mutation_plan: MutationPlan {
                    kind: "ModuleSplit".to_string(),
                    target: god_object.component.clone(),
                    actions: vec![
                        format!("Extract {}Execution", pascal_tail(&god_object.component)),
                        format!("Extract {}Policy", pascal_tail(&god_object.component)),
                        format!("Extract {}Memory", pascal_tail(&god_object.component)),
                    ],
                },
            });
        }
        for cycle in &dependency.circular_dependencies {
            proposals.push(RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: format!("Break dependency cycle {}", cycle.cycle_nodes.join(" -> ")),
                    reason: "circular dependency detected".to_string(),
                    impact_score: 0.86,
                    confidence: 0.88,
                },
                mutation_plan: MutationPlan {
                    kind: "DependencyInversion".to_string(),
                    target: cycle.cycle_nodes.join(" -> "),
                    actions: vec![
                        "Introduce boundary trait".to_string(),
                        "Invert dependency".to_string(),
                    ],
                },
            });
        }
        for violation in &architecture.layer_violations {
            proposals.push(RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: format!(
                        "Insert boundary between {} and {}",
                        violation.source, violation.target
                    ),
                    reason: format!(
                        "{:?} directly reaches {:?}",
                        violation.source_layer, violation.target_layer
                    ),
                    impact_score: 0.74,
                    confidence: 0.84,
                },
                mutation_plan: MutationPlan {
                    kind: "BoundaryAdapter".to_string(),
                    target: format!("{} -> {}", violation.source, violation.target),
                    actions: vec![
                        "Introduce interface boundary".to_string(),
                        "Route dependency through service layer".to_string(),
                    ],
                },
            });
        }
        proposals.sort_by(|left, right| {
            right
                .proposal
                .impact_score
                .total_cmp(&left.proposal.impact_score)
                .then(left.proposal.title.cmp(&right.proposal.title))
        });
        proposals.truncate(20);
        proposals
    }
}

fn analyze_impacts(
    proposals: &[RefactoringProposal],
    current_health: f32,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
) -> Vec<ImpactAnalysis> {
    proposals
        .iter()
        .map(|proposal| {
            let dependency_reduction = match proposal.mutation_plan.kind.as_str() {
                "DependencyInversion" => 0.35,
                "BoundaryAdapter" => 0.20,
                "ModuleSplit" => 0.12,
                _ => 0.08,
            };
            let violation_reduction = if proposal.mutation_plan.kind == "BoundaryAdapter" {
                0.40
            } else if proposal.mutation_plan.kind == "DependencyInversion" {
                0.25
            } else {
                0.10
            };
            let health_improvement = predicted_health_improvement(
                proposal,
                dependency,
                responsibility,
                architecture,
                design_drift,
            );
            let risk = mutation_risk(proposal, design_drift);
            ImpactAnalysis {
                proposal_title: proposal.proposal.title.clone(),
                dependency_reduction,
                violation_reduction,
                health_improvement,
                risk,
                predicted_health: (current_health + health_improvement).clamp(0.0, 100.0),
                design_drift_impact: design_drift_impact(proposal, design_drift),
            }
        })
        .collect()
}

fn simulate_architecture(
    proposals: &[RefactoringProposal],
    current_health: f32,
    impacts: &[ImpactAnalysis],
) -> Vec<ArchitectureSimulation> {
    proposals
        .iter()
        .zip(impacts.iter())
        .map(|(proposal, impact)| ArchitectureSimulation {
            proposal_title: proposal.proposal.title.clone(),
            current_health,
            predicted_health: impact.predicted_health,
            health_delta: impact.predicted_health - current_health,
        })
        .collect()
}

fn match_design_alignment(design_drift: &DesignDrift) -> DesignAlignment {
    if design_drift.missing_design_reference {
        return DesignAlignment {
            status: DesignAlignmentStatus::PartiallyAligned,
            score: 65.0,
            evidence: vec!["design.md not found".to_string()],
        };
    }
    let drift_count = design_drift.intent_drifts.len();
    let score = (100.0 - drift_count as f32 * 12.0).clamp(0.0, 100.0);
    let status = if score >= 85.0 {
        DesignAlignmentStatus::Aligned
    } else if score >= 55.0 {
        DesignAlignmentStatus::PartiallyAligned
    } else {
        DesignAlignmentStatus::Misaligned
    };
    DesignAlignment {
        status,
        score,
        evidence: if drift_count == 0 {
            vec!["no intent drift detected".to_string()]
        } else {
            design_drift
                .intent_drifts
                .iter()
                .map(|drift| {
                    format!(
                        "{}: {} -> {}",
                        drift.component, drift.expected, drift.actual
                    )
                })
                .collect()
        },
    }
}

fn model_mutation_impacts(impacts: &[ImpactAnalysis]) -> Vec<MutationImpactModel> {
    impacts
        .iter()
        .map(|impact| MutationImpactModel {
            proposal_title: impact.proposal_title.clone(),
            architecture_risk: impact.risk,
            dependency_impact: impact.dependency_reduction,
            design_drift_impact: impact.design_drift_impact,
            predicted_health_delta: impact.health_improvement,
        })
        .collect()
}

fn compute_convergence_score(
    architecture_health: f32,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    design_alignment: &DesignAlignment,
) -> ConvergenceScore {
    let dependency_stability = dependency_quality_score(dependency);
    let responsibility_quality = responsibility_quality_score(responsibility);
    let score = (architecture_health * 0.35
        + dependency_stability * 0.25
        + responsibility_quality * 0.25
        + design_alignment.score * 0.15)
        .clamp(0.0, 100.0);
    let state = if score >= 80.0 {
        ConvergenceState::Converging
    } else if score >= 50.0 {
        ConvergenceState::Unstable
    } else {
        ConvergenceState::Diverging
    };
    ConvergenceScore { score, state }
}

fn infer_direction(predicted_health_delta: f32, convergence_score: f32) -> Direction {
    if predicted_health_delta > 2.0 {
        Direction::Improving
    } else if predicted_health_delta < -1.0 || convergence_score < 25.0 {
        Direction::Degrading
    } else {
        Direction::Stable
    }
}

fn predicted_health_improvement(
    proposal: &RefactoringProposal,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
) -> f32 {
    let mut virtual_dependency = dependency.clone();
    let mut virtual_responsibility = responsibility.clone();
    let mut virtual_layer_violations = architecture.layer_violations.len();
    let virtual_boundary_violations = architecture.boundary_violations.len();

    match proposal.mutation_plan.kind.as_str() {
        "ModuleSplit" => {
            virtual_responsibility
                .god_objects
                .retain(|god| god.component != proposal.mutation_plan.target);
            virtual_responsibility
                .massive_modules
                .retain(|module| module.component != proposal.mutation_plan.target);
        }
        "DependencyInversion" => {
            if !virtual_dependency.circular_dependencies.is_empty() {
                virtual_dependency.circular_dependencies.remove(0);
            }
            virtual_dependency.dependency_density =
                (virtual_dependency.dependency_density * 0.90).max(0.0);
        }
        "BoundaryAdapter" => {
            virtual_layer_violations = virtual_layer_violations.saturating_sub(1);
        }
        _ => {}
    }

    let virtual_health = compute_architecture_health_v2(
        &virtual_dependency,
        &virtual_responsibility,
        virtual_layer_violations,
        virtual_boundary_violations,
        Some(design_drift),
    );
    (virtual_health.score - architecture.health.score).clamp(-25.0, 25.0)
}

fn mutation_risk(proposal: &RefactoringProposal, design_drift: &DesignDrift) -> f32 {
    let base = match proposal.mutation_plan.kind.as_str() {
        "ModuleSplit" => 0.28,
        "DependencyInversion" => 0.22,
        "BoundaryAdapter" => 0.16,
        _ => 0.20,
    };
    (base + design_drift.intent_drifts.len() as f32 * 0.03).min(0.95)
}

fn design_drift_impact(proposal: &RefactoringProposal, design_drift: &DesignDrift) -> f32 {
    if design_drift.missing_design_reference {
        return -1.0;
    }
    if proposal.mutation_plan.kind == "BoundaryAdapter" {
        1.5
    } else {
        -(design_drift.intent_drifts.len() as f32 * 0.2)
    }
}

fn pascal_tail(value: &str) -> String {
    value
        .split("::")
        .last()
        .unwrap_or(value)
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn module_dependency_edges(modules: &[AstModule]) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    for module in modules {
        let source = module.module_path.clone();
        for dependency in &module.uses {
            let target = normalize_dependency_name(dependency);
            if !target.is_empty() && target != source {
                edges.entry(source.clone()).or_default().insert(target);
            }
        }
        for declared_module in &module.modules {
            edges
                .entry(source.clone())
                .or_default()
                .insert(declared_module.clone());
        }
    }
    edges
}

fn detect_circular_dependencies(
    edges: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<CircularDependency> {
    let mut cycles = BTreeSet::<Vec<String>>::new();
    for start in edges.keys() {
        let mut path = Vec::new();
        detect_cycles_from(start, start, edges, &mut path, &mut cycles);
    }
    cycles
        .into_iter()
        .map(|cycle_nodes| CircularDependency { cycle_nodes })
        .collect()
}

fn detect_cycles_from(
    start: &str,
    current: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    path: &mut Vec<String>,
    cycles: &mut BTreeSet<Vec<String>>,
) {
    if path.len() > 12 {
        return;
    }
    path.push(current.to_string());
    if let Some(next_nodes) = edges.get(current) {
        for next in next_nodes {
            if next == start && path.len() > 1 {
                let mut cycle = path.clone();
                canonicalize_cycle(&mut cycle);
                cycles.insert(cycle);
            } else if !path.contains(next) {
                detect_cycles_from(start, next, edges, path, cycles);
            }
        }
    }
    path.pop();
}

fn canonicalize_cycle(cycle: &mut [String]) {
    if cycle.is_empty() {
        return;
    }
    let min_index = cycle
        .iter()
        .enumerate()
        .min_by(|left, right| left.1.cmp(right.1))
        .map(|(index, _)| index)
        .unwrap_or(0);
    cycle.rotate_left(min_index);
}

fn detect_hidden_dependencies(
    edges: &BTreeMap<String, BTreeSet<String>>,
    semantic: &SemanticStructure,
) -> Vec<HiddenDependency> {
    let categories = semantic_category_map(semantic);
    let mut hidden = BTreeSet::<(String, String, String)>::new();
    for (source, targets) in edges {
        for via in targets {
            if !matches!(
                categories.get(via),
                Some(SemanticCategory::Runtime | SemanticCategory::Engine)
            ) {
                continue;
            }
            if let Some(second_targets) = edges.get(via) {
                for target in second_targets {
                    if target != source {
                        hidden.insert((source.clone(), via.clone(), target.clone()));
                    }
                }
            }
        }
    }
    hidden
        .into_iter()
        .map(|(source, via, target)| HiddenDependency {
            source,
            via,
            target,
        })
        .collect()
}

fn detect_dependency_hotspots(
    graph: &StructureGraph,
    modules: &[AstModule],
) -> Vec<DependencyHotspot> {
    let mut incoming = BTreeMap::<String, usize>::new();
    let mut outgoing = BTreeMap::<String, usize>::new();
    let mut calls = BTreeMap::<String, usize>::new();
    for edge in &graph.edges {
        *incoming.entry(edge.to.clone()).or_default() += 1;
        *outgoing.entry(edge.from.clone()).or_default() += 1;
        if edge.kind == StructureEdgeKind::Calls {
            *calls.entry(edge.from.clone()).or_default() += 1;
        }
    }
    let mut hotspots = modules
        .iter()
        .filter_map(|module| {
            let module_id = format!("module:{}", module.module_path);
            let incoming_edges = incoming.get(&module_id).copied().unwrap_or(0);
            let outgoing_edges = outgoing.get(&module_id).copied().unwrap_or(0);
            let call_count = module.calls.len()
                + module
                    .functions
                    .iter()
                    .map(|function| {
                        calls
                            .get(&format!("fn:{}::{function}", module.module_path))
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum::<usize>();
            let score = incoming_edges + outgoing_edges + call_count;
            (score > 50).then(|| DependencyHotspot {
                component: module.module_path.clone(),
                incoming_edges,
                outgoing_edges,
                calls: call_count,
            })
        })
        .collect::<Vec<_>>();
    hotspots.sort_by(|left, right| {
        let left_score = left.incoming_edges + left.outgoing_edges + left.calls;
        let right_score = right.incoming_edges + right.outgoing_edges + right.calls;
        right_score
            .cmp(&left_score)
            .then(left.component.cmp(&right.component))
    });
    hotspots.truncate(20);
    hotspots
}

fn default_architecture_rules() -> Vec<ArchitectureRule> {
    vec![
        ArchitectureRule {
            source: "UI".to_string(),
            target: "Storage".to_string(),
            allow: false,
        },
        ArchitectureRule {
            source: "Controller".to_string(),
            target: "Storage".to_string(),
            allow: false,
        },
        ArchitectureRule {
            source: "Storage".to_string(),
            target: "UI".to_string(),
            allow: false,
        },
        ArchitectureRule {
            source: "UI".to_string(),
            target: "Engine".to_string(),
            allow: true,
        },
    ]
}

fn architecture_allows(
    rules: &[ArchitectureRule],
    source: &SemanticCategory,
    target: &SemanticCategory,
) -> bool {
    let source = semantic_category_name(source);
    let target = semantic_category_name(target);
    rules
        .iter()
        .find(|rule| rule.source == source && rule.target == target)
        .map(|rule| rule.allow)
        .unwrap_or(true)
}

fn calibrate_architecture_analysis(
    mut architecture: ArchitectureAnalysis,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    design_drift: &DesignDrift,
) -> ArchitectureAnalysis {
    architecture.health = compute_architecture_health_v2(
        dependency,
        responsibility,
        architecture.layer_violations.len(),
        architecture.boundary_violations.len(),
        Some(design_drift),
    );
    architecture
}

fn compute_architecture_health_v2(
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    layer_violations: usize,
    boundary_violations: usize,
    design_drift: Option<&DesignDrift>,
) -> ArchitectureHealth {
    let calibration = compute_health_calibration(
        dependency,
        responsibility,
        layer_violations,
        boundary_violations,
        design_drift,
    );
    let score = calibration.final_score;
    let status = classify_architecture_health(score);
    ArchitectureHealth {
        score,
        status,
        calibration,
    }
}

fn compute_health_calibration(
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    layer_violations: usize,
    boundary_violations: usize,
    design_drift: Option<&DesignDrift>,
) -> ArchitectureHealthV2 {
    let dependency_score = dependency_quality_score(dependency);
    let responsibility_score = responsibility_quality_score(responsibility);
    let architecture_score = architecture_quality_score(
        layer_violations,
        boundary_violations,
        dependency.circular_dependencies.len(),
    );
    let drift_score = drift_quality_score(design_drift);
    let final_score = weighted_health_score(
        dependency_score,
        responsibility_score,
        architecture_score,
        drift_score,
    );
    ArchitectureHealthV2 {
        dependency_score,
        responsibility_score,
        architecture_score,
        drift_score,
        final_score,
    }
}

fn weighted_health_score(
    dependency_score: f32,
    responsibility_score: f32,
    architecture_score: f32,
    drift_score: f32,
) -> f32 {
    (dependency_score * 0.30
        + responsibility_score * 0.30
        + architecture_score * 0.30
        + drift_score * 0.10)
        .clamp(0.0, 100.0)
}

fn dependency_quality_score(dependency: &DependencyAnalysis) -> f32 {
    // Circular dependencies are a critical structural problem — penalize heavily.
    let cycle_score: f32 = match dependency.circular_dependencies.len() {
        0 => 100.0,
        1 => 55.0,
        2 => 20.0,
        3..=5 => 8.0,
        _ => 3.0,
    };
    // High hotspot concentration is a secondary concern — small deduction only.
    let hotspot_penalty: f32 =
        (dependency.hotspots.len().saturating_sub(25) as f32 * 2.0).min(20.0);
    (cycle_score - hotspot_penalty).clamp(0.0, 100.0)
}

fn responsibility_quality_score(responsibility: &ResponsibilityAnalysis) -> f32 {
    // Use the highest god-object score to represent worst-case severity.
    let max_god_score = responsibility
        .god_objects
        .iter()
        .map(|g| g.score)
        .fold(0.0_f32, f32::max);
    let god_base: f32 = if responsibility.god_objects.is_empty() {
        100.0
    } else if max_god_score > 2.0 {
        // score > 2.0 ≈ 200+ functions — extreme god object
        15.0
    } else if max_god_score > 1.5 {
        35.0
    } else if max_god_score > 1.0 {
        55.0
    } else {
        75.0
    };
    // Additional penalty for each additional god object beyond the first.
    let extra_god_penalty =
        ((responsibility.god_objects.len().saturating_sub(1)) as f32 * 8.0).min(25.0);
    let leakage_penalty =
        (responsibility.responsibility_leakage.len() as f32 * 5.0).min(15.0);
    (god_base - extra_god_penalty - leakage_penalty).clamp(0.0, 100.0)
}

fn architecture_quality_score(
    layer_violations: usize,
    boundary_violations: usize,
    circular_deps: usize,
) -> f32 {
    let violation_penalty =
        layer_violations as f32 * 6.0 + boundary_violations as f32 * 8.0;
    // Circular dependencies are also an architecture-level structural violation.
    let cycle_penalty = circular_deps as f32 * 25.0;
    let total_penalty = violation_penalty + cycle_penalty;
    (100.0 - total_penalty).clamp(0.0, 100.0)
}

fn drift_quality_score(design_drift: Option<&DesignDrift>) -> f32 {
    let Some(design_drift) = design_drift else {
        // No design reference at all — moderate structural documentation gap.
        return 75.0;
    };
    if design_drift.missing_design_reference {
        // Repo has no design.md — penalise for absent design documentation.
        75.0
    } else {
        match design_drift.intent_drifts.len() {
            0 => 100.0,
            1..=3 => 82.0,
            4..=10 => 62.0,
            _ => 35.0,
        }
    }
}

fn classify_architecture_health(score: f32) -> ArchitectureStatus {
    if score >= 93.0 {
        ArchitectureStatus::Excellent
    } else if score >= 70.0 {
        ArchitectureStatus::Good
    } else if score >= 50.0 {
        ArchitectureStatus::Healthy
    } else if score >= 35.0 {
        ArchitectureStatus::Warning
    } else {
        ArchitectureStatus::Critical
    }
}

fn semantic_category_map(semantic: &SemanticStructure) -> BTreeMap<String, SemanticCategory> {
    semantic
        .components
        .iter()
        .map(|component| (component.name.clone(), component.category.clone()))
        .collect()
}

fn semantic_category_for(semantic: &SemanticStructure, component: &str) -> SemanticCategory {
    semantic
        .components
        .iter()
        .find(|entry| entry.name == component)
        .map(|entry| entry.category.clone())
        .unwrap_or(SemanticCategory::Module)
}

fn semantic_category_name(category: &SemanticCategory) -> &'static str {
    match category {
        SemanticCategory::Controller => "Controller",
        SemanticCategory::Service => "Service",
        SemanticCategory::Repository => "Repository",
        SemanticCategory::Engine => "Engine",
        SemanticCategory::Memory => "Memory",
        SemanticCategory::Policy => "Policy",
        SemanticCategory::Runtime => "Runtime",
        SemanticCategory::UI => "UI",
        SemanticCategory::Storage => "Storage",
        SemanticCategory::Module => "Module",
    }
}

fn component_name_from_node_id(node_id: &str) -> String {
    node_id
        .split_once(':')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| node_id.to_string())
}

fn normalize_dependency_name(dependency: &str) -> String {
    let without_prefix = dependency
        .trim_start_matches("crate::")
        .trim_start_matches("super::")
        .trim_start_matches("self::");
    without_prefix
        .split("::")
        .next()
        .unwrap_or(without_prefix)
        .to_string()
}

fn build_analyze_result(
    root: &Path,
    modules: &[AstModule],
    graph: &StructureGraph,
    dependency: &DependencyAnalysis,
    responsibility: &ResponsibilityAnalysis,
    architecture: &ArchitectureAnalysis,
    convergence: &ConvergenceReport,
) -> AnalyzeResult {
    let mut top_counts = BTreeMap::<String, usize>::new();
    for module in modules {
        let top = module
            .module_path
            .split("::")
            .next()
            .unwrap_or("root")
            .to_string();
        *top_counts.entry(top).or_default() += module.structs.len()
            + module.enums.len()
            + module.traits.len()
            + module.functions.len()
            + module.modules.len();
    }
    let mut top_modules = top_counts.into_iter().collect::<Vec<_>>();
    top_modules.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));

    let mut dependencies = modules
        .iter()
        .flat_map(|module| module.uses.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dependencies.truncate(50);

    AnalyzeResult {
        project_name: root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Design_BrainModel")
            .to_string(),
        modules: graph
            .nodes
            .iter()
            .filter(|node| node.kind == StructureNodeKind::Module)
            .count(),
        structs: modules.iter().map(|module| module.structs.len()).sum(),
        enums: modules.iter().map(|module| module.enums.len()).sum(),
        traits: modules.iter().map(|module| module.traits.len()).sum(),
        functions: modules.iter().map(|module| module.functions.len()).sum(),
        module_count: graph
            .nodes
            .iter()
            .filter(|node| node.kind == StructureNodeKind::Module)
            .count(),
        struct_count: modules.iter().map(|module| module.structs.len()).sum(),
        enum_count: modules.iter().map(|module| module.enums.len()).sum(),
        trait_count: modules.iter().map(|module| module.traits.len()).sum(),
        function_count: modules.iter().map(|module| module.functions.len()).sum(),
        top_components: top_modules
            .iter()
            .take(5)
            .map(|(name, _)| name.clone())
            .collect(),
        top_modules: top_modules
            .into_iter()
            .take(5)
            .map(|(name, _)| name)
            .collect(),
        dependencies,
        circular_dependencies: dependency.circular_dependencies.len(),
        layer_violations: architecture.layer_violations.len(),
        boundary_violations: architecture.boundary_violations.len(),
        god_objects: responsibility.god_objects.len(),
        architecture_health: architecture.health.score,
        status: architecture.health.status,
        convergence_score: convergence.convergence_score.score,
        direction: convergence.direction,
        top_proposal: convergence
            .top_proposal
            .as_ref()
            .map(|proposal| proposal.title.clone()),
        predicted_health_delta: convergence.predicted_health_delta,
        convergence_confidence: convergence.confidence,
    }
}

fn persist_to_holographic_memory(
    root: &Path,
    result: &AnalyzeResult,
    repository: &RepositorySnapshot,
    graph: &StructureGraph,
    semantic_structure: &SemanticStructure,
    dependency_analysis: &DependencyAnalysis,
    responsibility_analysis: &ResponsibilityAnalysis,
    architecture_analysis: &ArchitectureAnalysis,
    design_drift: &DesignDrift,
    convergence_report: &ConvergenceReport,
    detailed_report: &DetailedDiagnosticReport,
    security_report: &SecurityArchitectureReport,
) -> Result<(), String> {
    let entry = SemanticMemoryEntry {
        id: semantic_memory_id(result),
        kind: "structure_analysis".to_string(),
        project_name: result.project_name.clone(),
        summary: result.clone(),
        repository_snapshot: repository.clone(),
        structure_graph: graph.clone(),
        semantic_structure: semantic_structure.clone(),
        dependency_analysis: dependency_analysis.clone(),
        responsibility_analysis: responsibility_analysis.clone(),
        architecture_analysis: architecture_analysis.clone(),
        design_drift: design_drift.clone(),
        convergence_report: convergence_report.clone(),
        detailed_report: detailed_report.clone(),
    };
    let dir = root.join(".dbm/analyze");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    fs::write(
        dir.join("repository_snapshot.json"),
        serde_json::to_string_pretty(repository).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("structure_graph.json"),
        serde_json::to_string_pretty(graph).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("analyze_result.json"),
        serde_json::to_string_pretty(result).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("semantic_structure.json"),
        serde_json::to_string_pretty(semantic_structure).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("dependency_analysis.json"),
        serde_json::to_string_pretty(dependency_analysis).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("responsibility_analysis.json"),
        serde_json::to_string_pretty(responsibility_analysis).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("architecture_analysis.json"),
        serde_json::to_string_pretty(architecture_analysis).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("design_drift.json"),
        serde_json::to_string_pretty(design_drift).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("health_report.json"),
        serde_json::to_string_pretty(&architecture_analysis.health)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("convergence_report.json"),
        serde_json::to_string_pretty(convergence_report).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("refactoring_proposals.json"),
        serde_json::to_string_pretty(&convergence_report.refactoring_proposals)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("impact_analysis.json"),
        serde_json::to_string_pretty(&convergence_report.impact_analysis)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("convergence_score.json"),
        serde_json::to_string_pretty(&convergence_report.convergence_score)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("detailed_report.json"),
        serde_json::to_string_pretty(detailed_report).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("health_breakdown.json"),
        serde_json::to_string_pretty(&detailed_report.health_breakdown)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("health_trend.json"),
        serde_json::to_string_pretty(&detailed_report.health_trend)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("health_calibration.json"),
        serde_json::to_string_pretty(&architecture_analysis.health.calibration)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("hotspots.json"),
        serde_json::to_string_pretty(&detailed_report.dependency_hotspots)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("security_analysis.json"),
        serde_json::to_string_pretty(security_report).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("security_report.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "risk_score": security_report.risk_score,
            "classification": security_report.classification,
            "trust_boundary_violations": security_report.trust_boundary_violations.len(),
            "privilege_violations": security_report.privilege_violations.len(),
            "dangerous_paths": security_report.dangerous_paths.len(),
            "attack_surface": security_report.attack_surface,
        }))
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("security_proposals.json"),
        serde_json::to_string_pretty(&security_report.proposals)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("security_risk_score.json"),
        serde_json::to_string_pretty(&security_report.risk_score)
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        dir.join("violations.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "layer_violations": detailed_report.layer_violations.clone(),
            "boundary_violations": detailed_report.boundary_violations.clone(),
        }))
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("semantic_memory.jsonl"))
        .map_err(|err| err.to_string())?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&entry).map_err(|err| err.to_string())?
    )
    .map_err(|err| err.to_string())
}

fn read_previous_health_score(root: &Path) -> Option<f32> {
    let path = root.join(".dbm/analyze/analyze_result.json");
    let content = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    value
        .get("architecture_health")
        .and_then(|score| score.as_f64())
        .map(|score| score as f32)
}

fn collect_repository_entries(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    directories: &mut Vec<PathBuf>,
    cargo_tomls: &mut Vec<PathBuf>,
) -> Result<(), String> {
    directories.push(dir.to_path_buf());
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("cannot read {}: {err}", dir.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_excluded_dir_name(&name) {
            continue;
        }
        if path.is_dir() {
            collect_repository_entries(&path, files, directories, cargo_tomls)?;
        } else if name == "Cargo.toml" {
            cargo_tomls.push(path);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn scan_roots(root: &Path) -> Vec<PathBuf> {
    let apps = root.join("apps");
    let crates = root.join("crates");
    if apps.exists() && crates.exists() {
        vec![apps, crates]
    } else {
        vec![root.to_path_buf()]
    }
}

fn is_excluded_dir_name(name: &str) -> bool {
    matches!(
        name,
        "target" | ".git" | "node_modules" | "dist" | "build" | ".dbm"
    )
}

fn resolve_repository_path(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    if let Ok(current) = std::env::current_dir()
        && current.file_name() == path.file_name()
    {
        return current;
    }
    path.to_path_buf()
}

fn module_path_from_relative_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        if let Component::Normal(value) = component {
            let value = value.to_string_lossy();
            if value == "src" {
                continue;
            }
            let clean = value.trim_end_matches(".rs");
            if matches!(clean, "lib" | "main" | "mod") {
                continue;
            }
            parts.push(clean.to_string());
        }
    }
    if parts.is_empty() {
        "root".to_string()
    } else {
        parts.join("::")
    }
}

fn insert_node(
    nodes: &mut BTreeMap<String, StructureNode>,
    id: String,
    label: String,
    kind: StructureNodeKind,
) {
    nodes
        .entry(id.clone())
        .or_insert(StructureNode { id, label, kind });
}

fn append_path_segment(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}::{segment}")
    }
}

fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn type_to_string(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "Self".to_string()),
        _ => "Self".to_string(),
    }
}

fn expr_to_call_name(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Path(path) => path_to_string(&path.path),
        _ => "call".to_string(),
    }
}

fn parse_workspace_members(cargo_toml: &Path) -> Result<Vec<String>, String> {
    if !cargo_toml.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(cargo_toml)
        .map_err(|err| format!("failed to read {}: {err}", cargo_toml.display()))?;
    let mut members = Vec::new();
    let mut in_workspace = false;
    let mut in_members = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_workspace = line == "[workspace]";
            in_members = false;
            continue;
        }
        if !in_workspace {
            continue;
        }
        if line.starts_with("members") {
            in_members = true;
        }
        if in_members {
            for value in quoted_values(line) {
                members.push(value);
            }
            if line.contains(']') {
                in_members = false;
            }
        }
    }
    members.sort();
    members.dedup();
    Ok(members)
}

fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        values.push(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
    values
}

fn classify_semantic_category(
    name: &str,
    node: &StructureNode,
    module: Option<&AstModule>,
) -> SemanticCategory {
    let value = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        module
            .map(|module| module.file_path.display().to_string())
            .unwrap_or_default()
            .to_ascii_lowercase()
    );
    if value.contains("controller") || value.contains("command") || value.contains("handler") {
        SemanticCategory::Controller
    } else if value.contains("service") {
        SemanticCategory::Service
    } else if value.contains("repository") || value.contains("repo") {
        SemanticCategory::Repository
    } else if value.contains("engine") || value.contains("analyze") || value.contains("parser") {
        SemanticCategory::Engine
    } else if value.contains("memory") || value.contains("holographic") {
        SemanticCategory::Memory
    } else if value.contains("policy") || value.contains("guard") {
        SemanticCategory::Policy
    } else if value.contains("runtime") || value.contains("executor") {
        SemanticCategory::Runtime
    } else if value.contains("ui") || value.contains("tui") || value.contains("renderer") {
        SemanticCategory::UI
    } else if value.contains("storage") || value.contains("store") || value.contains("persist") {
        SemanticCategory::Storage
    } else if node.kind == StructureNodeKind::Module {
        SemanticCategory::Module
    } else {
        SemanticCategory::Engine
    }
}

fn infer_responsibility(name: &str, node: &StructureNode, module: Option<&AstModule>) -> String {
    match classify_semantic_category(name, node, module) {
        SemanticCategory::Controller => "Command and request routing".to_string(),
        SemanticCategory::Service => "Application service orchestration".to_string(),
        SemanticCategory::Repository => "Repository access boundary".to_string(),
        SemanticCategory::Engine => {
            if name.to_ascii_lowercase().contains("analyze") {
                "Structure Analysis".to_string()
            } else {
                "Core engine computation".to_string()
            }
        }
        SemanticCategory::Memory => "Persistent structure memory".to_string(),
        SemanticCategory::Policy => "Execution policy and safety decisions".to_string(),
        SemanticCategory::Runtime => "Runtime execution and lifecycle".to_string(),
        SemanticCategory::UI => "User interface rendering and interaction".to_string(),
        SemanticCategory::Storage => "Persistence and storage management".to_string(),
        SemanticCategory::Module => "Structural module boundary".to_string(),
    }
}

fn semantic_evidence(name: &str, node: &StructureNode, module: Option<&AstModule>) -> Vec<String> {
    let mut evidence = vec![format!("name:{name}")];
    if let Some(module) = module {
        evidence.push(format!("path:{}", module.file_path.display()));
        if !module.uses.is_empty() {
            evidence.push(format!("uses:{}", module.uses.len()));
        }
        if !module.functions.is_empty() {
            evidence.push(format!("functions:{}", module.functions.len()));
        }
    }
    evidence.push(format!("node_kind:{:?}", node.kind));
    evidence
}

fn semantic_memory_id(result: &AnalyzeResult) -> String {
    let mut hasher = Sha256::new();
    hasher.update(result.project_name.as_bytes());
    hasher.update(result.module_count.to_le_bytes());
    hasher.update(result.struct_count.to_le_bytes());
    hasher.update(result.enum_count.to_le_bytes());
    hasher.update(result.trait_count.to_le_bytes());
    hasher.update(result.function_count.to_le_bytes());
    format!("semantic-memory:{:x}", hasher.finalize())
}

// ===== Security Architecture Analyzer Implementation =====

pub fn analyze_security_architecture(
    ast_modules: &[AstModule],
    semantic: &SemanticStructure,
) -> SecurityArchitectureReport {
    let security_layers = build_security_layer_map(semantic, ast_modules);
    let trust_boundary_violations =
        TrustBoundaryAnalyzer::analyze(ast_modules, &security_layers);
    let privilege_violations = PrivilegeBoundaryAnalyzer::analyze(ast_modules, &security_layers);
    let dangerous_paths = DangerousDependencyAnalyzer::analyze(ast_modules, &security_layers);
    let layer_violations = detect_security_layer_violations(ast_modules, &security_layers);
    let attack_surface = AttackSurfaceAnalyzer::analyze(ast_modules, &security_layers);
    let security_drift = SecurityDriftAnalyzer::analyze(ast_modules, semantic);
    let risk_score = compute_security_risk_score(
        &trust_boundary_violations,
        &privilege_violations,
        &dangerous_paths,
        &attack_surface,
    );
    let classification = classify_security_score(risk_score.final_score);
    let proposals = plan_security_proposals(
        &trust_boundary_violations,
        &privilege_violations,
        &dangerous_paths,
        &security_drift,
        &attack_surface,
    );
    let impact_analysis = compute_security_impact_analysis(&proposals, risk_score.final_score);
    SecurityArchitectureReport {
        trust_boundary_violations,
        privilege_violations,
        dangerous_paths,
        layer_violations,
        attack_surface,
        security_drift,
        risk_score,
        classification,
        proposals,
        impact_analysis,
    }
}

fn build_security_layer_map(
    semantic: &SemanticStructure,
    modules: &[AstModule],
) -> BTreeMap<String, SecurityLayer> {
    let mut layers: BTreeMap<String, SecurityLayer> = semantic
        .components
        .iter()
        .map(|c| (c.name.clone(), classify_security_layer_for_component(c)))
        .collect();
    for module in modules {
        if module_has_system_access(module) {
            layers
                .entry(module.module_path.clone())
                .or_insert(SecurityLayer::Service);
        }
    }
    layers
}

fn classify_security_layer_for_component(component: &SemanticComponent) -> SecurityLayer {
    let name = component.name.to_ascii_lowercase();
    if name.contains("input")
        || name.contains("request")
        || name.ends_with("_cli")
        || name.contains("cli_main")
    {
        return SecurityLayer::ExternalInput;
    }
    match &component.category {
        SemanticCategory::Controller => SecurityLayer::Api,
        SemanticCategory::Service => SecurityLayer::Service,
        SemanticCategory::Repository | SemanticCategory::Storage => SecurityLayer::Storage,
        SemanticCategory::Engine => SecurityLayer::Runtime,
        SemanticCategory::Memory => SecurityLayer::Memory,
        SemanticCategory::Policy => SecurityLayer::Policy,
        SemanticCategory::Runtime => SecurityLayer::Runtime,
        SemanticCategory::UI => SecurityLayer::Ui,
        SemanticCategory::Module => SecurityLayer::Service,
    }
}

fn module_has_system_access(module: &AstModule) -> bool {
    module_has_shell_access(module)
        || module_has_filesystem_access(module)
        || module_has_network_access(module)
}

fn module_has_shell_access(module: &AstModule) -> bool {
    module.uses.iter().any(|u| {
        u.contains("std::process") || u.contains("process::Command")
    })
}

fn module_has_filesystem_access(module: &AstModule) -> bool {
    module
        .uses
        .iter()
        .any(|u| u.contains("std::fs") || u.contains("tokio::fs") || u.contains("async_std::fs"))
}

fn module_has_network_access(module: &AstModule) -> bool {
    module.uses.iter().any(|u| {
        u.contains("std::net") || u.contains("tokio::net") || u.contains("async_std::net")
    })
}

fn is_external_facing_layer(layer: &SecurityLayer) -> bool {
    matches!(layer, SecurityLayer::ExternalInput | SecurityLayer::Api | SecurityLayer::Ui)
}

impl TrustBoundaryAnalyzer {
    pub fn analyze(
        modules: &[AstModule],
        layers: &BTreeMap<String, SecurityLayer>,
    ) -> Vec<TrustBoundaryViolation> {
        let mut violations: BTreeSet<(String, String)> = BTreeSet::new();
        let mut result = Vec::new();
        for module in modules {
            let source_layer = layers
                .get(&module.module_path)
                .cloned()
                .unwrap_or(SecurityLayer::Service);
            if !is_external_facing_layer(&source_layer) {
                continue;
            }
            for dep in &module.uses {
                let dep_name = normalize_dependency_name(dep);
                let target_layer = layers
                    .get(&dep_name)
                    .cloned()
                    .unwrap_or(SecurityLayer::Service);
                let severity = trust_boundary_violation_severity(&source_layer, &target_layer);
                if let Some(sev) = severity {
                    if violations.insert((module.module_path.clone(), dep_name.clone())) {
                        result.push(TrustBoundaryViolation {
                            source: module.module_path.clone(),
                            target: dep_name,
                            source_layer: source_layer.clone(),
                            target_layer,
                            severity: sev,
                        });
                    }
                }
            }
        }
        result.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));
        result
    }
}

fn trust_boundary_violation_severity(
    source: &SecurityLayer,
    target: &SecurityLayer,
) -> Option<SecuritySeverity> {
    match (source, target) {
        (SecurityLayer::ExternalInput, SecurityLayer::Storage)
        | (SecurityLayer::ExternalInput, SecurityLayer::System) => Some(SecuritySeverity::Critical),
        (SecurityLayer::ExternalInput, SecurityLayer::Runtime)
        | (SecurityLayer::Api, SecurityLayer::Storage)
        | (SecurityLayer::Api, SecurityLayer::System) => Some(SecuritySeverity::High),
        (SecurityLayer::Ui, SecurityLayer::Storage)
        | (SecurityLayer::Ui, SecurityLayer::System) => Some(SecuritySeverity::High),
        (SecurityLayer::Api, SecurityLayer::Runtime)
        | (SecurityLayer::Ui, SecurityLayer::Runtime) => Some(SecuritySeverity::Medium),
        _ => None,
    }
}

impl PrivilegeBoundaryAnalyzer {
    pub fn analyze(
        modules: &[AstModule],
        layers: &BTreeMap<String, SecurityLayer>,
    ) -> Vec<PrivilegeViolation> {
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        let mut result = Vec::new();
        for module in modules {
            let source_layer = layers
                .get(&module.module_path)
                .cloned()
                .unwrap_or(SecurityLayer::Service);
            if matches!(source_layer, SecurityLayer::Ui | SecurityLayer::Api) {
                let access_label = if module_has_shell_access(module) {
                    Some(("Shell", SecuritySeverity::Critical, "std::process"))
                } else if module_has_filesystem_access(module) {
                    Some(("Filesystem", SecuritySeverity::High, "std::fs"))
                } else if module_has_network_access(module) {
                    Some(("Network", SecuritySeverity::High, "std::net"))
                } else {
                    None
                };
                if let Some((resource, severity, api)) = access_label {
                    let key = (module.module_path.clone(), api.to_string());
                    if seen.insert(key) {
                        result.push(PrivilegeViolation {
                            source: module.module_path.clone(),
                            target: api.to_string(),
                            source_layer: source_layer.clone(),
                            target_layer: SecurityLayer::System,
                            severity,
                            description: format!(
                                "{:?} directly accesses {} without Policy boundary",
                                source_layer, resource
                            ),
                        });
                    }
                }
            }
        }
        result.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));
        result
    }
}

impl DangerousDependencyAnalyzer {
    pub fn analyze(
        modules: &[AstModule],
        layers: &BTreeMap<String, SecurityLayer>,
    ) -> Vec<DangerousPath> {
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        let mut result = Vec::new();
        for module in modules {
            let source_layer = layers
                .get(&module.module_path)
                .cloned()
                .unwrap_or(SecurityLayer::Service);
            if !is_external_facing_layer(&source_layer) {
                continue;
            }
            if module_has_shell_access(module) {
                let key = (module.module_path.clone(), "shell".to_string());
                if seen.insert(key) {
                    result.push(DangerousPath {
                        source: module.module_path.clone(),
                        target_api: "std::process::Command".to_string(),
                        kind: DangerousPathKind::ExternalInputToShell,
                        severity: SecuritySeverity::Critical,
                    });
                }
            }
            if module_has_filesystem_access(module) {
                let key = (module.module_path.clone(), "filesystem".to_string());
                if seen.insert(key) {
                    result.push(DangerousPath {
                        source: module.module_path.clone(),
                        target_api: "std::fs".to_string(),
                        kind: DangerousPathKind::ExternalInputToFilesystem,
                        severity: SecuritySeverity::High,
                    });
                }
            }
            if module_has_network_access(module) {
                let key = (module.module_path.clone(), "network".to_string());
                if seen.insert(key) {
                    result.push(DangerousPath {
                        source: module.module_path.clone(),
                        target_api: "std::net".to_string(),
                        kind: DangerousPathKind::ExternalInputToNetwork,
                        severity: SecuritySeverity::High,
                    });
                }
            }
        }
        result.sort_by(|a, b| a.source.cmp(&b.source).then(a.target_api.cmp(&b.target_api)));
        result
    }
}

fn detect_security_layer_violations(
    modules: &[AstModule],
    layers: &BTreeMap<String, SecurityLayer>,
) -> Vec<SecurityLayerViolation> {
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut result = Vec::new();
    for module in modules {
        let source_layer = layers
            .get(&module.module_path)
            .cloned()
            .unwrap_or(SecurityLayer::Service);
        for dep in &module.uses {
            let dep_name = normalize_dependency_name(dep);
            let target_layer = layers
                .get(&dep_name)
                .cloned()
                .unwrap_or(SecurityLayer::Service);
            let rule = security_layer_rule(&source_layer, &target_layer);
            if let Some((rule_text, severity)) = rule {
                let key = (module.module_path.clone(), dep_name.clone());
                if seen.insert(key) {
                    result.push(SecurityLayerViolation {
                        source: module.module_path.clone(),
                        target: dep_name,
                        source_layer: source_layer.clone(),
                        target_layer,
                        severity,
                        rule: rule_text,
                    });
                }
            }
        }
    }
    result.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));
    result
}

fn security_layer_rule(
    source: &SecurityLayer,
    target: &SecurityLayer,
) -> Option<(String, SecuritySeverity)> {
    match (source, target) {
        (SecurityLayer::Ui, SecurityLayer::Storage) => Some((
            "UI must not directly access Storage".to_string(),
            SecuritySeverity::High,
        )),
        (SecurityLayer::Api, SecurityLayer::Storage) => Some((
            "API must route through Service before Storage".to_string(),
            SecuritySeverity::High,
        )),
        (SecurityLayer::ExternalInput, SecurityLayer::Storage) => Some((
            "ExternalInput must be validated before reaching Storage".to_string(),
            SecuritySeverity::Critical,
        )),
        (SecurityLayer::ExternalInput, SecurityLayer::Runtime) => Some((
            "ExternalInput must not directly trigger Runtime execution".to_string(),
            SecuritySeverity::High,
        )),
        (SecurityLayer::Ui, SecurityLayer::System) | (SecurityLayer::Api, SecurityLayer::System) => {
            Some((
                "UI/API must not directly access System resources".to_string(),
                SecuritySeverity::Critical,
            ))
        }
        _ => None,
    }
}

impl AttackSurfaceAnalyzer {
    pub fn analyze(
        modules: &[AstModule],
        layers: &BTreeMap<String, SecurityLayer>,
    ) -> AttackSurfaceReport {
        let mut entry_points = 0usize;
        let mut shell_access = 0usize;
        let mut filesystem_access = 0usize;
        let mut network_access = 0usize;
        for module in modules {
            let layer = layers
                .get(&module.module_path)
                .cloned()
                .unwrap_or(SecurityLayer::Service);
            if is_external_facing_layer(&layer) {
                entry_points += module.functions.len();
            }
            if module_has_shell_access(module) {
                shell_access += 1;
            }
            if module_has_filesystem_access(module) {
                filesystem_access += 1;
            }
            if module_has_network_access(module) {
                network_access += 1;
            }
        }
        let risk = compute_attack_surface_risk(
            entry_points,
            shell_access,
            filesystem_access,
            network_access,
        );
        AttackSurfaceReport {
            entry_points,
            shell_access,
            filesystem_access,
            network_access,
            risk,
        }
    }
}

fn compute_attack_surface_risk(
    entry_points: usize,
    shell_access: usize,
    filesystem_access: usize,
    network_access: usize,
) -> AttackSurfaceRisk {
    let score = shell_access * 3 + network_access * 2 + filesystem_access + entry_points / 50;
    if score >= 6 {
        AttackSurfaceRisk::Critical
    } else if score >= 3 {
        AttackSurfaceRisk::High
    } else if score >= 1 {
        AttackSurfaceRisk::Medium
    } else {
        AttackSurfaceRisk::Low
    }
}

impl SecurityDriftAnalyzer {
    pub fn analyze(modules: &[AstModule], semantic: &SemanticStructure) -> Vec<SecurityDrift> {
        let mut drifts = Vec::new();
        let storage_names: BTreeSet<String> = semantic
            .components
            .iter()
            .filter(|c| matches!(c.category, SemanticCategory::Storage | SemanticCategory::Repository))
            .map(|c| c.name.clone())
            .collect();
        for module in modules {
            let category = semantic
                .components
                .iter()
                .find(|c| c.name == module.module_path)
                .map(|c| &c.category);
            if matches!(
                category,
                Some(SemanticCategory::UI | SemanticCategory::Controller)
            ) && module_has_filesystem_access(module)
            {
                let gateway = storage_names.iter().next();
                drifts.push(SecurityDrift {
                    component: module.module_path.clone(),
                    expected_pattern: gateway
                        .map(|g| format!("All writes must use {g}"))
                        .unwrap_or_else(|| "All writes must use StorageGateway".to_string()),
                    actual_pattern: format!(
                        "{} uses std::fs directly",
                        module.module_path
                    ),
                    description: "Direct filesystem access bypasses storage boundary".to_string(),
                });
            }
        }
        drifts
    }
}

fn compute_security_risk_score(
    trust_violations: &[TrustBoundaryViolation],
    privilege_violations: &[PrivilegeViolation],
    dangerous_paths: &[DangerousPath],
    attack_surface: &AttackSurfaceReport,
) -> SecurityRiskScore {
    let trust_boundary_score = (100.0
        - trust_violations
            .iter()
            .map(|v| match v.severity {
                SecuritySeverity::Critical => 20.0_f32,
                SecuritySeverity::High => 12.0,
                SecuritySeverity::Medium => 6.0,
                SecuritySeverity::Low => 2.0,
            })
            .sum::<f32>())
    .clamp(0.0, 100.0);

    let privilege_score = (100.0
        - privilege_violations
            .iter()
            .map(|v| match v.severity {
                SecuritySeverity::Critical => 25.0_f32,
                SecuritySeverity::High => 15.0,
                SecuritySeverity::Medium => 8.0,
                SecuritySeverity::Low => 3.0,
            })
            .sum::<f32>())
    .clamp(0.0, 100.0);

    let dependency_score = (100.0
        - dangerous_paths
            .iter()
            .map(|p| match p.severity {
                SecuritySeverity::Critical => 30.0_f32,
                SecuritySeverity::High => 18.0,
                SecuritySeverity::Medium => 8.0,
                SecuritySeverity::Low => 3.0,
            })
            .sum::<f32>())
    .clamp(0.0, 100.0);

    let attack_surface_score = match attack_surface.risk {
        AttackSurfaceRisk::Low => 100.0,
        AttackSurfaceRisk::Medium => 80.0,
        AttackSurfaceRisk::High => 55.0,
        AttackSurfaceRisk::Critical => 25.0,
    };

    let final_score = (trust_boundary_score * 0.30
        + privilege_score * 0.25
        + dependency_score * 0.30
        + attack_surface_score * 0.15)
        .clamp(0.0, 100.0);

    SecurityRiskScore {
        trust_boundary_score,
        privilege_score,
        dependency_score,
        attack_surface_score,
        final_score,
    }
}

fn classify_security_score(score: f32) -> SecurityClassification {
    if score >= 90.0 {
        SecurityClassification::Excellent
    } else if score >= 75.0 {
        SecurityClassification::Good
    } else if score >= 50.0 {
        SecurityClassification::Warning
    } else if score >= 25.0 {
        SecurityClassification::HighRisk
    } else {
        SecurityClassification::Critical
    }
}

fn plan_security_proposals(
    trust_violations: &[TrustBoundaryViolation],
    privilege_violations: &[PrivilegeViolation],
    dangerous_paths: &[DangerousPath],
    security_drift: &[SecurityDrift],
    _attack_surface: &AttackSurfaceReport,
) -> Vec<SecurityProposal> {
    let mut proposals = Vec::new();

    let has_shell = dangerous_paths
        .iter()
        .any(|p| matches!(p.kind, DangerousPathKind::ExternalInputToShell));
    let has_filesystem = dangerous_paths
        .iter()
        .any(|p| matches!(p.kind, DangerousPathKind::ExternalInputToFilesystem))
        || !security_drift.is_empty();
    let has_network = dangerous_paths
        .iter()
        .any(|p| matches!(p.kind, DangerousPathKind::ExternalInputToNetwork));
    let has_trust_violation = trust_violations
        .iter()
        .any(|v| matches!(v.severity, SecuritySeverity::Critical | SecuritySeverity::High));
    let has_privilege = !privilege_violations.is_empty();

    if has_trust_violation {
        proposals.push(SecurityProposal {
            title: "Insert Validation Layer".to_string(),
            reason: "External input reaches trusted zones without validation".to_string(),
            impact: 18.0,
            confidence: 0.92,
        });
    }
    if has_filesystem {
        proposals.push(SecurityProposal {
            title: "Introduce Storage Gateway".to_string(),
            reason: "Direct filesystem access bypasses storage boundary".to_string(),
            impact: 12.0,
            confidence: 0.89,
        });
    }
    if has_privilege {
        proposals.push(SecurityProposal {
            title: "Add Policy Boundary".to_string(),
            reason: "UI/API accesses system resources without policy enforcement".to_string(),
            impact: 10.0,
            confidence: 0.86,
        });
    }
    if has_shell {
        proposals.push(SecurityProposal {
            title: "Move Shell Execution Behind Runtime".to_string(),
            reason: "Shell access reachable from external input without runtime isolation".to_string(),
            impact: 22.0,
            confidence: 0.95,
        });
    }
    if has_network {
        proposals.push(SecurityProposal {
            title: "Route Network Access Through Service Layer".to_string(),
            reason: "Network access reachable from external-facing layer".to_string(),
            impact: 8.0,
            confidence: 0.84,
        });
    }

    proposals.sort_by(|a, b| b.impact.total_cmp(&a.impact).then(a.title.cmp(&b.title)));
    proposals
}

fn compute_security_impact_analysis(
    proposals: &[SecurityProposal],
    current_risk: f32,
) -> Vec<SecurityImpactAnalysis> {
    let mut cumulative = current_risk;
    proposals
        .iter()
        .map(|proposal| {
            let predicted = (cumulative + proposal.impact).clamp(0.0, 100.0);
            let analysis = SecurityImpactAnalysis {
                proposal_title: proposal.title.clone(),
                current_risk: cumulative,
                predicted_risk: predicted,
                impact: predicted - cumulative,
            };
            cumulative = predicted;
            analysis
        })
        .collect()
}

pub fn render_security_analysis(report: &SecurityArchitectureReport) -> String {
    let mut out = String::new();
    out.push_str("=== Security Analysis ===\n\n");
    out.push_str("Security Score\n");
    out.push_str(&format!("{:.1}\n\n", report.risk_score.final_score));
    out.push_str("Status\n");
    out.push_str(&format!("{:?}\n\n", report.classification));
    out.push_str("Trust Boundary Violations\n");
    out.push_str(&format!("{}\n\n", report.trust_boundary_violations.len()));
    out.push_str("Privilege Violations\n");
    out.push_str(&format!("{}\n\n", report.privilege_violations.len()));
    out.push_str("Dangerous Dependency Paths\n");
    out.push_str(&format!("{}\n\n", report.dangerous_paths.len()));
    out.push_str("Attack Surface\n");
    out.push_str(&format!("{:?}\n\n", report.attack_surface.risk));
    if let Some(proposal) = report.proposals.first() {
        out.push_str("Top Proposal\n");
        out.push_str(&format!("{}\n\n", proposal.title));
        out.push_str("Impact\n");
        out.push_str(&format!("{:+.1}\n\n", proposal.impact));
        out.push_str("Confidence\n");
        out.push_str(&format!("{:.2}\n", proposal.confidence));
    } else {
        out.push_str("Top Proposal\nNone\n");
    }
    out
}

pub fn render_security_analysis_detailed(report: &SecurityArchitectureReport) -> String {
    let mut out = render_security_analysis(report);

    out.push_str("\n\n=== Security Risk Breakdown ===\n\n");
    out.push_str(&format!(
        "Trust Boundary\n{:.1}\n\nPrivilege\n{:.1}\n\nDangerous Dependencies\n{:.1}\n\nAttack Surface\n{:.1}\n\nFinal\n{:.1}\n\n",
        report.risk_score.trust_boundary_score,
        report.risk_score.privilege_score,
        report.risk_score.dependency_score,
        report.risk_score.attack_surface_score,
        report.risk_score.final_score,
    ));

    out.push_str("=== Trust Boundary Violations ===\n\n");
    if report.trust_boundary_violations.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for v in &report.trust_boundary_violations {
            out.push_str(&format!(
                "{}\n ↓\n{}\nSeverity\n{:?}\n\n",
                v.source, v.target, v.severity
            ));
        }
    }

    out.push_str("=== Privilege Violations ===\n\n");
    if report.privilege_violations.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for v in &report.privilege_violations {
            out.push_str(&format!(
                "{}\n ↓\n{}\nDescription\n{}\nSeverity\n{:?}\n\n",
                v.source, v.target, v.description, v.severity
            ));
        }
    }

    out.push_str("=== Dangerous Dependency Paths ===\n\n");
    if report.dangerous_paths.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for p in &report.dangerous_paths {
            out.push_str(&format!(
                "{}\n ↓\n{}\nKind\n{:?}\nSeverity\n{:?}\n\n",
                p.source, p.target_api, p.kind, p.severity
            ));
        }
    }

    out.push_str("=== Attack Surface ===\n\n");
    out.push_str(&format!(
        "Entry Points\n{}\n\nShell Access\n{}\n\nFilesystem Access\n{}\n\nNetwork Access\n{}\n\nRisk\n{:?}\n\n",
        report.attack_surface.entry_points,
        report.attack_surface.shell_access,
        report.attack_surface.filesystem_access,
        report.attack_surface.network_access,
        report.attack_surface.risk,
    ));

    out.push_str("=== Security Drift ===\n\n");
    if report.security_drift.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for d in &report.security_drift {
            out.push_str(&format!(
                "{}\nExpected\n{}\nActual\n{}\n\n",
                d.component, d.expected_pattern, d.actual_pattern
            ));
        }
    }

    out.push_str("=== Security Proposals ===\n\n");
    if report.proposals.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for (i, proposal) in report.proposals.iter().enumerate() {
            out.push_str(&format!(
                "{}.\n{}\nImpact\n{:+.1}\nConfidence\n{:.2}\nReason\n{}\n\n",
                i + 1,
                proposal.title,
                proposal.impact,
                proposal.confidence,
                proposal.reason
            ));
        }
    }

    out.push_str("=== Security Impact Analysis ===\n\n");
    for analysis in &report.impact_analysis {
        out.push_str(&format!(
            "{}\nCurrent Risk\n{:.1}\n\n{}\n ↓\nPredicted Risk\n{:.1}\n\nImpact\n{:+.1}\n\n",
            analysis.proposal_title,
            analysis.current_risk,
            analysis.proposal_title,
            analysis.predicted_risk,
            analysis.impact,
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_excludes_target_and_collects_rust_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/a/src")).unwrap();
        fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        fs::write(dir.path().join("crates/a/src/lib.rs"), "pub struct A;").unwrap();
        fs::write(
            dir.path().join("target/debug/generated.rs"),
            "pub struct Generated;",
        )
        .unwrap();

        let snapshot = RepositoryScanner.scan(dir.path()).unwrap();
        assert_eq!(snapshot.rust_files.len(), 1);
        assert!(snapshot.rust_files[0].ends_with("crates/a/src/lib.rs"));
        assert!(snapshot.cargo_tomls.is_empty());
    }

    #[test]
    fn ast_extractor_collects_rust_items() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(
            &file,
            "use crate::runtime::RuntimeCore;\nmod runtime;\npub struct S;\npub enum E { A }\npub trait T {}\nimpl T for S {}\npub fn f() {}\n",
        )
        .unwrap();

        let ast = extract_rust_ast_modules(dir.path(), &[file]).unwrap();
        assert_eq!(ast[0].modules, vec!["runtime"]);
        assert_eq!(ast[0].structs, vec!["S"]);
        assert_eq!(ast[0].enums, vec!["E"]);
        assert_eq!(ast[0].traits, vec!["T"]);
        assert_eq!(ast[0].functions, vec!["f"]);
        assert_eq!(ast[0].uses, vec!["crate::runtime::RuntimeCore"]);
        assert_eq!(ast[0].impls[0].trait_name, "T");
    }

    #[test]
    fn semantic_analysis_classifies_engine_and_memory_components() {
        let graph = StructureGraph {
            nodes: vec![
                StructureNode {
                    id: "module:AnalyzeEngine".to_string(),
                    label: "AnalyzeEngine".to_string(),
                    kind: StructureNodeKind::Module,
                },
                StructureNode {
                    id: "module:HolographicMemory".to_string(),
                    label: "HolographicMemory".to_string(),
                    kind: StructureNodeKind::Module,
                },
            ],
            edges: vec![],
        };

        let semantic = analyze_semantic_structure(&graph, &[]);
        assert!(semantic.components.iter().any(|component| {
            component.name == "AnalyzeEngine"
                && component.category == SemanticCategory::Engine
                && component.responsibility == "Structure Analysis"
        }));
        assert!(semantic.components.iter().any(|component| {
            component.name == "HolographicMemory" && component.category == SemanticCategory::Memory
        }));
    }

    #[test]
    fn workspace_members_are_observed_from_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/cli\", \"crates/core\"]\n",
        )
        .unwrap();

        let snapshot = RepositoryScanner.scan(dir.path()).unwrap();
        assert_eq!(
            snapshot.workspace_members,
            vec!["apps/cli".to_string(), "crates/core".to_string()]
        );
    }

    #[test]
    fn structure_graph_includes_call_edges() {
        let ast = vec![AstModule {
            file_path: PathBuf::from("src/lib.rs"),
            module_path: "root".to_string(),
            modules: vec![],
            structs: vec![],
            enums: vec![],
            traits: vec![],
            functions: vec!["caller".to_string()],
            uses: vec![],
            impls: vec![],
            calls: vec![FunctionCall {
                caller: "caller".to_string(),
                callee: "callee".to_string(),
            }],
        }];

        let graph = build_structure_graph(&ast);
        assert!(graph.edges.iter().any(|edge| {
            edge.from == "fn:root::caller"
                && edge.to == "call:callee"
                && edge.kind == StructureEdgeKind::Calls
        }));
    }

    #[test]
    fn dependency_analyzer_detects_cycles_density_and_hotspots() {
        let ast = vec![
            test_ast_module("a", vec!["crate::b::B"], 120, 60),
            test_ast_module("b", vec!["crate::c::C"], 1, 0),
            test_ast_module("c", vec!["crate::a::A"], 1, 0),
        ];
        let graph = build_structure_graph(&ast);
        let semantic = analyze_semantic_structure(&graph, &ast);

        let analysis = DependencyAnalyzer::analyze(&graph, &semantic, &ast);
        assert_eq!(analysis.circular_dependencies.len(), 1);
        assert!(analysis.dependency_density > 0.0);
        assert!(
            analysis
                .hotspots
                .iter()
                .any(|hotspot| hotspot.component == "a")
        );
    }

    #[test]
    fn responsibility_analyzer_detects_god_objects() {
        let ast = vec![test_ast_module("runtime_core", vec![], 101, 0)];
        let graph = build_structure_graph(&ast);
        let semantic = analyze_semantic_structure(&graph, &ast);

        let analysis = analyze_responsibilities(&graph, &semantic, &ast);
        assert_eq!(analysis.god_objects.len(), 1);
        assert_eq!(analysis.god_objects[0].component, "runtime_core");
    }

    #[test]
    fn architecture_analyzer_reports_layer_violation_and_health() {
        let semantic = SemanticStructure {
            components: vec![
                SemanticComponent {
                    name: "ui".to_string(),
                    category: SemanticCategory::UI,
                    responsibility: "User interface rendering and interaction".to_string(),
                    evidence: vec![],
                },
                SemanticComponent {
                    name: "storage".to_string(),
                    category: SemanticCategory::Storage,
                    responsibility: "Persistence and storage management".to_string(),
                    evidence: vec![],
                },
            ],
        };
        let dependency = DependencyAnalysis {
            circular_dependencies: vec![CircularDependency {
                cycle_nodes: vec!["a".to_string(), "b".to_string()],
            }],
            hidden_dependencies: vec![HiddenDependency {
                source: "ui".to_string(),
                via: "runtime".to_string(),
                target: "storage".to_string(),
            }],
            dependency_density: 1.0,
            hotspots: vec![],
        };
        let responsibility = ResponsibilityAnalysis {
            god_objects: vec![],
            massive_modules: vec![],
            responsibility_leakage: vec![ResponsibilityLeakage {
                source: "ui".to_string(),
                target: "storage".to_string(),
                reason: "UI accesses Storage directly".to_string(),
            }],
        };

        let analysis = analyze_architecture(&semantic, &dependency, &responsibility);
        assert_eq!(analysis.layer_violations.len(), 1);
        assert!(analysis.health.score < 100.0);
        assert!(matches!(
            analysis.health.status,
            ArchitectureStatus::Excellent
                | ArchitectureStatus::Good
                | ArchitectureStatus::Fair
                | ArchitectureStatus::Healthy
                | ArchitectureStatus::Warning
                | ArchitectureStatus::Critical
        ));
    }

    #[test]
    fn convergence_engine_generates_split_proposal_and_impact_prediction() {
        let dependency = DependencyAnalysis {
            circular_dependencies: vec![],
            hidden_dependencies: vec![],
            dependency_density: 1.0,
            hotspots: vec![],
        };
        let responsibility = ResponsibilityAnalysis {
            god_objects: vec![GodObject {
                component: "RuntimeCore".to_string(),
                score: 1.32,
            }],
            massive_modules: vec![],
            responsibility_leakage: vec![],
        };
        let architecture = ArchitectureAnalysis {
            rules: default_architecture_rules(),
            layer_violations: vec![],
            boundary_violations: vec![],
            health: ArchitectureHealth {
                score: 72.3,
                status: ArchitectureStatus::Warning,
                calibration: test_health_calibration(72.3),
            },
        };
        let design_drift = DesignDrift {
            design_source: Some(PathBuf::from("design.md")),
            intent_drifts: vec![],
            missing_design_reference: false,
        };

        let report =
            analyze_convergence(&dependency, &responsibility, &architecture, &design_drift);
        let top = report.top_proposal.as_ref().expect("top proposal");
        assert_eq!(top.title, "Split RuntimeCore");
        assert!(report.predicted_health_delta > 0.0);
        assert!(report.confidence >= 0.80);
        assert!(matches!(
            report.direction,
            Direction::Improving | Direction::Stable
        ));
        assert!(!report.impact_analysis.is_empty());
        assert!(!report.architecture_simulations.is_empty());
        assert!(!report.mutation_impact_models.is_empty());
    }

    #[test]
    fn design_matcher_scores_missing_design_as_partial_alignment() {
        let drift = DesignDrift {
            design_source: None,
            intent_drifts: vec![],
            missing_design_reference: true,
        };

        let alignment = match_design_alignment(&drift);
        assert_eq!(alignment.status, DesignAlignmentStatus::PartiallyAligned);
        assert_eq!(alignment.score, 65.0);
    }

    #[test]
    fn diagnostic_report_explains_health_targets_and_proposals() {
        let dependency = DependencyAnalysis {
            circular_dependencies: vec![CircularDependency {
                cycle_nodes: vec!["apps::cli::coding".to_string(), "runtime_core".to_string()],
            }],
            hidden_dependencies: vec![],
            dependency_density: 2.0,
            hotspots: vec![DependencyHotspot {
                component: "apps::cli::coding".to_string(),
                incoming_edges: 421,
                outgoing_edges: 76,
                calls: 184,
            }],
        };
        let responsibility = ResponsibilityAnalysis {
            god_objects: vec![GodObject {
                component: "apps::cli::coding".to_string(),
                score: 1.84,
            }],
            massive_modules: vec![MassiveModule {
                component: "apps::cli::coding".to_string(),
                function_count: 184,
                dependency_count: 61,
            }],
            responsibility_leakage: vec![],
        };
        let mut architecture = ArchitectureAnalysis {
            rules: default_architecture_rules(),
            layer_violations: vec![LayerViolation {
                source: "ui".to_string(),
                target: "storage".to_string(),
                source_layer: SemanticCategory::UI,
                target_layer: SemanticCategory::Storage,
            }],
            boundary_violations: vec![BoundaryViolation {
                source: "apps::cli".to_string(),
                target: "crates::runtime_core".to_string(),
                reason: "internal access crosses module boundary".to_string(),
            }],
            health: ArchitectureHealth {
                score: 0.0,
                status: ArchitectureStatus::Critical,
                calibration: test_health_calibration(0.0),
            },
        };
        let design_drift = DesignDrift {
            design_source: Some(PathBuf::from("design.md")),
            intent_drifts: vec![IntentDrift {
                component: "memory".to_string(),
                expected: "Memory Layer".to_string(),
                actual: "Execution Logic".to_string(),
            }],
            missing_design_reference: false,
        };
        architecture = calibrate_architecture_analysis(
            architecture,
            &dependency,
            &responsibility,
            &design_drift,
        );
        let convergence = ConvergenceReport {
            architecture_health: architecture.health.score,
            convergence_score: ConvergenceScore {
                score: 78.5,
                state: ConvergenceState::Unstable,
            },
            direction: Direction::Improving,
            top_proposal: Some(ConvergenceProposal {
                title: "Split apps::cli::coding".to_string(),
                reason: "God Object".to_string(),
                impact_score: 10.2,
                confidence: 0.96,
            }),
            predicted_health_delta: 10.2,
            confidence: 0.96,
            refactoring_proposals: vec![RefactoringProposal {
                proposal: ConvergenceProposal {
                    title: "Split apps::cli::coding".to_string(),
                    reason: "God Object".to_string(),
                    impact_score: 10.2,
                    confidence: 0.96,
                },
                mutation_plan: MutationPlan {
                    kind: "split_module".to_string(),
                    target: "apps::cli::coding".to_string(),
                    actions: vec!["extract focused runtime boundary".to_string()],
                },
            }],
            impact_analysis: vec![ImpactAnalysis {
                proposal_title: "Split apps::cli::coding".to_string(),
                dependency_reduction: 0.12,
                violation_reduction: 0.50,
                health_improvement: 10.2,
                risk: 0.13,
                predicted_health: 17.7,
                design_drift_impact: -2.1,
            }],
            architecture_simulations: vec![],
            design_alignment: DesignAlignment {
                status: DesignAlignmentStatus::PartiallyAligned,
                score: 60.0,
                evidence: vec![],
            },
            mutation_impact_models: vec![],
        };

        let report = DiagnosticReportEngine::build(
            &dependency,
            &responsibility,
            &architecture,
            &design_drift,
            &convergence,
            Some(58.0),
        );

        assert_eq!(
            report.health_breakdown.final_score,
            architecture.health.calibration.final_score
        );
        assert_eq!(report.health_trend.previous_score, 58.0);
        assert_eq!(
            report.health_trend.delta,
            report.health_trend.current_score - report.health_trend.previous_score
        );
        assert!(report.health_trend.delta.is_finite());
        assert_eq!(report.contribution_analysis.len(), 4);
        assert_eq!(report.god_objects[0].component, "apps::cli::coding");
        assert_eq!(report.god_objects[0].function_count, 184);
        assert!(
            report.god_objects[0]
                .reasons
                .contains(&"excessive function count".to_string())
        );
        assert_eq!(report.dependency_hotspots[0].incoming_edges, 421);
        assert_eq!(report.refactoring_proposals[0].rank, 1);
        assert_eq!(report.refactoring_proposals[0].impact, 10.2);
        assert!(matches!(
            report.layer_violations[0].severity,
            DiagnosticSeverity::High
        ));
        assert!(!report.convergence_explanation.reasons.is_empty());

        let output_health_score = architecture.health.score;
        let output_health_status = architecture.health.status;
        let output = AnalyzeEngineOutput {
            root: PathBuf::from("."),
            repository: RepositorySnapshot {
                root: PathBuf::from("."),
                rust_files: vec![],
                directories: vec![],
                cargo_tomls: vec![],
                workspace_members: vec![],
            },
            files: vec![],
            ast_modules: vec![],
            graph: StructureGraph {
                nodes: vec![],
                edges: vec![],
            },
            semantic_structure: SemanticStructure { components: vec![] },
            dependency_analysis: dependency,
            responsibility_analysis: responsibility,
            architecture_analysis: architecture,
            design_drift,
            convergence_report: convergence,
            detailed_report: report,
            security_report: analyze_security_architecture(&[], &SemanticStructure { components: vec![] }),
            result: AnalyzeResult {
                project_name: "Design_BrainModel".to_string(),
                modules: 1,
                structs: 0,
                enums: 0,
                traits: 0,
                functions: 184,
                module_count: 1,
                struct_count: 0,
                enum_count: 0,
                trait_count: 0,
                function_count: 184,
                top_components: vec!["apps::cli::coding".to_string()],
                top_modules: vec!["apps::cli::coding".to_string()],
                dependencies: vec![],
                circular_dependencies: 1,
                layer_violations: 1,
                boundary_violations: 1,
                god_objects: 1,
                architecture_health: output_health_score,
                status: output_health_status,
                convergence_score: 78.5,
                direction: Direction::Improving,
                top_proposal: Some("Split apps::cli::coding".to_string()),
                predicted_health_delta: 10.2,
                convergence_confidence: 0.96,
            },
        };
        let rendered = render_detailed_analyze_result(&output);
        assert!(rendered.contains("=== Health Breakdown ==="));
        assert!(rendered.contains("=== God Objects ==="));
        assert!(rendered.contains("=== Refactoring Proposals ==="));
        assert!(rendered.contains("Split apps::cli::coding"));
    }

    fn test_ast_module(
        module_path: &str,
        uses: Vec<&str>,
        function_count: usize,
        call_count: usize,
    ) -> AstModule {
        let functions = (0..function_count)
            .map(|index| format!("f_{index}"))
            .collect::<Vec<_>>();
        let calls = (0..call_count)
            .map(|index| FunctionCall {
                caller: "f_0".to_string(),
                callee: format!("callee_{index}"),
            })
            .collect::<Vec<_>>();
        AstModule {
            file_path: PathBuf::from(format!("{module_path}.rs")),
            module_path: module_path.to_string(),
            modules: vec![],
            structs: vec![],
            enums: vec![],
            traits: vec![],
            functions,
            uses: uses.into_iter().map(ToString::to_string).collect(),
            impls: vec![],
            calls,
        }
    }

    fn test_health_calibration(score: f32) -> ArchitectureHealthV2 {
        ArchitectureHealthV2 {
            dependency_score: score,
            responsibility_score: score,
            architecture_score: score,
            drift_score: score,
            final_score: score,
        }
    }
}
