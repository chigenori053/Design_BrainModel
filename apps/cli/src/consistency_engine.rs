//! DBM Consistency Engine  (DBM-CONSISTENCY-ENGINE-SPEC v1.0, Phase B-4)
//!
//! Validates a [`ProjectIr`] snapshot for:
//!
//! 1. **Dependency Correctness** – every tracked dependency exists on disk.
//! 2. **State Consistency**   – no file has drifted from its IR snapshot.
//! 3. **Execution Readiness** – the graph is acyclic and all nodes are synced.
//! 4. **Violation Detection** – classifies every anomaly into a [`Violation`].
//!
//! # Design constraints
//! - Deterministic: same [`ProjectIr`] input → same [`ConsistencyReport`].
//! - O(nodes + edges) time complexity.
//! - Pure function: no mutation, no I/O beyond the file-hash read.
//!
//! # Architectural position
//! ```text
//! REPL
//!  ↓
//! Planner
//!  ↓
//! Consistency Engine   ← this module
//!  ↓
//! Executor
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ir_state::ProjectIr;

// ─── § 5.1 Violation ─────────────────────────────────────────────────────────

/// A single consistency violation found in a [`ProjectIr`].
///
/// Variants map directly to the six violation kinds defined in
/// DBM-CONSISTENCY-ENGINE-SPEC § 5.1.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Violation {
    /// A `use` / `mod` import references a module that cannot be resolved to
    /// any tracked file.  The path is the *source* file containing the
    /// unresolvable import statement.
    UnresolvedImport(PathBuf),
    /// A tracked dependency edge points to a path that does not exist on disk.
    MissingDependency(PathBuf),
    /// The on-disk content of this file has diverged from the IR snapshot hash.
    DriftDetected(PathBuf),
    /// A dependency of this file is in a dirty / drifted state.
    /// The path is the dirty dependency, not the dependent.
    DirtyDependency(PathBuf),
    /// A cycle exists in the dependency graph.  The path list describes the
    /// cycle in traversal order (last element == first element to close it).
    CyclicDependency(Vec<PathBuf>),
    /// The IR snapshot is internally inconsistent (e.g. the file is
    /// unreadable so the snapshot cannot be verified).
    InconsistentState(PathBuf),
}

impl Violation {
    /// Human-readable one-line description (§ 9 output format).
    pub fn describe(&self) -> String {
        match self {
            Violation::UnresolvedImport(p) => {
                format!("UnresolvedImport: {}", p.display())
            }
            Violation::MissingDependency(p) => {
                format!("MissingDependency: {}", p.display())
            }
            Violation::DriftDetected(p) => {
                format!("DriftDetected: {}", p.display())
            }
            Violation::DirtyDependency(p) => {
                format!("DirtyDependency: {}", p.display())
            }
            Violation::CyclicDependency(paths) => {
                let joined = paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" → ");
                format!("CyclicDependency: [{joined}]")
            }
            Violation::InconsistentState(p) => {
                format!("InconsistentState: {}", p.display())
            }
        }
    }
}

// ─── § 5.2 ConsistencyReport ─────────────────────────────────────────────────

/// Result of a full consistency check over a [`ProjectIr`] (§ 5.2).
///
/// `is_consistent == true` iff `violations` is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyReport {
    /// `true` when no violations were found; the IR is safe for execution.
    pub is_consistent: bool,
    /// All violations detected, in deterministic order.
    pub violations: Vec<Violation>,
}

impl ConsistencyReport {
    /// Render the report in the canonical output format (§ 9).
    ///
    /// # Normal output (§ 9.1)
    /// ```text
    /// [CONSISTENCY]
    /// status: OK
    /// ```
    ///
    /// # Failure output (§ 9.2)
    /// ```text
    /// [CONSISTENCY]
    /// status: FAILED
    ///
    /// violations:
    ///   - DriftDetected: apps/a.rs
    ///   - MissingDependency: apps/b.rs
    /// ```
    pub fn render(&self) -> String {
        if self.is_consistent {
            return "[CONSISTENCY]\nstatus: OK".to_string();
        }

        let mut lines = vec![
            "[CONSISTENCY]".to_string(),
            "status: FAILED".to_string(),
            String::new(),
            "violations:".to_string(),
        ];
        for v in &self.violations {
            lines.push(format!("  - {}", v.describe()));
        }
        lines.join("\n")
    }
}

// ─── § 4.1 / § 7 ConsistencyEngine ──────────────────────────────────────────

/// Validates a [`ProjectIr`] and returns a [`ConsistencyReport`].
///
/// All methods are stateless pure functions.
pub struct ConsistencyEngine;

