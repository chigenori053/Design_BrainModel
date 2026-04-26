use architecture_reasoner::ArchitectureGraph;
use code_language_core::{CodeLanguageCore, ParsedSourceFile};
use design_domain::Layer;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct ReconstructionMetrics {
    module_recall: f64,
    dependency_recall: f64,
    layer_accuracy: f64,
}

#[test]
fn test5_large_codebase_architecture_reconstruction() {
    let files = build_large_codebase_fixture();
    let expected_modules = expected_module_names();
    let expected_dependencies = expected_dependencies();
    let expected_layers = expected_layers();
    let total_loc = files
        .iter()
        .map(|file| file.source.lines().count())
        .sum::<usize>();

    assert!(files.len() >= 100);
    assert!(total_loc >= 10_000, "loc={total_loc}");

    let core = CodeLanguageCore::default();
    let graph = core.reverse_architecture(&files);
    let metrics = reconstruction_metrics(
        &graph,
        &expected_modules,
        &expected_dependencies,
        &expected_layers,
    );

    println!(
        "Test5 Large Codebase Reconstruction\nmodule_recall: {:.2}\ndependency_recall: {:.2}\nlayer_accuracy: {:.2}",
        metrics.module_recall, metrics.dependency_recall, metrics.layer_accuracy
    );

    assert!(
        metrics.module_recall >= 0.8,
        "module_recall={}",
        metrics.module_recall
    );
    assert!(
        metrics.dependency_recall >= 0.8,
        "dependency_recall={}",
        metrics.dependency_recall
    );
    assert!(
        metrics.layer_accuracy >= 0.7,
        "layer_accuracy={}",
        metrics.layer_accuracy
    );
}

fn build_large_codebase_fixture() -> Vec<ParsedSourceFile> {
    let mut files = Vec::new();
    for group in 0..30 {
        files.push(module_file(group, "controller"));
        files.push(module_file(group, "service"));
        files.push(module_file(group, "repository"));
        files.push(module_file(group, "database"));
    }
    files
}

fn module_file(group: usize, kind: &str) -> ParsedSourceFile {
    let class_name = match kind {
        "controller" => format!("Orders{group}Controller"),
        "service" => format!("Orders{group}Service"),
        "repository" => format!("Orders{group}Repository"),
        "database" => format!("Orders{group}Database"),
        _ => unreachable!(),
    };
    let dependency_block = match kind {
        "controller" => format!("use crate::orders_{group}_service::Orders{group}Service;\n"),
        "service" => format!("use crate::orders_{group}_repository::Orders{group}Repository;\n"),
        "repository" => format!("use crate::orders_{group}_database::Orders{group}Database;\n"),
        "database" => String::new(),
        _ => unreachable!(),
    };
    let file_name = format!("src/orders_{group}_{kind}.rs");
    let filler = (0..96)
        .map(|line| format!("// filler line {line} for {class_name}"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        "{dependency_block}pub struct {class_name};\n\npub async fn handle_{group}_{kind}() {{\n    let _ = \"{class_name}\";\n}}\n{filler}\n"
    );
    ParsedSourceFile {
        path: file_name,
        source,
    }
}

fn expected_module_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for group in 0..30 {
        names.insert(format!("Orders{group}Controller"));
        names.insert(format!("Orders{group}Service"));
        names.insert(format!("Orders{group}Repository"));
        names.insert(format!("Orders{group}Database"));
    }
    names
}

fn expected_dependencies() -> BTreeSet<(String, String)> {
    let mut deps = BTreeSet::new();
    for group in 0..30 {
        deps.insert((
            format!("Orders{group}Controller"),
            format!("Orders{group}Service"),
        ));
        deps.insert((
            format!("Orders{group}Service"),
            format!("Orders{group}Repository"),
        ));
        deps.insert((
            format!("Orders{group}Repository"),
            format!("Orders{group}Database"),
        ));
    }
    deps
}

fn expected_layers() -> BTreeMap<String, Layer> {
    let mut layers = BTreeMap::new();
    for group in 0..30 {
        layers.insert(format!("Orders{group}Controller"), Layer::Ui);
        layers.insert(format!("Orders{group}Service"), Layer::Service);
        layers.insert(format!("Orders{group}Repository"), Layer::Repository);
        layers.insert(format!("Orders{group}Database"), Layer::Database);
    }
    layers
}

fn reconstruction_metrics(
    graph: &ArchitectureGraph,
    expected_modules: &BTreeSet<String>,
    expected_dependencies: &BTreeSet<(String, String)>,
    expected_layers: &BTreeMap<String, Layer>,
) -> ReconstructionMetrics {
    let actual_modules = graph
        .nodes
        .iter()
        .map(|node| node.name.clone())
        .collect::<BTreeSet<_>>();
    let module_hits = expected_modules.intersection(&actual_modules).count();
    let actual_lookup = graph
        .nodes
        .iter()
        .map(|node| (node.id, node.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let actual_dependencies = graph
        .dependency_edges()
        .filter_map(|edge| {
            Some((
                actual_lookup.get(&edge.from)?.clone(),
                actual_lookup.get(&edge.to)?.clone(),
            ))
        })
        .collect::<BTreeSet<_>>();
    let dependency_hits = expected_dependencies
        .intersection(&actual_dependencies)
        .count();
    let layer_hits = graph
        .nodes
        .iter()
        .filter(|node| expected_layers.get(&node.name) == Some(&node.layer))
        .count();

    ReconstructionMetrics {
        module_recall: module_hits as f64 / expected_modules.len() as f64,
        dependency_recall: dependency_hits as f64 / expected_dependencies.len() as f64,
        layer_accuracy: layer_hits as f64 / expected_layers.len() as f64,
    }
}
