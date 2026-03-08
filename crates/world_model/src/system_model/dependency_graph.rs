use design_domain::Architecture;

pub fn dependency_cycle_count(architecture: &Architecture) -> usize {
    let edges: Vec<(u64, u64)> = architecture
        .dependencies
        .iter()
        .map(|dependency| (dependency.from.0, dependency.to.0))
        .collect();

    edges
        .iter()
        .filter(|(from, to)| edges.iter().any(|(lhs, rhs)| lhs == to && rhs == from))
        .count()
        / 2
}
