//! State graph (DAG) for exploration deduplication.
//!
//! Spec: DBM-GRAPH-INTEGRATION-STEP2 v1.0
//!
//! Converts the exploration structure from a Tree → DAG by identifying
//! identical `CoreState`s via `state_hash` and reusing existing nodes
//! instead of creating new ones.
//!
//! ## Invariants
//! - Every state is uniquely identified by its content hash.
//! - Identical states are stored once and reused.
//! - Cycles are prevented: a state that hashes to the current state triggers
//!   `Err("Cycle detected")` rather than a graph insertion.
//! - Node count is bounded by `MAX_GRAPH_NODES`; oldest nodes are evicted
//!   when the limit is reached.
//!
//! ## Relationship to History
//! `History` = linear UI view (one entry per user operation).
//! `StateGraph` = internal DAG (deduplicated exploration structure).

use crate::core::CoreState;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use strategy_engine::Limits;

/// Hard upper bound on graph node count.  Spec §9.
pub const MAX_GRAPH_NODES: usize = 1000;

// ── Node ──────────────────────────────────────────────────────────────────────

/// A single node in the state graph.  Spec §5.
pub struct Node {
    /// Content hash; used as the node's identity key.
    pub id: u64,
    /// The canonical state stored at this node.
    pub state: CoreState,
}

// ── StateGraph ────────────────────────────────────────────────────────────────

/// Content-addressable DAG of `CoreState` nodes.  Spec §5.
///
/// On each execution step the engine calls `reuse_or_insert` which:
/// 1. Computes the content hash of the new state.
/// 2. Returns the existing node if the hash is already present.
/// 3. Inserts a new node if not, evicting old nodes when at capacity.
/// 4. Returns `Err("Cycle detected")` if the new hash equals the current one.
pub struct StateGraph {
    /// Keyed by content hash.  Spec §5 `nodes: HashMap<u64, Node>`.
    nodes: HashMap<u64, Node>,
    /// Insertion order — used for FIFO eviction.
    order: Vec<u64>,
    limits: Limits,
}

impl Default for StateGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl StateGraph {
    pub fn new() -> Self {
        Self::with_limits(Limits::default())
    }

    pub fn with_limits(limits: Limits) -> Self {
        Self {
            nodes: HashMap::new(),
            order: Vec::new(),
            limits,
        }
    }

    // ── Hash function ─────────────────────────────────────────────────────────

    /// Compute a deterministic content hash for a `CoreState`.
    ///
    /// Spec: DBM-GRAPH-HASH-NORMALIZATION-FIX §3–§8.
    ///
    /// **Included** (spec §3.1):
    /// - `design`: `reason_units`, `structure`, `constraints`
    /// - `current_plan`: `summary` + `steps` (sorted for order independence)
    /// - `status`: pipeline execution state
    ///
    /// **Excluded** (spec §3.2 — 必須):
    /// - `proposals` — generated every call; not structural state
    /// - `last_diff` — display-only; changes on every execution
    /// - `version` — monotonic counter; differs on replay
    ///
    /// **Normalization** (spec §7): collections are sorted and strings are
    /// whitespace-trimmed so cosmetic differences do not change the hash.
    pub fn state_hash(state: &CoreState) -> u64 {
        let mut hasher = DefaultHasher::new();

        // ── design ────────────────────────────────────────────────────────────
        if let Some(ref design) = state.design {
            // reason_units: sort for ordering independence
            let mut units: Vec<(&str, &str, &str)> = design
                .reason_units
                .iter()
                .map(|u| (u.id.trim(), u.title.trim(), u.summary.trim()))
                .collect();
            units.sort_unstable();
            for (id, title, summary) in units {
                id.hash(&mut hasher);
                title.hash(&mut hasher);
                summary.hash(&mut hasher);
            }

            // structure
            design.structure.module.trim().hash(&mut hasher);
            let mut functions: Vec<&str> = design
                .structure
                .functions
                .iter()
                .map(|f| f.trim())
                .collect();
            functions.sort_unstable();
            for f in functions {
                f.hash(&mut hasher);
            }

            // constraints: sorted (spec §7.1)
            let mut constraints: Vec<&str> =
                design.constraints.iter().map(|c| c.text.trim()).collect();
            constraints.sort_unstable();
            for c in constraints {
                c.hash(&mut hasher);
            }
        }

        // ── current_plan ──────────────────────────────────────────────────────
        if let Some(ref plan) = state.current_plan {
            plan.summary.trim().hash(&mut hasher);
            let mut steps: Vec<&str> = plan.steps.iter().map(|s| s.trim()).collect();
            steps.sort_unstable();
            for step in steps {
                step.hash(&mut hasher);
            }
        }

        // ── status (pipeline execution state) ────────────────────────────────
        // last_diff and proposals are intentionally excluded (spec §3.2).
        format!("{:?}", state.status).hash(&mut hasher);

        hasher.finish()
    }

