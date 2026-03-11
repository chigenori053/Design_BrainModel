use architecture_reasoner::{ArchitectureNodeKind, ReverseArchitectureReasoner};
use code_language_core::{CodeLanguageCore, ParsedSourceFile};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
struct RealCodebaseMetrics {
    module_recall: f64,
    dependency_recall: f64,
    component_accuracy: f64,
}

#[test]
fn test9_real_codebase_reconstruction() {
    let files = load_workspace_rust_sources();
    let total_loc = files
        .iter()
        .map(|file| file.source.lines().count())
        .sum::<usize>();

    assert!(total_loc > 20_000, "loc={total_loc}");

    let expected_modules = collect_expected_modules(&files);
    let expected_dependencies = collect_expected_dependencies(&files);

    let core = CodeLanguageCore::default();
    let graph = ReverseArchitectureReasoner.infer_from_code_ir(&core.parse_sources(&files));
    let metrics = real_codebase_metrics(&graph, &expected_modules, &expected_dependencies);

    println!(
        "Test9 Real Codebase\nmodule_recall: {:.2}\ndependency_recall: {:.2}\ncomponent_accuracy: {:.2}",
        metrics.module_recall, metrics.dependency_recall, metrics.component_accuracy
    );

    assert!(
        metrics.module_recall >= 0.75,
        "module_recall={}",
        metrics.module_recall
    );
    assert!(
        metrics.dependency_recall >= 0.75,
        "dependency_recall={}",
        metrics.dependency_recall
    );
    assert!(
        metrics.component_accuracy >= 0.70,
        "component_accuracy={}",
        metrics.component_accuracy
    );
}

fn load_workspace_rust_sources() -> Vec<ParsedSourceFile> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root");
    let crates_dir = root.join("crates");
    let mut paths = Vec::new();
    collect_rs_files(&crates_dir, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let source = fs::read_to_string(&path).ok()?;
            let relative = path.strip_prefix(&root).ok()?.to_string_lossy().to_string();
            Some(ParsedSourceFile {
                path: relative,
                source,
            })
        })
        .collect()
}

fn collect_rs_files(dir: &PathBuf, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, paths);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            paths.push(path);
        }
    }
}

fn collect_expected_modules(files: &[ParsedSourceFile]) -> BTreeSet<String> {
    files.iter().filter_map(primary_declaration_name).collect()
}

fn collect_expected_dependencies(files: &[ParsedSourceFile]) -> BTreeSet<(String, String)> {
    let module_names = collect_expected_modules(files);
    files
        .iter()
        .flat_map(|file| {
            let module_names = module_names.clone();
            let owner = primary_declaration_name(file).unwrap_or_else(|| {
                file.path
                    .rsplit('/')
                    .next()
                    .unwrap_or(file.path.as_str())
                    .trim_end_matches(".rs")
                    .to_string()
            });
            file.source.lines().filter_map(move |line| {
                let trimmed = line.trim();
                let target = trimmed
                    .strip_prefix("use ")
                    .and_then(|rest| rest.split("::").last())
                    .map(|segment| segment.trim_end_matches(';').trim().to_string())?;
                if module_names.contains(&target) {
                    Some((owner.clone(), target))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn primary_declaration_name(file: &ParsedSourceFile) -> Option<String> {
    file.source.lines().find_map(|line| {
        let trimmed = line.trim();
        extract_name(trimmed, "pub struct ")
            .or_else(|| extract_name(trimmed, "struct "))
            .or_else(|| extract_name(trimmed, "pub enum "))
            .or_else(|| extract_name(trimmed, "enum "))
            .or_else(|| extract_name(trimmed, "pub trait "))
            .or_else(|| extract_name(trimmed, "trait "))
            .map(|name| name.to_string())
    })
}

fn real_codebase_metrics(
    graph: &architecture_reasoner::ArchitectureGraph,
    expected_modules: &BTreeSet<String>,
    expected_dependencies: &BTreeSet<(String, String)>,
) -> RealCodebaseMetrics {
    let actual_modules = graph
        .nodes
        .iter()
        .map(|node| node.name.clone())
        .collect::<BTreeSet<_>>();
    let module_hits = expected_modules.intersection(&actual_modules).count();
    let lookup = graph
        .nodes
        .iter()
        .map(|node| (node.id, node.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let actual_dependencies = graph
        .dependency_edges()
        .filter_map(|edge| {
            Some((
                lookup.get(&edge.from)?.clone(),
                lookup.get(&edge.to)?.clone(),
            ))
        })
        .collect::<BTreeSet<_>>();
    let dependency_hits = expected_dependencies
        .intersection(&actual_dependencies)
        .count();
    let component_hits = graph
        .nodes
        .iter()
        .filter(|node| expected_kind(&node.name) == node.kind)
        .count();

    RealCodebaseMetrics {
        module_recall: if expected_modules.is_empty() {
            1.0
        } else {
            module_hits as f64 / expected_modules.len() as f64
        },
        dependency_recall: if expected_dependencies.is_empty() {
            1.0
        } else {
            dependency_hits as f64 / expected_dependencies.len() as f64
        },
        component_accuracy: if graph.nodes.is_empty() {
            1.0
        } else {
            component_hits as f64 / graph.nodes.len() as f64
        },
    }
}

fn expected_kind(name: &str) -> ArchitectureNodeKind {
    let lower = name.to_ascii_lowercase();
    if lower.contains("module") || lower.contains("service") || lower.contains("repository") {
        ArchitectureNodeKind::Module
    } else if lower.contains("component") || lower.contains("gateway") {
        ArchitectureNodeKind::Component
    } else {
        ArchitectureNodeKind::Class
    }
}

fn extract_name<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.strip_prefix(marker)
        .and_then(|rest| {
            rest.split(|ch: char| {
                ch == '(' || ch == '{' || ch == ';' || ch == ':' || ch == '<' || ch.is_whitespace()
            })
            .next()
        })
        .filter(|name| !name.is_empty())
}
