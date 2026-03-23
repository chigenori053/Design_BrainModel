use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use integration_layer::{
    CycleReport, DiagnosticAnalysis, Issue, LayerModel, LayerViolation, Pattern, RoleAssignment,
    SemanticLayer, Severity, StructuralAnalysis, SystemInput, SystemOutput, diagnostic_analysis,
    structural_analysis, to_relations, to_system_output, validate_round_trip_design,
};
use serde::Serialize;

use crate::coding::{CodeChangeSet, CodingExecutionResult};
use crate::dbm::{DBMClient, ProjectAnalysisResult};
use crate::runner::{MemoryUsage, OutputMeta, SandboxMode};

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
    pub dependencies: Vec<AnalysisDependency>,
    pub todo_files: usize,
    pub cycles: CycleReport,
    pub layers: LayerModel,
    pub violations: Vec<LayerViolation>,
    pub roles: Vec<RoleAssignment>,
    pub semantic_layers: Vec<SemanticLayer>,
    pub data_flow: Vec<DataFlowEdgeReport>,
    pub issues: Vec<Issue>,
    pub summary: AnalysisSummary,
    pub next_action: String,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisDependency {
    pub from: String,
    pub to: String,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct DataFlowEdgeReport {
    pub from: String,
    pub to: String,
    pub weight: f32,
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

pub fn analyze_path(path: &Path) -> Result<AnalysisReport, String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("path is not a directory: {}", path.display()));
    }
    let client = DBMClient::new();
    let project = client
        .analyze_project(&path.display().to_string())
        .map_err(|err| format!("project analysis failed: {err}"))?;
    build_analysis_report(path, &project)
}

pub fn build_design_report(path: &Path) -> Result<DesignReport, String> {
    let analysis = analyze_path(path)?;
    let design_graph = design_graph_from_analysis(&analysis);
    let structural = structural_analysis(&design_graph);
    let analysis = enrich_analysis_report(analysis, diagnostic_analysis(&design_graph));
    Ok(match to_system_output(to_relations(SystemInput::Design(design_graph))) {
        SystemOutput::Design(graph) => design_report_from_graph(&analysis, &graph, &structural),
        _ => design_from_analysis(&analysis),
    })
}

pub fn build_validation_report(path: &Path) -> Result<ValidationReport, String> {
    let analysis = analyze_path(path)?;
    let design_graph = design_graph_from_analysis(&analysis);
    let structural = diagnostic_analysis(&design_graph);
    let mut report = validate_from_analysis(&analysis, &structural);
    let integration = validate_round_trip_design(&design_graph);
    if !integration.is_valid {
        report.valid = false;
        report
            .issues
            .extend(integration.issues.into_iter().map(|issue| issue.message));
    }
    Ok(report)
}

pub fn analysis_to_system_input(analysis: &AnalysisReport) -> SystemInput {
    let mut entities = analysis
        .modules
        .iter()
        .map(|module| module.name.clone())
        .collect::<Vec<_>>();
    entities.push("architecture".to_string());
    entities.push("change_impact".to_string());
    entities.sort();
    entities.dedup();
    SystemInput::Analyze(integration_layer::AnalysisInput {
        system_id: analysis.root.clone(),
        entities,
        has_cycle: false,
    })
}

pub fn design_graph_from_analysis(analysis: &AnalysisReport) -> unified_design_ir::DesignGraph {
    let mut builder = unified_design_ir::DesignGraphBuilder::new();
    let mut node_names = analysis
        .modules
        .iter()
        .map(|module| module.name.clone())
        .collect::<BTreeSet<_>>();
    for dependency in &analysis.dependencies {
        node_names.insert(dependency.from.clone());
        node_names.insert(dependency.to.clone());
    }
    for node_name in node_names {
        builder = builder.add_node(unified_design_ir::DesignNode {
            id: unified_design_ir::DesignNodeId(node_name.clone()),
            name: node_name.clone(),
            kind: if node_name.contains("api") {
                unified_design_ir::DesignNodeKind::API
            } else if node_name.contains("db") {
                unified_design_ir::DesignNodeKind::Database
            } else {
                unified_design_ir::DesignNodeKind::Module
            },
            metadata: unified_design_ir::DesignMetadata::default(),
        });
    }
    for dependency in &analysis.dependencies {
        builder = builder.add_edge(unified_design_ir::DesignEdge {
            source: unified_design_ir::DesignNodeId(dependency.from.clone()),
            target: unified_design_ir::DesignNodeId(dependency.to.clone()),
            relation: unified_design_ir::DesignRelation::DependsOn,
        });
    }
    builder.build()
}

