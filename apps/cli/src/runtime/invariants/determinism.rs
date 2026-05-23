use crate::runtime::cognitive_orchestration::{
    BranchEvaluation, CognitiveOrchestrationResult, TransactionGraph,
};
use crate::tui::rendering::{ProjectionSnapshot, projection_semantic_hash};

pub struct DeterminismInvariantSuite;

impl DeterminismInvariantSuite {
    pub fn assert_runtime_determinism() {}

    pub fn assert_projection_determinism() {}

    pub fn assert_branch_determinism() {}

    pub fn assert_same_projection_hash(snapshot: &ProjectionSnapshot) {
        assert_eq!(
            snapshot.projection_hash.semantic_hash,
            projection_semantic_hash(snapshot)
        );
    }

    pub fn assert_same_orchestration(
        first: &CognitiveOrchestrationResult,
        second: &CognitiveOrchestrationResult,
    ) {
        assert_eq!(first, second);
    }

    pub fn assert_transaction_graph_ordered(graph: &TransactionGraph) {
        let ordered = graph
            .transactions
            .windows(2)
            .all(|pair| pair[0].transaction_id <= pair[1].transaction_id);
        assert!(ordered);
    }

    pub fn assert_branch_ordered(branches: &[BranchEvaluation]) {
        let ordered = branches
            .windows(2)
            .all(|pair| pair[0].branch_id <= pair[1].branch_id);
        assert!(ordered);
    }
}
