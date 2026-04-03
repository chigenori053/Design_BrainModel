use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::coding::{CodingOptions, execute_code_change_set, generate_code_change_set};
use crate::dbm::ProjectAnalysisResult;
use crate::source_index::ModuleSourceIndex;
use crate::world;
use integration_layer::{
    CycleReport, DiagnosticAnalysis, Issue, LayerModel, Severity, StructuralAnalysis, SystemInput,
    SystemOutput, diagnostic_analysis, structural_analysis, to_relations, to_system_output,
    validate_round_trip_design,
};

#[path = "service/dto.rs"]
pub mod dto;
#[path = "service/reasoning.rs"]
pub mod reasoning;

pub use dto::*;
pub use reasoning::{
    DeterministicIssueAggregator, IssueAggregator, generate_plan, infer_root_cause,
};

pub fn analyze_path(path: &Path) -> Result<AnalysisReport, String> {
    let project = world::analyze_project(path)?;
    build_analysis_report(path, &project)
}

pub fn build_design_report(path: &Path) -> Result<DesignReport, String> {
    let analysis = analyze_path(path)?;
    let design_graph = design_graph_from_analysis(&analysis);
    let structural = structural_analysis(&design_graph);
    let analysis = enrich_analysis_report(analysis, diagnostic_analysis(&design_graph));
    Ok(
        match to_system_output(to_relations(SystemInput::Design(design_graph))) {
            SystemOutput::Design(graph) => design_report_from_graph(&analysis, &graph, &structural),
            _ => design_from_analysis(&analysis),
        },
    )
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

pub fn build_refactoring_report(
    path: &Path,
    dry_run: bool,
    options: &CodingOptions,
) -> Result<CodingReport, String> {
    let analysis = analyze_path(path)?;
    let design_graph = design_graph_from_analysis(&analysis);
    let structural = structural_analysis(&design_graph);
    let changes = generate_code_change_set(path, &structural.code_patches)?;
    let execution = execute_code_change_set(path, &changes, options)?;
    Ok(CodingReport {
        root: path.display().to_string(),
        dry_run,
        execution,
        patches: structural.code_patches,
        changes,
    })
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
    let root_cause = if issues.is_empty() {
        None
    } else {
        Some(infer_root_cause(&issues))
    };
    let refactor_plan = root_cause.as_ref().map(generate_plan).unwrap_or_default();
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
    analysis.next_action = format!("cli refactoring {}", analysis.root);
    analysis.root_cause = root_cause;
    analysis.refactor_plan = refactor_plan;
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
    world::path_contains_parent_component(path)
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
    if analysis
        .top_level_entries
        .iter()
        .any(|entry| entry == "src")
    {
        components.push("src".to_string());
    }
    if analysis
        .top_level_entries
        .iter()
        .any(|entry| entry == "tests")
    {
        components.push("tests".to_string());
    }
    if analysis
        .top_level_entries
        .iter()
        .any(|entry| entry == "crates")
    {
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

fn build_analysis_report(
    root: &Path,
    project: &ProjectAnalysisResult,
) -> Result<AnalysisReport, String> {
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
    let code_issues = analyze_code_issues(root, project);

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
                source_path: module
                    .files
                    .iter()
                    .min()
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect(),
        graph_nodes: build_graph_nodes(root, project),
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
        code_issues,
        summary: AnalysisSummary::default(),
        next_action: String::new(),
        root_cause: None,
        refactor_plan: Vec::new(),
    })
}

fn build_graph_nodes(root: &Path, project: &ProjectAnalysisResult) -> Vec<ModuleNode> {
    let mut graph_nodes = BTreeMap::<String, ModuleNode>::new();
    let index = ModuleSourceIndex::build(root).unwrap_or_default();

    for (qualified_id, source_path) in index.all_bindings() {
        let logical_name = qualified_id
            .module_path
            .split("::")
            .last()
            .unwrap_or(&qualified_id.module_path)
            .to_string();
        graph_nodes.insert(
            logical_name.clone(),
            ModuleNode {
                qualified_id,
                logical_name,
                source_path: Some(source_path),
            },
        );
    }

    let mut logical_names = BTreeSet::new();
    for module in &project.modules {
        logical_names.insert(module.name.clone());
    }
    for dependency in &project.dependencies {
        logical_names.insert(dependency.from.clone());
        logical_names.insert(dependency.to.clone());
    }

    logical_names
        .into_iter()
        .for_each(|logical_name| {
            if graph_nodes.contains_key(&logical_name) {
                return;
            }
            if let Some((qualified_id, source_path)) = index.bind_graph_node(&logical_name) {
                graph_nodes.insert(logical_name.clone(), ModuleNode {
                    qualified_id,
                    logical_name,
                    source_path: Some(source_path),
                });
            } else {
                graph_nodes.insert(logical_name.clone(), ModuleNode {
                    qualified_id: crate::source_index::QualifiedModuleId {
                        crate_name: root
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("unknown")
                            .replace('-', "_"),
                        module_path: logical_name.replace('-', "_"),
                    },
                    logical_name,
                    source_path: None,
                });
            }
        });
    graph_nodes.into_values().collect()
}

fn analyze_code_issues(root: &Path, project: &ProjectAnalysisResult) -> Vec<CodeIssue> {
    let mut issues = Vec::new();

    for file in &project.files {
        let path = root.join(&file.path);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        let lower_path = file.path.to_ascii_lowercase();

        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let line_no = index + 1;

            if is_dto_file(&lower_path) && is_boundary_leak_import(trimmed) {
                issues.push(CodeIssue {
                    severity: "high".to_string(),
                    category: "BoundaryLeak".to_string(),
                    file: file.path.clone(),
                    line: line_no,
                    title: "DTO depends on internal module".to_string(),
                    issue: format!(
                        "DTO file imports `{}`. Transport objects should stay isolated from internal runtime or world modules.",
                        trimmed
                    ),
                    snippet: snippet_for_line(&lines, index),
                });
            }

            if is_wildcard_import(trimmed) {
                issues.push(CodeIssue {
                    severity: "medium".to_string(),
                    category: "WildcardImport".to_string(),
                    file: file.path.clone(),
                    line: line_no,
                    title: "Wildcard import obscures dependency surface".to_string(),
                    issue: "Wildcard import makes the dependency boundary implicit and increases the chance of accidental coupling.".to_string(),
                    snippet: snippet_for_line(&lines, index),
                });
            }

            if is_deep_relative_import(trimmed) {
                issues.push(CodeIssue {
                    severity: "medium".to_string(),
                    category: "DeepRelativeImport".to_string(),
                    file: file.path.clone(),
                    line: line_no,
                    title: "Deep relative import crosses local boundaries".to_string(),
                    issue: "Import walks up multiple parent scopes. That usually signals a brittle module boundary or a misplaced file.".to_string(),
                    snippet: snippet_for_line(&lines, index),
                });
            }

            if trimmed.contains("TODO") || trimmed.contains("FIXME") {
                issues.push(CodeIssue {
                    severity: "low".to_string(),
                    category: "DeferredWork".to_string(),
                    file: file.path.clone(),
                    line: line_no,
                    title: "Deferred work marker remains in code".to_string(),
                    issue: "TODO/FIXME marker is still present in source. If it represents a real design gap, it should be tracked outside the implementation or resolved.".to_string(),
                    snippet: snippet_for_line(&lines, index),
                });
            }
        }
    }

    issues.sort_by(|lhs, rhs| {
        severity_rank(&lhs.severity)
            .cmp(&severity_rank(&rhs.severity))
            .then(lhs.file.cmp(&rhs.file))
            .then(lhs.line.cmp(&rhs.line))
            .then(lhs.category.cmp(&rhs.category))
    });
    issues
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn is_dto_file(path: &str) -> bool {
    path.ends_with("/dto.rs")
        || path.ends_with("\\dto.rs")
        || path.ends_with("/dto.ts")
        || path.ends_with("\\dto.ts")
        || path.ends_with("/dto.py")
        || path.ends_with("\\dto.py")
        || path.ends_with("_dto.rs")
        || path.ends_with("_dto.ts")
        || path.ends_with("_dto.py")
}

fn is_boundary_leak_import(trimmed: &str) -> bool {
    const INTERNAL_MODULES: [&str; 6] = [
        "crate::world",
        "crate::app",
        "crate::renderer",
        "crate::debug",
        "crate::loop",
        "crate::ui",
    ];

    (trimmed.starts_with("use ") || trimmed.starts_with("import ") || trimmed.starts_with("from "))
        && INTERNAL_MODULES
            .iter()
            .any(|module| trimmed.contains(module))
}

fn is_wildcard_import(trimmed: &str) -> bool {
    trimmed.contains("::*")
        || trimmed.contains(" import *")
        || (trimmed.starts_with("from ") && trimmed.ends_with(" import *"))
}

fn is_deep_relative_import(trimmed: &str) -> bool {
    trimmed.contains("super::super")
        || trimmed.contains("super::super::")
        || trimmed.contains("../../")
        || trimmed.contains("../..")
}

fn snippet_for_line(lines: &[&str], line_index: usize) -> String {
    let start = line_index.saturating_sub(1);
    let end = usize::min(line_index + 2, lines.len());
    lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, line)| format!("{:>4} | {}", start + offset + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
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