pub fn enrich_analysis_report(
    mut analysis: AnalysisReport,
    structural: DiagnosticAnalysis,
) -> AnalysisReport {
    let issues = structural.issues.clone();
    let summary = summarize_issues(&issues);
    analysis.cycles = structural.cycle_report;
    analysis.layers = structural.layer_model;
    analysis.violations = structural.violations;
    analysis.roles = structural.semantic.roles;
    analysis.semantic_layers = structural.semantic.layers;
    analysis.data_flow = structural
        .data_flow
        .flows
        .into_iter()
        .map(|flow| DataFlowEdgeReport {
            from: flow.from,
            to: flow.to,
            weight: f32::from(flow.weight_milli) / 1000.0,
        })
        .collect();
    analysis.issues = issues;
    analysis.summary = summary;
    analysis.next_action = format!("cli refactor {}", analysis.root);
    analysis
}

pub fn validate_from_analysis(
    analysis: &AnalysisReport,
    structural: &DiagnosticAnalysis,
) -> ValidationReport {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    if analysis.source_files == 0 {
        issues.push("no source files detected".to_string());
    }
    if analysis.manifests.is_empty() {
        warnings.push("no manifest file detected".to_string());
    }
    if !analysis
        .architecture_hints
        .iter()
        .any(|hint| hint == "has-tests")
    {
        warnings.push("test directory not detected".to_string());
    }
    issues.extend(
        structural
            .integrity
            .issues
            .iter()
            .map(|issue| issue.message.clone()),
    );
    warnings.extend(
        structural
            .violations
            .iter()
            .map(|violation| format!("LayerViolation: {} -> {}", violation.from, violation.to)),
    );
    issues.sort();
    issues.dedup();
    warnings.sort();
    warnings.dedup();

    ValidationReport {
        root: analysis.root.clone(),
        valid: issues.is_empty(),
        issues,
        warnings,
        cycles: structural.cycle_report.clone(),
        layers: structural.layer_model.clone(),
        violations: structural.violations.clone(),
        patterns: structural.semantic.patterns.clone(),
    }
}

pub fn path_contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn summarize_issues(issues: &[Issue]) -> AnalysisSummary {
    let mut summary = AnalysisSummary::default();
    for issue in issues {
        match issue.severity {
            Severity::Critical => summary.critical += 1,
            Severity::High => summary.high += 1,
            Severity::Medium => summary.medium += 1,
            _ => {}
        }
    }
    summary
}

fn design_from_analysis(analysis: &AnalysisReport) -> DesignReport {
    let inferred_style = if analysis
        .architecture_hints
        .iter()
        .any(|hint| hint == "workspace-layout")
    {
        "workspace"
    } else if analysis.languages.contains_key("Rust") {
        "service"
    } else if analysis.languages.contains_key("Python") {
        "application"
    } else {
        "generic"
    };

    let mut components = Vec::new();
    if analysis.top_level_entries.iter().any(|entry| entry == "src") {
        components.push("src".to_string());
    }
    if analysis.top_level_entries.iter().any(|entry| entry == "tests") {
        components.push("tests".to_string());
    }
    if analysis.top_level_entries.iter().any(|entry| entry == "crates") {
        components.push("crates".to_string());
    }
    if components.is_empty() {
        components.push("root".to_string());
    }

    let mut design_units = analysis
        .manifests
        .iter()
        .map(|manifest| format!("manifest:{manifest}"))
        .collect::<Vec<_>>();
    if design_units.is_empty() {
        design_units.push("source-scan".to_string());
    }

    DesignReport {
        root: analysis.root.clone(),
        inferred_style: inferred_style.to_string(),
        components,
        design_units,
        recommended_next_steps: vec![
            "cli analyze <path>".to_string(),
            "cli validate <path> --json".to_string(),
        ],
        cycles: analysis.cycles.clone(),
        layers: analysis.layers.clone(),
        violations: analysis.violations.clone(),
        roles: analysis.roles.clone(),
        semantic_layers: analysis.semantic_layers.clone(),
        patterns: Vec::new(),
        suggestions: Vec::new(),
    }
}

