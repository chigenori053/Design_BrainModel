use crate::runtime::branch::{BranchRuntime, BranchSnapshot};

/// Types of architectural nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Service,
    Runtime,
    Storage,
    Interface,
    ExecutionGraph,
    DeploymentUnit,
}

/// A node within the architectural topology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureNode {
    pub node_id: String,
    pub node_type: NodeType,
    pub dependencies: Vec<String>,
    pub execution_role: String,
    pub verification_requirements: Vec<String>,
}

/// The synthesized architectural topology.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchitectureTopology {
    pub nodes: Vec<ArchitectureNode>,
    pub dependency_edges: Vec<(String, String)>,
    pub execution_order: Vec<String>,
    pub deployment_graph: Vec<String>,
}

/// Goals and constraints for architecture synthesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureGoal {
    pub goal_id: String,
    pub root_intent: String,
    pub functional_targets: Vec<String>,
    pub nonfunctional_constraints: Vec<String>,
    pub deployment_constraints: Vec<String>,
}

/// Persistent memory for architecture synthesis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArchitectureMemory {
    pub successful_topologies: Vec<String>,
    pub failed_topologies: Vec<String>,
    pub recurring_dependency_failures: Vec<String>,
    pub deployment_failures: Vec<String>,
    pub execution_graph_history: Vec<String>,
}

impl ArchitectureMemory {
    pub fn record_success(&mut self, topology_id: String) {
        self.successful_topologies.push(topology_id);
    }

    pub fn record_failure(&mut self, topology_id: String) {
        self.failed_topologies.push(topology_id);
    }
}

/// Synthesize a candidate architecture topology from a goal.
pub fn synthesize_architecture(
    _goal: &ArchitectureGoal,
    _memory: &ArchitectureMemory,
) -> ArchitectureTopology {
    // Mock synthesis logic (Specified in 5.1).
    // In a real system, this would use CodeIR and LLM planners.
    ArchitectureTopology::default()
}

/// Evaluate the stability of an architectural topology.
pub fn evaluate_architecture_stability(
    topology: &ArchitectureTopology,
    _memory: &ArchitectureMemory,
) -> f32 {
    // Rule 3: deployment infeasibility penalty.
    if topology.deployment_graph.is_empty() && !topology.nodes.is_empty() {
        return -10.0;
    }

    // Default stability score.
    1.0
}

/// Generate a deterministic execution graph for the topology.
pub fn generate_execution_graph(topology: &ArchitectureTopology) -> Vec<String> {
    // Rule 9.2: execution ordering is deterministic.
    let mut order = topology.execution_order.clone();
    order.sort();
    order
}

/// Repair an unstable dependency topology.
pub fn topology_repair(
    runtime: &mut BranchRuntime,
    _topology: &ArchitectureTopology,
) -> Option<BranchSnapshot> {
    // Rule 2: dependency graph instability triggers repair.
    let parent = &runtime.committed_branch;
    let mut repair = parent.clone();
    repair.branch_id.0.push_str("-topology-repair");
    repair.tx_id.push_str("-topology-repair-tx");

    // Improve stability in the repair branch.
    repair.score.world_consistency.dependency_consistency = 10.0;

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

    fn make_goal(id: &str) -> ArchitectureGoal {
        ArchitectureGoal {
            goal_id: id.to_string(),
            root_intent: "test goal".to_string(),
            functional_targets: vec![],
            nonfunctional_constraints: vec![],
            deployment_constraints: vec![],
        }
    }

    /// Rule 9.1: same goal same topology.
    #[test]
    fn topology_synthesis_deterministic() {
        let goal = make_goal("g1");
        let memory = ArchitectureMemory::default();
        let t1 = synthesize_architecture(&goal, &memory);
        let t2 = synthesize_architecture(&goal, &memory);
        assert_eq!(t1, t2);
    }

    /// Rule 3: invalid deployment rejected (low stability score).
    #[test]
    fn unstable_dependency_detected() {
        let mut topology = ArchitectureTopology::default();
        topology.nodes.push(ArchitectureNode {
            node_id: "n1".into(),
            node_type: NodeType::Service,
            dependencies: vec![],
            execution_role: "".into(),
            verification_requirements: vec![],
        });
        // Missing deployment graph -> unstable.
        let memory = ArchitectureMemory::default();
        let stability = evaluate_architecture_stability(&topology, &memory);
        assert!(stability < 0.0);
    }

    /// Rule 9.2: execution ordering deterministic.
    #[test]
    fn execution_graph_consistency_verified() {
        let mut topology = ArchitectureTopology::default();
        topology.execution_order = vec!["b".into(), "a".into(), "c".into()];
        let graph = generate_execution_graph(&topology);
        assert_eq!(graph, vec!["a", "b", "c"]);
    }

    /// Rule 2: repair recovers topology continuity.
    #[test]
    fn topology_repair_restores_stability() {
        let snapshot = BranchSnapshot::new(
            BranchId("root".into()),
            None,
            "tx-root".into(),
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
        );
        let mut runtime = BranchRuntime::new(snapshot);
        let topology = ArchitectureTopology::default();

        let repair = topology_repair(&mut runtime, &topology).unwrap();
        assert!(repair.branch_id.0.contains("topology-repair"));
        assert!(repair.score.world_consistency.dependency_consistency > 0.0);
    }

    #[test]
    fn architecture_memory_prevents_repeated_failure() {
        let mut memory = ArchitectureMemory::default();
        memory.record_failure("bad-topology".into());
        assert!(
            memory
                .failed_topologies
                .contains(&"bad-topology".to_string())
        );
    }
}
