use design_domain::{Architecture, DependencyKind};

pub fn call_graph_edges(architecture: &Architecture) -> usize {
    architecture
        .dependencies
        .iter()
        .filter(|dependency| matches!(dependency.kind, DependencyKind::Calls))
        .count()
}
