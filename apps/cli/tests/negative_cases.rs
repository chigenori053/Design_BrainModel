use std::fs;
use std::path::{Path, PathBuf};

use design_cli::service::{analyze_path, design_graph_from_analysis, enrich_analysis_report};
use integration_layer::IssueType;

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn analyze_fixture(name: &str) -> design_cli::service::AnalysisReport {
    let path = fixture_root(name);
    let analysis = analyze_path(&path).expect("fixture analysis");
    let graph = design_graph_from_analysis(&analysis);
    enrich_analysis_report(analysis, integration_layer::diagnostic_analysis(&graph))
}

fn dto_types(path: &Path) -> Vec<String> {
    let content = fs::read_to_string(path).expect("dto source");
    let mut in_struct = false;
    let mut types = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub struct ") {
            in_struct = true;
            continue;
        }
        if in_struct && trimmed.starts_with('}') {
            in_struct = false;
            continue;
        }
        if !in_struct || !trimmed.starts_with("pub ") {
            continue;
        }
        if let Some((_, ty)) = trimmed.split_once(':') {
            types.push(ty.trim().trim_end_matches(',').to_string());
        }
    }

    types
}

#[test]
fn test_detect_cycle() {
    let result = analyze_fixture("architecture_cycle");
    assert!(result.cycles.has_cycle);
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueType::Cycle)
    );
}

#[test]
fn test_detect_layer_violation() {
    let result = analyze_fixture("architecture_layer_violation");
    assert!(!result.violations.is_empty());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueType::LayerViolation)
    );
}

#[test]
fn test_detect_role_mismatch() {
    let result = analyze_fixture("architecture_role_mismatch");
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueType::RoleMismatch)
    );
}

#[test]
fn test_detect_dto_leak() {
    let dto_path = fixture_root("architecture_dto_leak")
        .join("src")
        .join("service")
        .join("dto.rs");
    let dto_source = fs::read_to_string(&dto_path).expect("dto source");
    let types = dto_types(&dto_path);
    assert!(
        dto_source.contains("use crate::world::WorldState;")
            && types.iter().any(|ty| ty == "WorldState"),
        "expected DTO leak in fixture, got {types:?}"
    );

    let result = analyze_fixture("architecture_dto_leak");
    assert!(
        result
            .code_issues
            .iter()
            .any(|issue| issue.category == "BoundaryLeak" && issue.file.ends_with("dto.rs")),
        "expected BoundaryLeak code issue, got {:?}",
        result.code_issues
    );
}
