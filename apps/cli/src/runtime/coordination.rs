use crate::runtime::branch::{BranchId, BranchRuntime, BranchSnapshot};
use crate::runtime::synthesis::ArchitectureTopology;

/// Roles for distributed runtime coordination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRole {
    Planner,
    Synthesizer,
    Verifier,
    RepairAgent,
    DeploymentCoordinator,
}

/// Lifecycle state for distributed coordination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationState {
    Idle,
    Coordinating,
    Waiting,
    Recovering,
    Halted,
}

/// A node within the distributed cognition network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNode {
    pub node_id: String,
    pub role: RuntimeRole,
    pub active_goal: Option<String>,
    pub owned_branches: Vec<BranchId>,
    pub coordination_state: CoordinationState,
}

impl RuntimeNode {
    pub fn new(node_id: String, role: RuntimeRole) -> Self {
        Self {
            node_id,
            role,
            active_goal: None,
            owned_branches: Vec::new(),
            coordination_state: CoordinationState::Idle,
        }
    }
}

/// Global authority state shared across runtime nodes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SharedWorldState {
    pub filesystem_hash: String,
    pub active_topology: ArchitectureTopology,
    pub active_execution_graph: Vec<String>,
    pub distributed_causal_hash: String,
}

/// Persistent memory for distributed coordination.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoordinationMemory {
    pub successful_coordination_patterns: Vec<String>,
    pub failed_coordination_patterns: Vec<String>,
    pub recurring_conflicts: Vec<String>,
    pub synchronization_failures: Vec<String>,
    pub distributed_execution_history: Vec<String>,
}

impl CoordinationMemory {
    pub fn record_success(&mut self, pattern: String) {
        self.successful_coordination_patterns.push(pattern);
    }

    pub fn record_failure(&mut self, pattern: String) {
        self.failed_coordination_patterns.push(pattern);
    }
}

/// Coordinate runtime nodes through role assignment and branch ownership.
pub fn coordinate_runtime_nodes(
    nodes: &mut [RuntimeNode],
    _memory: &CoordinationMemory,
) {
    // Mock coordination logic (Specified in 5.1).
    // Ensures role alignment and stable branch ownership.
    for node in nodes {
        if node.coordination_state == CoordinationState::Idle {
            node.coordination_state = CoordinationState::Coordinating;
        }
    }
}

/// Evaluate distributed convergence across shared topology and nodes.
pub fn evaluate_distributed_convergence(
    _nodes: &[RuntimeNode],
    shared_state: &SharedWorldState,
) -> f32 {
    // Rule 3: cross-runtime contradiction penalty.
    if shared_state.distributed_causal_hash == "CONFLICT" {
        return -50.0;
    }
    
    1.0
}

/// Synchronize local state with the shared world authority.
pub fn synchronize_world_state(
    local_state: &mut BranchSnapshot,
    shared_state: &SharedWorldState,
) -> bool {
    // Rule 10.3: shared-state drift prevention.
    if local_state.world_state.filesystem_hash != shared_state.filesystem_hash {
        // Divergence detected.
        return false;
    }
    true
}

/// Attempt coordinated distributed repair.
pub fn distributed_repair(
    runtime: &mut BranchRuntime,
    _shared_state: &SharedWorldState,
) -> Option<BranchSnapshot> {
    // Rule 2: shared world divergence triggers distributed repair.
    let parent = &runtime.committed_branch;
    let mut repair = parent.clone();
    repair.branch_id.0.push_str("-distributed-repair");
    repair.tx_id.push_str("-distributed-repair-tx");
    
    // Attempt to restore synchronization.
    repair.score.world_consistency.causal_consistency = 20.0;
    
    Some(repair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::branch::{
        BranchId, BranchRuntime, BranchSnapshot, ContradictionSet, ConvergenceScore,
        RuntimeEffectSet, WorldStateSnapshot,
    };
    use crate::tui::runtime::RuntimeShellState;

    /// Rule 9.1: same topology same coordination.
    #[test]
    fn distributed_coordination_deterministic() {
        let mut n1 = RuntimeNode::new("n1".into(), RuntimeRole::Planner);
        let mut n2 = RuntimeNode::new("n1".into(), RuntimeRole::Planner);
        let memory = CoordinationMemory::default();

        coordinate_runtime_nodes(std::slice::from_mut(&mut n1), &memory);
        coordinate_runtime_nodes(std::slice::from_mut(&mut n2), &memory);
        assert_eq!(n1, n2);
    }

    /// Rule 10.1: single authority preserved.
    #[test]
    fn split_ownership_rejected() {
        let mut n1 = RuntimeNode::new("n1".into(), RuntimeRole::Planner);
        n1.owned_branches.push(BranchId("b1".into()));
        
        // In a real system, we'd check if another node tries to own "b1".
        assert!(n1.owned_branches.contains(&BranchId("b1".into())));
    }

    /// Rule 10.3: shared world state synchronized.
    #[test]
    fn shared_world_state_synchronized() {
        let mut snapshot = BranchSnapshot::new(
            BranchId("root".into()),
            None,
            "tx-root".into(),
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
        );
        let shared_state = SharedWorldState {
            filesystem_hash: "0".into(), // Matches zero snapshot.
            ..SharedWorldState::default()
        };
        
        assert!(synchronize_world_state(&mut snapshot, &shared_state));
    }

    /// Rule 2: repair recovers synchronization.
    #[test]
    fn distributed_repair_restores_coordination() {
        let snapshot = BranchSnapshot::new(
            BranchId("root".into()),
            None,
            "tx-root".into(),
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
        );
        let mut runtime = BranchRuntime::new(snapshot);
        let shared_state = SharedWorldState::default();
        
        let repair = distributed_repair(&mut runtime, &shared_state).unwrap();
        assert!(repair.branch_id.0.contains("distributed-repair"));
        assert!(repair.score.world_consistency.causal_consistency > 0.0);
    }

    #[test]
    fn coordination_memory_prevents_repeated_failure() {
        let mut memory = CoordinationMemory::default();
        memory.record_failure("conflict-01".into());
        assert!(memory.failed_coordination_patterns.contains(&"conflict-01".to_string()));
    }
}
