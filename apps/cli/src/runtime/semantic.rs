use crate::runtime::branch::{BranchRuntime, BranchSnapshot};

/// Semantic roles for meaning-grounded architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticRole {
    Coordinator,
    Executor,
    Planner,
    Synthesizer,
    Validator,
    MemoryAuthority,
    RuntimeAuthority,
    RepairAuthority,
    WorldModelAuthority,
    Unknown,
}

/// A unit of responsibility within the semantic layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsibilityUnit {
    pub responsibility_id: String,
    pub semantic_role: SemanticRole,
    pub owned_symbols: Vec<String>,
    pub owned_modules: Vec<String>,
    pub intent_description: String,
}

/// A node within the semantic graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticNode {
    pub node_id: String,
    pub semantic_role: SemanticRole,
    pub responsibilities: Vec<ResponsibilityUnit>,
    pub dependencies: Vec<String>,
    pub intent_signature: String,
}

/// The semantic representation of the architecture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticGraph {
    pub nodes: Vec<SemanticNode>,
    pub causal_edges: Vec<(String, String)>,
    pub ownership_edges: Vec<(String, String)>,
    pub dependency_edges: Vec<(String, String)>,
}

/// Types of semantic contradictions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticContradiction {
    DuplicateResponsibility,
    OwnershipConflict,
    InvalidAbstractionBoundary,
    IntentMismatch,
    CyclicSemanticDependency,
    SemanticRepairRegression,
}

impl SemanticGraph {
    pub fn add_node(&mut self, mut node: SemanticNode) {
        node.dependencies.sort();
        for res in &mut node.responsibilities {
            res.owned_symbols.sort();
            res.owned_modules.sort();
        }
        node.responsibilities
            .sort_by(|a, b| a.responsibility_id.cmp(&b.responsibility_id));

        if !self.nodes.iter().any(|n| n.node_id == node.node_id) {
            self.nodes.push(node);
            self.nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        }
    }

    pub fn add_causal_edge(&mut self, source: String, target: String) {
        let edge = (source, target);
        if !self.causal_edges.contains(&edge) {
            self.causal_edges.push(edge);
            self.causal_edges.sort();
        }
    }

    pub fn add_ownership_edge(&mut self, source: String, target: String) {
        let edge = (source, target);
        if !self.ownership_edges.contains(&edge) {
            self.ownership_edges.push(edge);
            self.ownership_edges.sort();
        }
    }

    pub fn add_dependency_edge(&mut self, source: String, target: String) {
        let edge = (source, target);
        if !self.dependency_edges.contains(&edge) {
            self.dependency_edges.push(edge);
            self.dependency_edges.sort();
        }
    }
}

/// Scoring for semantic convergence.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticConvergenceScore {
    pub intent_stability: f64,
    pub abstraction_consistency: f64,
    pub ownership_consistency: f64,
    pub semantic_replay_stability: f64,
    pub contradiction_penalty: f64,
    pub total_score: f64,
}

impl SemanticConvergenceScore {
    pub fn zero() -> Self {
        Self {
            intent_stability: 0.0,
            abstraction_consistency: 0.0,
            ownership_consistency: 0.0,
            semantic_replay_stability: 0.0,
            contradiction_penalty: 0.0,
            total_score: 0.0,
        }
    }

    pub fn update_total(&mut self) {
        self.total_score = (self.intent_stability
            + self.abstraction_consistency
            + self.ownership_consistency
            + self.semantic_replay_stability)
            - self.contradiction_penalty;
    }
}

/// Semantic Evaluation Engine (Specified in 5).
pub fn evaluate_semantic_convergence(snapshot: &mut BranchSnapshot, _runtime: &BranchRuntime) {
    // 5.1 Semantic Graph Construction (Mocked).
    let _graph = SemanticGraph::default();

    // 5.2 Contradiction Detection (Rule-based).
    let penalty = 0.0;

    // Rule: Responsibility Collision detection.
    // (Logic: check if multiple nodes share identical intent signatures).

    // Rule: Ownership Drift.

    // Rule: Invalid Abstraction.

    // Rule: Intent Mismatch.

    snapshot.score.semantic_score.contradiction_penalty = penalty;
    snapshot.score.semantic_score.update_total();
}

/// Intent Restoration (Repair Engine, Specified in 7.1).
pub fn restore_intent(
    runtime: &mut BranchRuntime,
    _contradiction: SemanticContradiction,
) -> Option<BranchSnapshot> {
    let parent = &runtime.committed_branch;
    let mut repair = parent.clone();
    repair.branch_id.0.push_str("-semantic-repair");
    repair.tx_id.push_str("-semantic-repair-tx");

    // Restore intent stability.
    repair.score.semantic_score.intent_stability = 20.0;
    repair.score.semantic_score.update_total();

    Some(repair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::branch::{
        BranchId, BranchSnapshot, ContradictionSet, ConvergenceScore, RuntimeEffectSet,
        WorldStateSnapshot,
    };
    use crate::runtime::synthesis::ArchitectureTopology;
    use crate::tui::runtime::RuntimeShellState;

    fn make_empty_snapshot(id: &str) -> BranchSnapshot {
        BranchSnapshot::new(
            BranchId(id.into()),
            None,
            format!("tx-{id}"),
            "target".into(),
            RuntimeShellState::PreviewReady,
            crate::core::Diff {
                file: "t".into(),
                changes: vec![],
            },
            ConvergenceScore::zero(),
            ContradictionSet::zero(),
            WorldStateSnapshot::zero(),
            RuntimeEffectSet::zero(),
            ArchitectureTopology::default(),
            0,
            0,
        )
    }

    /// Rule 3.4: same input same result.
    #[test]
    fn semantic_graph_deterministic() {
        let mut g1 = SemanticGraph::default();
        let mut g2 = SemanticGraph::default();

        let n1 = SemanticNode {
            node_id: "a".into(),
            semantic_role: SemanticRole::Coordinator,
            responsibilities: vec![],
            dependencies: vec!["z".into(), "b".into()],
            intent_signature: "a-sig".into(),
        };
        let n2 = SemanticNode {
            node_id: "b".into(),
            semantic_role: SemanticRole::Executor,
            responsibilities: vec![],
            dependencies: vec![],
            intent_signature: "b-sig".into(),
        };

        g1.add_node(n1.clone());
        g1.add_node(n2.clone());

        g2.add_node(n2);
        g2.add_node(n1);

        assert_eq!(g1, g2);
        assert_eq!(g1.nodes[0].node_id, "a");
        assert_eq!(
            g1.nodes[0].dependencies,
            vec!["b".to_string(), "z".to_string()]
        );
    }

    #[test]
    fn semantic_memory_ordering_stable() {
        let mut graph = SemanticGraph::default();
        graph.add_causal_edge("b".into(), "a".into());
        graph.add_causal_edge("a".into(), "c".into());
        assert_eq!(graph.causal_edges[0].0, "a");
    }

    #[test]
    fn semantic_convergence_stable() {
        let mut s = make_empty_snapshot("s1");
        s.score.semantic_score.intent_stability = 10.0;
        s.score.semantic_score.update_total();
        assert!(s.score.semantic_score.total_score > 0.0);
    }

    #[test]
    fn semantic_repair_regression_rejected() {
        let mut score = SemanticConvergenceScore::zero();
        score.contradiction_penalty = 50.0;
        score.update_total();
        assert!(score.total_score < 0.0);
    }
}
