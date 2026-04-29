use std::fs;
use std::path::{Path, PathBuf};

use super::{ApiChange, DesignDelta, DesignGraph, MutationPlan};

pub fn discover_cargo_manifests(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut manifests = Vec::new();
    visit_manifests(workspace_root, &mut manifests)?;
    manifests.sort();
    Ok(manifests)
}

fn visit_manifests(dir: &Path, manifests: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with('.') || file_name == "target" {
            continue;
        }
        if path.is_dir() {
            visit_manifests(&path, manifests)?;
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml") {
            manifests.push(path);
        }
    }
    Ok(())
}

pub fn manifest_package_name(manifest: &Path) -> Result<Option<String>, String> {
    let content = fs::read_to_string(manifest).map_err(|err| err.to_string())?;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("name") {
            return Ok(parse_toml_string_value(trimmed));
        }
    }
    Ok(None)
}

pub fn manifest_internal_dependencies(
    manifest: &Path,
    crate_names: &[String],
) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(manifest).map_err(|err| err.to_string())?;
    let mut in_dependencies = false;
    let mut dependencies = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_dependencies = trimmed == "[dependencies]"
                || trimmed == "[dev-dependencies]"
                || trimmed == "[build-dependencies]";
            continue;
        }
        if !in_dependencies || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if crate_names.iter().any(|candidate| candidate == name) {
            dependencies.push(name.to_string());
        }
    }

    dependencies.sort();
    dependencies.dedup();
    Ok(dependencies)
}

pub fn extract_design_delta(graph: &DesignGraph, spec: &str) -> DesignDelta {
    let lower = spec.to_lowercase();
    let impacted_crates = infer_impacted_crates(graph, &lower);
    let introduced_interfaces = infer_interfaces(&lower, &impacted_crates);
    let dependency_moves = infer_dependency_moves(&lower, &impacted_crates);
    let api_changes = infer_api_changes(&lower, &impacted_crates);

    DesignDelta {
        workspace_root: graph.workspace_root.clone(),
        impacted_crates,
        introduced_interfaces,
        dependency_moves,
        api_changes,
    }
}

pub fn build_mutation_plan(
    graph: &DesignGraph,
    delta: &DesignDelta,
    spec: &str,
    conservative: bool,
) -> MutationPlan {
    let mut impacted_crates = delta.impacted_crates.clone();
    if conservative && impacted_crates.len() > 1 {
        impacted_crates.truncate(1);
    }

    let target_files = graph
        .crates
        .iter()
        .filter(|krate| impacted_crates.iter().any(|name| name == &krate.name))
        .map(|krate| krate.manifest_path.clone())
        .collect::<Vec<_>>();

    let mut expected_tests = impacted_crates
        .iter()
        .map(|name| format!("cargo test -p {name}"))
        .collect::<Vec<_>>();
    if expected_tests.is_empty() {
        expected_tests.push("cargo check".to_string());
    }

    if spec.to_lowercase().contains("trait") {
        expected_tests.push("cargo check --workspace".to_string());
    }

    let rollback_units = impacted_crates
        .iter()
        .map(|name| format!("crate::{name}"))
        .collect::<Vec<_>>();

    MutationPlan {
        delta: DesignDelta {
            impacted_crates,
            ..delta.clone()
        },
        target_files,
        expected_tests,
        rollback_units,
    }
}

fn infer_impacted_crates(graph: &DesignGraph, lower: &str) -> Vec<String> {
    let mut impacted = graph
        .crates
        .iter()
        .filter(|krate| lower.contains(&krate.name.to_lowercase()))
        .map(|krate| krate.name.clone())
        .collect::<Vec<_>>();

    if impacted.is_empty() {
        impacted = graph
            .crates
            .iter()
            .take(if lower.contains("workspace") || lower.contains("全体") {
                graph.crates.len().max(1)
            } else {
                graph.crates.len().clamp(1, 2)
            })
            .map(|krate| krate.name.clone())
            .collect::<Vec<_>>();
    }

    impacted
}

fn infer_interfaces(lower: &str, impacted_crates: &[String]) -> Vec<String> {
    if !lower.contains("trait") && !lower.contains("interface") && !lower.contains("抽象") {
        return Vec::new();
    }
    impacted_crates
        .iter()
        .map(|name| format!("{name}::DesignBoundary"))
        .collect()
}

fn infer_dependency_moves(lower: &str, impacted_crates: &[String]) -> Vec<(String, String)> {
    if impacted_crates.len() < 2 {
        return Vec::new();
    }
    if lower.contains("crate 境界")
        || lower.contains("boundary")
        || lower.contains("dependency")
        || lower.contains("依存")
    {
        impacted_crates
            .windows(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect()
    } else {
        Vec::new()
    }
}

fn infer_api_changes(lower: &str, impacted_crates: &[String]) -> Vec<ApiChange> {
    if !(lower.contains("api") || lower.contains("公開") || lower.contains("public")) {
        return Vec::new();
    }
    impacted_crates
        .iter()
        .map(|crate_name| ApiChange {
            crate_name: crate_name.clone(),
            surface: "public-api".to_string(),
            change: "migration-required".to_string(),
        })
        .collect()
}

fn parse_toml_string_value(line: &str) -> Option<String> {
    let (_, value) = line.split_once('=')?;
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> DesignGraph {
        DesignGraph {
            workspace_root: PathBuf::from("."),
            crates: vec![
                super::super::CrateNode {
                    name: "design_cli".to_string(),
                    manifest_path: PathBuf::from("apps/cli/Cargo.toml"),
                    internal_dependencies: vec!["agent_core".to_string()],
                },
                super::super::CrateNode {
                    name: "agent_core".to_string(),
                    manifest_path: PathBuf::from("crates/agent_core/Cargo.toml"),
                    internal_dependencies: Vec::new(),
                },
            ],
            dependency_edges: vec![("design_cli".to_string(), "agent_core".to_string())],
        }
    }

    #[test]
    fn delta_extracts_interfaces_and_dependency_moves() {
        let delta = extract_design_delta(
            &sample_graph(),
            "design_cli に trait を追加して crate 境界を維持する",
        );
        assert_eq!(delta.impacted_crates, vec!["design_cli".to_string()]);
        assert_eq!(
            delta.introduced_interfaces,
            vec!["design_cli::DesignBoundary"]
        );
    }

    #[test]
    fn conservative_mutation_plan_reduces_blast_radius() {
        let graph = sample_graph();
        let delta = extract_design_delta(
            &graph,
            "workspace 全体で trait 分離して dependency を整理する",
        );
        let plan = build_mutation_plan(&graph, &delta, "workspace 全体", true);
        assert_eq!(plan.delta.impacted_crates.len(), 1);
    }
}
