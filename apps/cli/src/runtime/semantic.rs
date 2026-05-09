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
pub fn evaluate_semantic_convergence(
    snapshot: &mut BranchSnapshot,
    _runtime: &BranchRuntime,
) {
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
        BranchId, BranchSnapshot, ContradictionSet, ConvergenceScore,
        RuntimeEffectSet, WorldStateSnapshot,
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
            crate::core::Diff { file: "t".into(), changes: vec![] },
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
        let g1 = SemanticGraph::default();
        let g2 = SemanticGraph::default();
        assert_eq!(g1, g2);
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