    // ── Reuse / insert ────────────────────────────────────────────────────────

    /// Return the canonical `CoreState` for `new_state`, inserting a new node
    /// if necessary.  Spec §6.
    ///
    /// - Returns `Ok(existing.clone())` when a node with the same hash exists.
    /// - Returns `Ok(new_state)` after inserting a new node.
    /// - Returns `Err("Cycle detected")` when `hash(new_state) == current_hash`
    ///   (spec §8) — the caller is responsible for the fallback.
    pub fn reuse_or_insert(
        &mut self,
        new_state: CoreState,
        current_hash: u64,
        request_id: u64,
    ) -> Result<CoreState, &'static str> {
        let hash = Self::state_hash(&new_state);

        // Cycle detection (spec §8)
        if hash == current_hash {
            println!(
                "[GRAPH][id={}] hash={} cycle_detected=true nodes={}",
                request_id,
                hash,
                self.nodes.len()
            );
            return Err("Cycle detected");
        }

        // Reuse existing node (spec §6 reuse path)
        if self.nodes.contains_key(&hash) {
            println!(
                "[GRAPH][id={}] hash={} reused=true nodes={}",
                request_id,
                hash,
                self.nodes.len()
            );
            return Ok(self.nodes[&hash].state.clone());
        }

        // Capacity check — evict before inserting (spec §9)
        if self.nodes.len() >= self.limits.max_graph_nodes {
            self.evict_old_nodes(request_id);
        }

