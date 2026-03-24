use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use design_cli::service::{analyze_path, design_graph_from_analysis, enrich_analysis_report};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    from: String,
    to: String,
}

#[derive(Debug, Default)]
struct Graph {
    edges: Vec<Edge>,
    layers: BTreeMap<String, usize>,
}

impl Graph {
    fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to)
    }

    fn has_cycle(&self) -> bool {
        fn visit(
            node: &str,
            graph: &Graph,
            visiting: &mut BTreeSet<String>,
            visited: &mut BTreeSet<String>,
        ) -> bool {
            if visited.contains(node) {
                return false;
            }
            if !visiting.insert(node.to_string()) {
                return true;
            }
            for edge in graph.edges.iter().filter(|edge| edge.from == node) {
                if visit(&edge.to, graph, visiting, visited) {
                    return true;
                }
            }
            visiting.remove(node);
            visited.insert(node.to_string());
            false
        }

        let mut nodes = BTreeSet::new();
        for edge in &self.edges {
            nodes.insert(edge.from.clone());
            nodes.insert(edge.to.clone());
        }

        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        nodes
            .into_iter()
            .any(|node| visit(&node, self, &mut visiting, &mut visited))
    }
}

#[derive(Debug)]
struct DtoDef {
    name: String,
    methods: Vec<String>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn cli_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn module_file(name: &str) -> PathBuf {
    let src = cli_src();
    match name {
        "app" => src.join("app.rs"),
        "loop" => src.join("loop.rs"),
        "renderer" => src.join("renderer.rs"),
        "service" => src.join("service.rs"),
        "world" => src.join("world.rs"),
        "debug" => src.join("debug.rs"),
        other => panic!("unknown module {other}"),
    }
}

fn read_module(name: &str) -> String {
    fs::read_to_string(module_file(name)).expect("module source")
}

fn analyze_dependency_graph() -> Graph {
    let mut graph = Graph::default();
    graph.layers.insert("app".to_string(), 3);
    graph.layers.insert("loop".to_string(), 3);
    graph.layers.insert("renderer".to_string(), 2);
    graph.layers.insert("service".to_string(), 1);
    graph.layers.insert("world".to_string(), 0);
    graph.layers.insert("debug".to_string(), 0);

    let modules = ["app", "loop", "renderer", "service", "world", "debug"];
    for from in modules {
        let content = read_module(from);
        for to in modules {
            if from == to {
                continue;
            }
            let needle_use = format!("use crate::{to}");
            let needle_path = format!("crate::{to}::");
            if content.contains(&needle_use) || content.contains(&needle_path) {
                graph.edges.push(Edge {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
        }
    }

    graph
        .edges
        .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
    graph.edges.dedup();
    graph
}

fn get_module_deps(module: &str) -> Vec<String> {
    let graph = analyze_dependency_graph();
    graph
        .edges
        .into_iter()
        .filter(|edge| edge.from == module)
        .map(|edge| edge.to)
        .collect()
}

fn collect_dto_types() -> Vec<String> {
    let dto_path = cli_src().join("service").join("dto.rs");
    let content = fs::read_to_string(dto_path).expect("dto source");
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

fn parse_dto_defs() -> Vec<DtoDef> {
    let dto_path = cli_src().join("service").join("dto.rs");
    let content = fs::read_to_string(dto_path).expect("dto source");
    let mut defs = Vec::new();
    let mut names = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed
            .strip_prefix("pub struct ")
            .and_then(|rest| rest.split_whitespace().next())
        {
            let name = name.trim_end_matches('{').to_string();
            names.push(name.clone());
            defs.push(DtoDef {
                name,
                methods: Vec::new(),
            });
        }
    }

    for name in &names {
        let impl_prefix = format!("impl {name}");
        if content
            .lines()
            .any(|line| line.trim_start().starts_with(&impl_prefix))
        {
            let def = defs
                .iter_mut()
                .find(|def| def.name == *name)
                .expect("dto def");
            def.methods.push("impl".to_string());
        }
    }

    defs
}

fn clean_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("architecture_clean")
}

#[test]
fn test_no_forbidden_dependencies() {
    let graph = analyze_dependency_graph();

    assert!(!graph.has_edge("renderer", "world"));
    assert!(!graph.has_edge("app", "world"));
    assert!(!graph.has_edge("loop", "world"));
    assert!(!graph.has_edge("service", "renderer"));
}

#[test]
fn test_no_cycles() {
    let graph = analyze_dependency_graph();
    assert!(!graph.has_cycle());
}

#[test]
fn test_layer_constraints() {
    let graph = analyze_dependency_graph();

    for edge in &graph.edges {
        let from = graph.layers.get(&edge.from).expect("from layer");
        let to = graph.layers.get(&edge.to).expect("to layer");
        assert!(
            from >= to,
            "layer violation: {}({}) -> {}({})",
            edge.from,
            from,
            edge.to,
            to
        );
    }
}

#[test]
fn test_no_world_types_in_dto() {
    for ty in collect_dto_types() {
        assert!(!ty.contains("world::"), "world type leaked into dto: {ty}");
    }
}

#[test]
fn test_dto_no_methods() {
    for dto in parse_dto_defs() {
        assert_eq!(dto.methods.len(), 0, "dto has methods: {}", dto.name);
    }
}

#[test]
fn test_dbm_analyze_clean() {
    let root = clean_fixture_root();
    let analysis = analyze_path(&root).expect("analyze fixture");
    let graph = design_graph_from_analysis(&analysis);
    let report = enrich_analysis_report(analysis, integration_layer::diagnostic_analysis(&graph));

    assert!(!report.cycles.has_cycle);
    assert!(
        report.violations.is_empty(),
        "violations: {:?}",
        report.violations
    );
    assert_eq!(report.summary.critical, 0);
    assert_eq!(report.summary.high, 0);
}

#[test]
fn test_renderer_no_world_access() {
    let deps = get_module_deps("renderer");
    assert!(!deps.iter().any(|dep| dep == "world"));
}

#[test]
fn test_workspace_fixture_exists() {
    assert!(workspace_root().join("apps/cli").exists());
}
