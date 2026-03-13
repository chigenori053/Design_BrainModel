use architecture_reasoner::{
    ArchitectureEdge, ArchitectureEdgeKind, ArchitectureGraph, ArchitectureNode,
    ArchitectureNodeKind,
};
use code_language_core::{CodeLanguageCore, ParsedSourceFile};
use design_domain::Layer;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn test6_architecture_compression() {
    let files = build_large_codebase_fixture();
    let core = CodeLanguageCore::default();
    let graph = core.reverse_architecture(&files);
    let compressed = compress_by_layer(&graph);

    let compression_ratio = files.len() as f64 / compressed.nodes.len() as f64;
    let entropy_before = graph_entropy(&graph);
    let entropy_after = graph_entropy(&compressed);
    let entropy_reduction = if entropy_before == 0.0 {
        0.0
    } else {
        ((entropy_before - entropy_after) / entropy_before).clamp(0.0, 1.0)
    };

    println!(
        "Test6 Architecture Compression\ncompression_ratio: {:.2}\nentropy_reduction: {:.2}",
        compression_ratio, entropy_reduction
    );

    assert!(
        compression_ratio >= 10.0,
        "compression_ratio={compression_ratio}"
    );
    assert!(
        entropy_reduction >= 0.2,
        "entropy_reduction={entropy_reduction}"
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
    let filler = (0..96)
        .map(|line| format!("// filler line {line} for {class_name}"))
        .collect::<Vec<_>>()
        .join("\n");
    ParsedSourceFile {
        path: format!("src/orders_{group}_{kind}.rs"),
        source: format!(
            "{dependency_block}pub struct {class_name};\n\npub async fn handle_{group}_{kind}() {{\n    let _ = \"{class_name}\";\n}}\n{filler}\n"
        ),
    }
}

fn compress_by_layer(graph: &ArchitectureGraph) -> ArchitectureGraph {
    let mut layer_ids = BTreeMap::new();
    let mut nodes = Vec::new();
    for (index, layer) in [
        Layer::Ui,
        Layer::Service,
        Layer::Repository,
        Layer::Database,
    ]
    .into_iter()
    .enumerate()
    {
        let id = index as u64 + 1;
        layer_ids.insert(layer, id);
        nodes.push(ArchitectureNode {
            id,
            name: layer.as_str().to_string(),
            kind: ArchitectureNodeKind::Component,
            layer,
            responsibility: format!("compressed {}", layer.as_str().to_ascii_lowercase()),
        });
    }
    let node_layers = graph
        .nodes
        .iter()
        .map(|node| (node.id, node.layer))
        .collect::<BTreeMap<_, _>>();
    let edges = graph
        .dependency_edges()
        .filter_map(|edge| {
            let from_layer = node_layers.get(&edge.from)?;
            let to_layer = node_layers.get(&edge.to)?;
            Some((*layer_ids.get(from_layer)?, *layer_ids.get(to_layer)?))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|(from, to)| ArchitectureEdge {
            from,
            to,
            kind: ArchitectureEdgeKind::Dependency,
        })
        .collect::<Vec<_>>();
    ArchitectureGraph { nodes, edges }
}

fn graph_entropy(graph: &ArchitectureGraph) -> f64 {
    let edges = graph.dependency_edges().collect::<Vec<_>>();
    if edges.is_empty() {
        return 0.0;
    }
    let mut counts = BTreeMap::<(u64, u64), usize>::new();
    for edge in edges {
        *counts.entry((edge.from, edge.to)).or_default() += 1;
    }
    let total = counts.values().sum::<usize>() as f64;
    counts.values().fold(0.0, |sum, count| {
        let p = *count as f64 / total;
        if p == 0.0 { sum } else { sum - p * p.log2() }
    })
}
