use architecture_domain::ArchitectureState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphMetrics {
    pub node_count: usize,
    pub edge_count: usize,
    pub max_depth: usize,
    pub cycle_count: usize,
    pub max_fan_in: usize,
    pub max_fan_out: usize,
    pub centrality_peak: usize,
}

pub trait GraphAnalysisEngine {
    fn analyze(&self, architecture: &ArchitectureState) -> GraphMetrics;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicGraphAnalysisEngine;

impl GraphAnalysisEngine for DeterministicGraphAnalysisEngine {
    fn analyze(&self, architecture: &ArchitectureState) -> GraphMetrics {
        let node_count = architecture.metrics.component_count;
        let edge_count = architecture.metrics.dependency_count;
        let ids = architecture
            .components
            .iter()
            .map(|component| component.id.0)
            .collect::<Vec<_>>();

        let max_fan_in = ids
            .iter()
            .map(|id| {
                architecture
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.to.0 == *id)
                    .count()
            })
            .max()
            .unwrap_or(0);
        let max_fan_out = ids
            .iter()
            .map(|id| {
                architecture
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.from.0 == *id)
                    .count()
            })
            .max()
            .unwrap_or(0);
        let centrality_peak = ids
            .iter()
            .map(|id| {
                architecture
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.from.0 == *id || dependency.to.0 == *id)
                    .count()
            })
            .max()
            .unwrap_or(0);

        GraphMetrics {
            node_count,
            edge_count,
            max_depth: longest_path_len(architecture),
            cycle_count: count_two_node_cycles(architecture),
            max_fan_in,
            max_fan_out,
            centrality_peak,
        }
    }
}

fn longest_path_len(architecture: &ArchitectureState) -> usize {
    let ids = architecture
        .components
        .iter()
        .map(|component| component.id.0)
        .collect::<Vec<_>>();

    ids.iter()
        .map(|id| depth_from(*id, architecture, &mut Vec::new()))
        .max()
        .unwrap_or(0)
}

fn depth_from(node: u64, architecture: &ArchitectureState, stack: &mut Vec<u64>) -> usize {
    if stack.contains(&node) {
        return 0;
    }
    stack.push(node);
    let best = architecture
        .dependencies
        .iter()
        .filter(|dependency| dependency.from.0 == node)
        .map(|dependency| 1 + depth_from(dependency.to.0, architecture, stack))
        .max()
        .unwrap_or(1);
    stack.pop();
    best
}

fn count_two_node_cycles(architecture: &ArchitectureState) -> usize {
    let mut visited = Vec::new();
    let mut stack = Vec::new();
    let mut cycles = 0;

    let mut ids = architecture
        .components
        .iter()
        .map(|component| component.id.0)
        .collect::<Vec<_>>();
    ids.sort_unstable();

    for id in ids {
        if dfs_cycle_count(id, architecture, &mut visited, &mut stack, &mut cycles) {
            stack.clear();
        }
    }

    cycles
}

fn dfs_cycle_count(
    node: u64,
    architecture: &ArchitectureState,
    visited: &mut Vec<u64>,
    stack: &mut Vec<u64>,
    cycles: &mut usize,
) -> bool {
    if stack.contains(&node) {
        *cycles += 1;
        return true;
    }
    if visited.contains(&node) {
        return false;
    }

    visited.push(node);
    stack.push(node);
    let mut found = false;
    for next in architecture
        .dependencies
        .iter()
        .filter(|dependency| dependency.from.0 == node)
        .map(|dependency| dependency.to.0)
    {
        found |= dfs_cycle_count(next, architecture, visited, stack, cycles);
    }
    stack.pop();
    found
}
