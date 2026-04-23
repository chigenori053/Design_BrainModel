use std::collections::{HashMap, HashSet};
use crate::error::CodegenError;

/// Build adjacency list: module_name → list of module_names it imports from.
pub fn build_import_graph(
    modules: &[(String, Vec<String>)],
) -> HashMap<String, Vec<String>> {
    modules
        .iter()
        .map(|(name, deps)| (name.clone(), deps.clone()))
        .collect()
}

/// DFS-based cycle detection over the import graph.
/// Returns `Err(CyclicDependency)` with the cycle path if one is found.
pub fn detect_cycle(
    module_names: &[String],
    adj: &HashMap<String, Vec<String>>,
) -> Result<(), CodegenError> {
    let mut visited: HashSet<&str> = HashSet::new();
    let mut in_stack: HashSet<&str> = HashSet::new();

    for name in module_names {
        if !visited.contains(name.as_str()) {
            let mut path = vec![];
            dfs(name, adj, &mut visited, &mut in_stack, &mut path)?;
        }
    }
    Ok(())
}

fn dfs<'a>(
    node: &'a str,
    adj: &'a HashMap<String, Vec<String>>,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
    path: &mut Vec<&'a str>,
) -> Result<(), CodegenError> {
    visited.insert(node);
    in_stack.insert(node);
    path.push(node);

    if let Some(neighbors) = adj.get(node) {
        // Sort neighbours for deterministic traversal order.
        let mut sorted: Vec<&str> = neighbors.iter().map(|s| s.as_str()).collect();
        sorted.sort_unstable();

        for neighbour in sorted {
            if !visited.contains(neighbour) {
                // `neighbour` may not be a known module (validated separately).
                // Only recurse if it's in the graph.
                if adj.contains_key(neighbour) {
                    dfs(neighbour, adj, visited, in_stack, path)?;
                }
            } else if in_stack.contains(neighbour) {
                // Cycle found — collect the path segment forming the cycle.
                let start = path.iter().position(|&n| n == neighbour).unwrap_or(0);
                let mut cycle: Vec<String> =
                    path[start..].iter().map(|s| s.to_string()).collect();
                cycle.push(neighbour.to_string());
                return Err(CodegenError::CyclicDependency { cycle });
            }
        }
    }

    in_stack.remove(node);
    path.pop();
    Ok(())
}