impl ConsistencyEngine {
    /// Run all consistency checks on `project` and return a report (§ 7).
    ///
    /// Check order (deterministic, sorted by file path):
    /// 1. **Drift** – `state.dirty` flag or hash mismatch (§ 6.2).
    /// 2. **IR state** – file is unreadable → [`Violation::InconsistentState`] (§ 6.5).
    /// 3. **Dependency existence** – tracked dep missing on disk (§ 6.1).
    /// 4. **Dirty dependency** – dep is in drifted state (§ 6.3).
    /// 5. **Unresolved import edges** – edge points outside tracked nodes (§ 6.1).
    /// 6. **Cycle detection** – SCC via DFS coloring (§ 6.4).
    pub fn check(project: &ProjectIr) -> ConsistencyReport {
        let mut violations: Vec<Violation> = Vec::new();

        // Stable traversal order: sort by path.
        let mut paths: Vec<&PathBuf> = project.nodes.keys().collect();
        paths.sort();

        for path in &paths {
            let state = &project.nodes[*path];

            // § 6.2 Drift detection.
            if state.dirty {
                // dirty flag already set by IrStateManager – file has changed.
                violations.push(Violation::DriftDetected((*path).clone()));
            } else {
                // Secondary verification: re-read hash from disk.
                match crate::ir_sync::hash_file(path) {
                    Ok(current) if current != state.snapshot.file_hash => {
                        violations.push(Violation::DriftDetected((*path).clone()));
                    }
                    Err(_) => {
                        // File unreadable – snapshot cannot be trusted.
                        violations.push(Violation::InconsistentState((*path).clone()));
                    }
                    Ok(_) => {}
                }
            }

            // § 6.1 / § 6.3 Per-dependency checks.
            // Sort for deterministic output.
            let mut deps = state.dependencies.clone();
            deps.sort();
            for dep in &deps {
                // Existence: dependency must be present on disk.
                if !dep.exists() {
                    violations.push(Violation::MissingDependency(dep.clone()));
                }
                // Dirty propagation: dependency is drifted.
                if let Some(dep_state) = project.nodes.get(dep) {
                    if dep_state.dirty {
                        violations.push(Violation::DirtyDependency(dep.clone()));
                    }
                }
            }
        }

        // § 6.1 Unresolved import edges (edge.to not in nodes).
        let mut sorted_edges = project.edges.clone();
        sorted_edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
        for edge in &sorted_edges {
            if !project.nodes.contains_key(&edge.to) {
                violations.push(Violation::UnresolvedImport(edge.from.clone()));
            }
        }

        // § 6.4 Cycle detection.
        if let Some(cycle) = detect_cycle(project) {
            violations.push(Violation::CyclicDependency(cycle));
        }

        // Deduplicate (stable, preserve first occurrence).
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        violations.retain(|v| seen.insert(v.describe()));

        let is_consistent = violations.is_empty();
        ConsistencyReport {
            is_consistent,
            violations,
        }
    }
}

// ─── § 7.2 Cycle detection ────────────────────────────────────────────────────