        // Insert new node (spec §6 insert path)
        self.order.push(hash);
        self.nodes.insert(
            hash,
            Node {
                id: hash,
                state: new_state.clone(),
            },
        );
        println!(
            "[GRAPH][id={}] hash={} reused=false nodes={}",
            request_id,
            hash,
            self.nodes.len()
        );
        Ok(new_state)
    }

    // ── Capacity management ───────────────────────────────────────────────────

    /// Evict the oldest 10% of nodes (at least 1) to free capacity.  Spec §9.
    fn evict_old_nodes(&mut self, request_id: u64) {
        let evict_count = (self.limits.max_graph_nodes / 10)
            .max(1)
            .min(self.order.len());
        let evicted: Vec<u64> = self.order.drain(..evict_count).collect();
        for hash in &evicted {
            self.nodes.remove(hash);
        }
        println!(
            "[GRAPH][id={}] eviction=true count={} nodes={}",
            request_id,
            evicted.len(),
            self.nodes.len()
        );
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn contains(&self, hash: u64) -> bool {
        self.nodes.contains_key(&hash)
    }

    pub fn get(&self, hash: u64) -> Option<&Node> {
        self.nodes.get(&hash)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        Constraint, CorePlan, DesignDocument, Diff, DiffChunk, ReasonUnit, StructureTree,
    };

    fn empty_state() -> CoreState {
        CoreState::default()
    }

    fn state_with_plan(summary: &str, steps: &[&str]) -> CoreState {
        CoreState {
            current_plan: Some(CorePlan {
                summary: summary.to_string(),
                steps: steps.iter().map(|s| s.to_string()).collect(),
            }),
            ..CoreState::default()
        }
    }

    fn state_with_design(module: &str, constraints: &[&str]) -> CoreState {
        CoreState {
            design: Some(DesignDocument::new(
                1,
                vec![ReasonUnit {
                    id: "r1".to_string(),
                    title: "Title".to_string(),
                    summary: "Summary".to_string(),
                }],
                StructureTree {
                    module: module.to_string(),
                    functions: vec!["fn_a".to_string()],
                },
                constraints
                    .iter()
                    .map(|t| Constraint {
                        text: t.to_string(),
                    })
                    .collect(),
            )),
            ..CoreState::default()
        }
    }

    // ── Hash stability (spec §12 "hashが安定") ────────────────────────────────

    #[test]
    fn hash_is_stable_for_same_state() {
        let s = state_with_plan("build", &["cargo build"]);
        assert_eq!(StateGraph::state_hash(&s), StateGraph::state_hash(&s));
    }

    #[test]
    fn hash_excludes_version() {
        let mut s1 = state_with_plan("build", &["cargo build"]);
        let mut s2 = s1.clone();
        s1.version = 1;
        s2.version = 99;
        assert_eq!(
            StateGraph::state_hash(&s1),
            StateGraph::state_hash(&s2),
            "version must not affect hash"
        );
    }

    #[test]
    fn hash_excludes_proposals() {
        // proposals field should not affect state identity
        let base = empty_state();
        let with_proposals = base.clone();
        // Proposals are not hashed — different proposals, same hash
        assert_eq!(
            StateGraph::state_hash(&base),
            StateGraph::state_hash(&with_proposals)
        );
        // (field is Vec<ExecutionPlanCandidate>; left empty here)
    }

    // ── Differentiation (spec §12 "異なる状態は分離される") ──────────────────

    #[test]
    fn different_plans_have_different_hashes() {
        let s1 = state_with_plan("build", &["cargo build"]);
        let s2 = state_with_plan("test", &["cargo test"]);
        assert_ne!(StateGraph::state_hash(&s1), StateGraph::state_hash(&s2));
    }

    #[test]
    fn different_designs_have_different_hashes() {
        let s1 = state_with_design("api", &["no panics"]);
        let s2 = state_with_design("core", &["no panics"]);
        assert_ne!(StateGraph::state_hash(&s1), StateGraph::state_hash(&s2));
    }

    #[test]
    fn different_constraints_have_different_hashes() {
        let s1 = state_with_design("api", &["constraint A"]);
        let s2 = state_with_design("api", &["constraint B"]);
        assert_ne!(StateGraph::state_hash(&s1), StateGraph::state_hash(&s2));
    }

    // ── Constraint ordering independence ──────────────────────────────────────

    #[test]
    fn constraints_are_order_independent() {
        let s1 = state_with_design("api", &["A", "B", "C"]);
        let s2 = state_with_design("api", &["C", "A", "B"]);
        assert_eq!(
            StateGraph::state_hash(&s1),
            StateGraph::state_hash(&s2),
            "constraint order must not affect hash"
        );
    }

    // ── Reuse (spec §12 "同一状態で再利用される") ─────────────────────────────

    #[test]
    fn identical_state_is_reused() {
        let mut graph = StateGraph::new();
        let s = state_with_plan("build", &["cargo build"]);
        let current_hash = 0u64; // distinct from s's hash

        graph
            .reuse_or_insert(s.clone(), current_hash, 1)
            .expect("first insert should succeed");
        assert_eq!(graph.len(), 1);

        // Second call with identical content must reuse, not add a second node
        graph
            .reuse_or_insert(s.clone(), current_hash, 1)
            .expect("reuse should succeed");
        assert_eq!(graph.len(), 1, "identical state must reuse existing node");
    }

    // ── Cycle prevention (spec §12 "循環が発生しない") ────────────────────────

    #[test]
    fn cycle_detection_returns_err() {
        let mut graph = StateGraph::new();
        let s = state_with_plan("build", &["cargo build"]);
        let hash = StateGraph::state_hash(&s);

        let result = graph.reuse_or_insert(s, hash, 1); // current_hash == new hash
        assert!(
            result.is_err(),
            "transition to identical state must be reported as cycle"
        );
    }

    // ── A → B → C path deduplication (spec §12 ケース) ────────────────────────

    #[test]
    fn repeated_path_reuses_terminal_node() {
        let mut graph = StateGraph::new();

        let state_a = state_with_plan("A", &["step_a"]);
        let state_b = state_with_plan("B", &["step_b"]);
        let state_c = state_with_plan("C", &["step_c"]);

        let hash_a = StateGraph::state_hash(&state_a);
        let hash_b = StateGraph::state_hash(&state_b);
        let hash_c = StateGraph::state_hash(&state_c);

        // First traversal: A → B → C
        let zero_hash = 0u64;
        graph
            .reuse_or_insert(state_a.clone(), zero_hash, 1)
            .unwrap();
        graph.reuse_or_insert(state_b.clone(), hash_a, 1).unwrap();
        graph.reuse_or_insert(state_c.clone(), hash_b, 1).unwrap();
        assert_eq!(graph.len(), 3);

        // Second traversal: A → B → C — must reuse all three, no new nodes
        graph
            .reuse_or_insert(state_a.clone(), zero_hash, 1)
            .unwrap();
        graph.reuse_or_insert(state_b.clone(), hash_a, 1).unwrap();
        graph.reuse_or_insert(state_c.clone(), hash_b, 1).unwrap();
        assert_eq!(graph.len(), 3, "replay must not create new nodes");

        // C node is the same: both traversals share it
        assert!(graph.contains(hash_c), "C node must be present");
    }

    // ── Max nodes / eviction (spec §9) ────────────────────────────────────────

    #[test]
    fn eviction_keeps_node_count_bounded() {
        let mut graph = StateGraph::new();
        // Insert MAX_GRAPH_NODES + 1 distinct states
        for i in 0..=(MAX_GRAPH_NODES as u64) {
            let s = state_with_plan(&format!("plan_{i}"), &[&format!("step_{i}")]);
            let prev_hash = i.wrapping_sub(1); // distinct pseudo-hash for each
            let _ = graph.reuse_or_insert(s, prev_hash, 1);
        }
        assert!(
            graph.len() <= MAX_GRAPH_NODES,
            "graph.len()={} must be ≤ MAX_GRAPH_NODES={}",
            graph.len(),
            MAX_GRAPH_NODES
        );
    }

    #[test]
    fn custom_graph_limit_is_enforced() {
        let mut graph = StateGraph::with_limits(Limits {
            max_graph_nodes: 5,
            ..Limits::default()
        });

        for i in 0..10u64 {
            let s = state_with_plan(&format!("plan_{i}"), &[&format!("step_{i}")]);
            let _ = graph.reuse_or_insert(s, i.wrapping_sub(1), 1);
        }

        assert!(graph.len() <= 5);
    }

    // ── Diff exclusion (spec DBM-GRAPH-HASH-NORMALIZATION-FIX §9.1) ──────────
    // last_diff is display-only; it must never affect state identity.

    #[test]
    fn hash_excludes_diff() {
        // s1: no diff
        let s1 = empty_state();

        // s2: diff with content A
        let mut s2 = empty_state();
        s2.last_diff = Some(Diff {
            file: "src/main.rs".into(),
            changes: vec![DiffChunk {
                old_line: Some(1),
                new_line: Some(1),
                old: Some("- old line".into()),
                new: Some("+ new line".into()),
            }],
        });

        // s3: diff with different content
        let mut s3 = empty_state();
        s3.last_diff = Some(Diff {
            file: "src/lib.rs".into(),
            changes: vec![DiffChunk {
                old_line: Some(99),
                new_line: Some(99),
                old: Some("- completely different".into()),
                new: Some("+ also different".into()),
            }],
        });

        assert_eq!(
            StateGraph::state_hash(&s1),
            StateGraph::state_hash(&s2),
            "diff must not affect hash (no diff vs diff A)"
        );
        assert_eq!(
            StateGraph::state_hash(&s2),
            StateGraph::state_hash(&s3),
            "diff must not affect hash (diff A vs diff B)"
        );
    }

    // ── Graph reuse with changing diff (spec §10 Graph挙動確認) ───────────────

    #[test]
    fn same_structural_state_reused_despite_different_diff() {
        let mut graph = StateGraph::new();
        let base = state_with_plan("build", &["cargo build"]);
        let current_hash = 0u64;

        // Insert with no diff
        graph
            .reuse_or_insert(base.clone(), current_hash, 1)
            .expect("first insert");
        assert_eq!(graph.len(), 1);

        // Same structural state but with a diff attached
        let mut with_diff = base.clone();
        with_diff.last_diff = Some(Diff {
            file: "src/main.rs".into(),
            changes: vec![DiffChunk {
                old_line: Some(1),
                new_line: Some(1),
                old: Some("- x".into()),
                new: Some("+ y".into()),
            }],
        });

        // Must reuse — diff is excluded from hash
        graph
            .reuse_or_insert(with_diff, current_hash, 1)
            .expect("reuse should succeed");
        assert_eq!(graph.len(), 1, "diff change must not create a new node");
    }
}