fn design_report_from_graph(
    analysis: &AnalysisReport,
    graph: &unified_design_ir::DesignGraph,
    structural: &StructuralAnalysis,
) -> DesignReport {
    let mut report = design_from_analysis(analysis);
    report.components = graph.nodes().iter().map(|node| node.name.clone()).collect();
    report.components.sort();
    report.components.dedup();
    report.design_units = graph
        .edges()
        .iter()
        .map(|edge| format!("{}->{:?}->{}", edge.source.0, edge.relation, edge.target.0))
        .collect();
    if report.design_units.is_empty() {
        report.design_units.push("source-scan".to_string());
    }
    report.cycles = structural.cycle_report.clone();
    report.layers = structural.layer_model.clone();
    report.violations = structural.violations.clone();
    report.roles = structural.semantic.roles.clone();
    report.semantic_layers = structural.semantic.layers.clone();
    report.patterns = structural.semantic.patterns.clone();
    report.suggestions = structural.semantic.suggestions.clone();
    if structural.cycle_report.has_cycle {
        report
            .recommended_next_steps
            .push("Break cycle between dependent modules".to_string());
    }
    report
}

fn build_analysis_report(root: &Path, project: &ProjectAnalysisResult) -> Result<AnalysisReport, String> {
    let mut languages = BTreeMap::new();
    let mut todo_files = 0usize;
    for file in &project.files {
        *languages
            .entry(file.language.as_str().to_string())
            .or_insert(0) += 1;
        if !file.todos.is_empty() {
            todo_files += 1;
        }
    }

    let manifests = collect_manifests(root)?;
    let top_level_entries = collect_top_level_entries(root)?;
    let architecture_hints = infer_architecture_hints(project, &manifests, &top_level_entries);

    Ok(AnalysisReport {
        root: root.display().to_string(),
        total_files: project.summary.total_files,
        source_files: project.files.len(),
        avg_complexity: project.summary.avg_complexity.as_str().to_string(),
        manifests,
        languages,
        top_level_entries,
        architecture_hints,
        modules: project
            .modules
            .iter()
            .map(|module| AnalysisModule {
                name: module.name.clone(),
                file_count: module.files.len(),
            })
            .collect(),
        dependencies: project
            .dependencies
            .iter()
            .map(|edge| AnalysisDependency {
                from: edge.from.clone(),
                to: edge.to.clone(),
            })
            .collect(),
        todo_files,
        cycles: CycleReport {
            has_cycle: false,
            cycles: Vec::new(),
        },
        layers: LayerModel { layers: Vec::new() },
        violations: Vec::new(),
        roles: Vec::new(),
        semantic_layers: Vec::new(),
        data_flow: Vec::new(),
        issues: Vec::new(),
        summary: AnalysisSummary::default(),
        next_action: String::new(),
    })
}

fn collect_manifests(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect_paths(root, &mut files)?;
    Ok(files
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| matches!(name, "Cargo.toml" | "pyproject.toml" | "package.json"))
                .unwrap_or(false)
        })
        .map(|path| relativize(root, path))
        .collect())
}

fn collect_top_level_entries(root: &Path) -> Result<Vec<String>, String> {
    let mut entries = fs::read_dir(root)
        .map_err(|err| format!("failed to read {}: {err}", root.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn infer_architecture_hints(
    project: &ProjectAnalysisResult,
    manifests: &[String],
    top_level_entries: &[String],
) -> Vec<String> {
    let mut architecture_hints = BTreeSet::new();
    if manifests.iter().any(|path| path.ends_with("Cargo.toml")) {
        architecture_hints.insert("rust-project".to_string());
    }
    if project
        .files
        .iter()
        .any(|file| file.path.starts_with("src/") || file.path.contains("/src/"))
    {
        architecture_hints.insert("layered-source-layout".to_string());
    }
    if project
        .files
        .iter()
        .any(|file| file.path.starts_with("tests/") || file.path.contains("/tests/"))
        || top_level_entries.iter().any(|entry| entry == "tests")
    {
        architecture_hints.insert("has-tests".to_string());
    }
    if top_level_entries.iter().any(|entry| entry == "crates") {
        architecture_hints.insert("workspace-layout".to_string());
    }
    if project.modules.len() > 1 {
        architecture_hints.insert("multi-module".to_string());
    }
    if !project.dependencies.is_empty() {
        architecture_hints.insert("dependency-graph-available".to_string());
    }
    architecture_hints.into_iter().collect()
}

fn collect_paths(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read dir entry: {err}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), ".git" | "target" | "node_modules") {
            continue;
        }
        if path.is_dir() {
            collect_paths(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn relativize(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