/// Detect a cycle in `project.edges` using DFS coloring.
///
/// Returns the first cycle found as an ordered path list, or `None` when the
/// graph is acyclic.  Runs in O(V + E).
///
/// Color encoding: `0` = unvisited, `1` = in current DFS path (gray),
/// `2` = fully processed (black).
fn detect_cycle(project: &ProjectIr) -> Option<Vec<PathBuf>> {
    let mut color: HashMap<PathBuf, u8> = project.nodes.keys().map(|k| (k.clone(), 0u8)).collect();

    let mut paths: Vec<PathBuf> = project.nodes.keys().cloned().collect();
    paths.sort();

    for start in &paths {
        if color.get(start).copied().unwrap_or(0) == 0 {
            let mut stack: Vec<PathBuf> = Vec::new();
            if let Some(cycle) = dfs_cycle(start, project, &mut color, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
}

/// Recursive DFS helper for cycle detection.
fn dfs_cycle(
    node: &PathBuf,
    project: &ProjectIr,
    color: &mut HashMap<PathBuf, u8>,
    stack: &mut Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    color.insert(node.clone(), 1); // mark gray (in-progress)
    stack.push(node.clone());

    if let Some(state) = project.nodes.get(node) {
        let mut deps = state.dependencies.clone();
        deps.sort(); // deterministic traversal order

        for dep in &deps {
            let dep_color = color.get(dep).copied().unwrap_or(0);
            if dep_color == 1 {
                // Back edge found – we have a cycle.
                let cycle_start = stack.iter().position(|p| p == dep).unwrap_or(0);
                let mut cycle: Vec<PathBuf> = stack[cycle_start..].to_vec();
                cycle.push(dep.clone()); // close the loop
                return Some(cycle);
            }
            if dep_color == 0 {
                if let Some(cycle) = dfs_cycle(dep, project, color, stack) {
                    return Some(cycle);
                }
            }
        }
    }

    stack.pop();
    color.insert(node.clone(), 2); // mark black (done)
    None
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::ir_state::{CodeIr, DependencyEdge, DependencyKind, IrSnapshot, IrState, ProjectIr};

    /// Write `content` to a temp file and return its path.
    fn tmp(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Build a minimal synced [`IrState`] for a file that exists on disk.
    fn synced_state(path: &PathBuf) -> IrState {
        let content = std::fs::read_to_string(path).unwrap();
        let hash = crate::ir_sync::hash_content(&content);
        IrState {
            snapshot: IrSnapshot {
                file_path: path.clone(),
                file_hash: hash,
                ir: CodeIr {
                    source: content,
                    lang: "rs".to_string(),
                },
                generated_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            },
            dirty: false,
            dependencies: Vec::new(),
            dependents: Vec::new(),
        }
    }

    // ── § 14.4: clean project → is_consistent ───────────────────────────────

    #[test]
    fn clean_project_is_consistent() {
        let path = tmp("ce_clean.rs", "fn main() {}");
        let mut nodes = std::collections::HashMap::new();
        nodes.insert(path.clone(), synced_state(&path));
        let project = ProjectIr {
            nodes,
            edges: vec![],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(report.is_consistent, "violations: {:?}", report.violations);
        assert_eq!(report.render(), "[CONSISTENCY]\nstatus: OK");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_project_is_consistent() {
        let report = ConsistencyEngine::check(&ProjectIr::default());
        assert!(report.is_consistent);
    }

    // ── § 14.2: Drift ────────────────────────────────────────────────────────

    #[test]
    fn dirty_flag_raises_drift_detected() {
        let path = tmp("ce_drift.rs", "fn main() {}");
        let mut state = synced_state(&path);
        state.dirty = true; // simulate drift

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(path.clone(), state);
        let project = ProjectIr {
            nodes,
            edges: vec![],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::DriftDetected(_))),
            "expected DriftDetected, got: {:?}",
            report.violations
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn filesystem_change_raises_drift_detected() {
        let path = tmp("ce_fsdrift.rs", "fn v1() {}");
        let state = synced_state(&path); // snapshot captured for "fn v1() {}"
        std::fs::write(&path, "fn v2() {}").unwrap(); // file changes on disk

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(path.clone(), state);
        let project = ProjectIr {
            nodes,
            edges: vec![],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::DriftDetected(_))),
            "{:?}",
            report.violations
        );
        let _ = std::fs::remove_file(&path);
    }

    // ── § 14.1: Missing dependency ───────────────────────────────────────────

    #[test]
    fn missing_dependency_raises_violation() {
        let path = tmp("ce_main.rs", "fn main() {}");
        let ghost = PathBuf::from("/nonexistent/__ce_ghost.rs");

        let mut state = synced_state(&path);
        state.dependencies = vec![ghost.clone()];

        let ghost_state = IrState {
            snapshot: IrSnapshot {
                file_path: ghost.clone(),
                file_hash: 0,
                ir: CodeIr::default(),
                generated_at: 0,
            },
            dirty: false,
            dependencies: vec![],
            dependents: vec![path.clone()],
        };

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(path.clone(), state);
        nodes.insert(ghost.clone(), ghost_state);
        let project = ProjectIr {
            nodes,
            edges: vec![DependencyEdge {
                from: path.clone(),
                to: ghost.clone(),
                kind: DependencyKind::Import,
            }],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::MissingDependency(_))),
            "{:?}",
            report.violations
        );
        let _ = std::fs::remove_file(&path);
    }

    // ── § 14.3: Cyclic dependency ────────────────────────────────────────────

    #[test]
    fn cycle_raises_cyclic_dependency() {
        let a = tmp("ce_cycle_a.rs", "fn a() {}");
        let b = tmp("ce_cycle_b.rs", "fn b() {}");

        let mut state_a = synced_state(&a);
        state_a.dependencies = vec![b.clone()];
        state_a.dependents = vec![];

        let mut state_b = synced_state(&b);
        state_b.dependencies = vec![a.clone()]; // cycle: a → b → a
        state_b.dependents = vec![];

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(a.clone(), state_a);
        nodes.insert(b.clone(), state_b);
        let project = ProjectIr {
            nodes,
            edges: vec![
                DependencyEdge {
                    from: a.clone(),
                    to: b.clone(),
                    kind: DependencyKind::Import,
                },
                DependencyEdge {
                    from: b.clone(),
                    to: a.clone(),
                    kind: DependencyKind::Import,
                },
            ],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::CyclicDependency(_))),
            "{:?}",
            report.violations
        );
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }

    // ── DirtyDependency ──────────────────────────────────────────────────────

    #[test]
    fn dirty_dependency_raises_violation() {
        let a = tmp("ce_dep_a.rs", "fn a() {}");
        let b = tmp("ce_dep_b.rs", "fn b() {}");

        let mut state_a = synced_state(&a);
        state_a.dependencies = vec![b.clone()];

        let mut state_b = synced_state(&b);
        state_b.dirty = true; // b is drifted

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(a.clone(), state_a);
        nodes.insert(b.clone(), state_b);
        let project = ProjectIr {
            nodes,
            edges: vec![DependencyEdge {
                from: a.clone(),
                to: b.clone(),
                kind: DependencyKind::Import,
            }],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::DirtyDependency(_))),
            "{:?}",
            report.violations
        );
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }

    // ── UnresolvedImport ─────────────────────────────────────────────────────

    #[test]
    fn edge_to_untracked_node_raises_unresolved_import() {
        let a = tmp("ce_unresolved.rs", "fn a() {}");
        let phantom = PathBuf::from("/nonexistent/__ce_phantom.rs");

        let state_a = synced_state(&a);
        let mut nodes = std::collections::HashMap::new();
        nodes.insert(a.clone(), state_a);

        // Edge points outside `nodes` → UnresolvedImport
        let project = ProjectIr {
            nodes,
            edges: vec![DependencyEdge {
                from: a.clone(),
                to: phantom.clone(), // NOT in nodes
                kind: DependencyKind::Import,
            }],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::UnresolvedImport(_))),
            "{:?}",
            report.violations
        );
        let _ = std::fs::remove_file(&a);
    }

    // ── InconsistentState ────────────────────────────────────────────────────

    #[test]
    fn unreadable_file_raises_inconsistent_state() {
        let path = PathBuf::from("/nonexistent/__ce_unreadable.rs");

        // Build a state that claims the file exists but it does not.
        let state = IrState {
            snapshot: IrSnapshot {
                file_path: path.clone(),
                file_hash: 99999, // arbitrary – will never match real hash
                ir: CodeIr::default(),
                generated_at: 0,
            },
            dirty: false,
            dependencies: vec![],
            dependents: vec![],
        };

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(path.clone(), state);
        let project = ProjectIr {
            nodes,
            edges: vec![],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(!report.is_consistent);
        assert!(
            report.violations.iter().any(|v| matches!(
                v,
                Violation::InconsistentState(_) | Violation::DriftDetected(_)
            )),
            "{:?}",
            report.violations
        );
    }

    // ── render output format ─────────────────────────────────────────────────

    #[test]
    fn render_ok_format() {
        let report = ConsistencyReport {
            is_consistent: true,
            violations: vec![],
        };
        assert_eq!(report.render(), "[CONSISTENCY]\nstatus: OK");
    }

    #[test]
    fn render_failed_format() {
        let report = ConsistencyReport {
            is_consistent: false,
            violations: vec![
                Violation::DriftDetected(PathBuf::from("apps/a.rs")),
                Violation::MissingDependency(PathBuf::from("apps/b.rs")),
            ],
        };
        let rendered = report.render();
        assert!(rendered.contains("status: FAILED"));
        assert!(rendered.contains("DriftDetected: apps/a.rs"));
        assert!(rendered.contains("MissingDependency: apps/b.rs"));
    }

    // ── Multi-node clean graph ───────────────────────────────────────────────

    #[test]
    fn two_node_clean_graph_is_consistent() {
        let a = tmp("ce_two_a.rs", "fn a() {}");
        let b = tmp("ce_two_b.rs", "fn b() {}");

        let mut state_a = synced_state(&a);
        state_a.dependencies = vec![b.clone()];

        let state_b = synced_state(&b);

        let mut nodes = std::collections::HashMap::new();
        nodes.insert(a.clone(), state_a);
        nodes.insert(b.clone(), state_b);
        let project = ProjectIr {
            nodes,
            edges: vec![DependencyEdge {
                from: a.clone(),
                to: b.clone(),
                kind: DependencyKind::Import,
            }],
        };

        let report = ConsistencyEngine::check(&project);

        assert!(report.is_consistent, "{:?}", report.violations);
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }
}
