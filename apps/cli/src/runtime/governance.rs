use crate::runtime::branch::{BranchRuntime, BranchSnapshot};
use crate::runtime::coordination::{
    RuntimeNode, SharedWorldState, evaluate_distributed_convergence,
};
use crate::tui::runtime::RuntimeShellState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitivePolicy {
    pub policy_id: String,
    pub reasoning_depth_limit: usize,
    pub branch_budget_limit: usize,
    pub repair_attempt_limit: usize,
    pub semantic_contradiction_threshold: usize,
    pub distributed_coordination_limit: usize,
    pub self_modification_limit: usize,
}

impl Default for CognitivePolicy {
    fn default() -> Self {
        Self {
            policy_id: "dbm-meta-cognitive-governance-v1".to_string(),
            reasoning_depth_limit: 3,
            branch_budget_limit: 10,
            repair_attempt_limit: 3,
            semantic_contradiction_threshold: 2,
            distributed_coordination_limit: 3,
            self_modification_limit: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceState {
    Stable,
    Supervising,
    Restricting,
    Recovering,
    Halted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CognitiveObservation {
    pub runtime_id: String,
    pub active_goal: String,
    pub convergence_score: f64,
    pub semantic_stability: f64,
    pub contradiction_count: usize,
    pub repair_cycles: usize,
    pub coordination_divergence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GovernanceMemory {
    pub stable_policies: Vec<String>,
    pub unstable_policies: Vec<String>,
    pub governance_failures: Vec<String>,
    pub policy_mutation_history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GovernanceEvaluation {
    pub state: GovernanceState,
    pub halt_state: Option<RuntimeShellState>,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceMemoryEvent {
    StablePolicy,
    UnstablePolicy,
    GovernanceFailure,
    PolicyMutation,
}

pub fn commit_memory_event(
    memory: &mut GovernanceMemory,
    event: GovernanceMemoryEvent,
    value: String,
) {
    let target = match event {
        GovernanceMemoryEvent::StablePolicy => &mut memory.stable_policies,
        GovernanceMemoryEvent::UnstablePolicy => &mut memory.unstable_policies,
        GovernanceMemoryEvent::GovernanceFailure => &mut memory.governance_failures,
        GovernanceMemoryEvent::PolicyMutation => &mut memory.policy_mutation_history,
    };
    if !target.contains(&value) {
        target.push(value);
        target.sort();
    }
}

pub fn observe_cognition(
    runtime_id: &str,
    active_goal: &str,
    snapshot: &BranchSnapshot,
    runtime: &BranchRuntime,
    nodes: &[RuntimeNode],
    shared_state: &SharedWorldState,
) -> CognitiveObservation {
    let distributed_score = evaluate_distributed_convergence(nodes, shared_state);
    let distributed_divergence = if distributed_score < 0.0 {
        distributed_score.abs() as usize
    } else {
        0
    };

    CognitiveObservation {
        runtime_id: runtime_id.to_string(),
        active_goal: active_goal.to_string(),
        convergence_score: snapshot.score.total() as f64,
        semantic_stability: snapshot.score.semantic_score.total_score,
        contradiction_count: snapshot.contradictions.total()
            + usize::from(snapshot.score.semantic_score.contradiction_penalty > 0.0),
        repair_cycles: snapshot.depth,
        coordination_divergence: runtime.speculative_branches.len() + distributed_divergence,
    }
}

pub fn evaluate_governance_stability(
    policy: &CognitivePolicy,
    observation: &CognitiveObservation,
    memory: &GovernanceMemory,
) -> GovernanceEvaluation {
    if policy.self_modification_limit == 0 {
        return halted(
            RuntimeShellState::PolicyMutationHalt,
            "self modification limit exhausted",
        );
    }

    if memory.unstable_policies.contains(&policy.policy_id) {
        return halted(
            RuntimeShellState::GovernanceCollapseHalt,
            "policy is recorded as unstable",
        );
    }

    if observation.repair_cycles > policy.reasoning_depth_limit
        || observation.coordination_divergence > policy.branch_budget_limit
    {
        return halted(
            RuntimeShellState::RunawayCognitionHalt,
            "reasoning depth or branch pressure exceeded policy bounds",
        );
    }

    if observation.contradiction_count > policy.semantic_contradiction_threshold {
        return GovernanceEvaluation {
            state: GovernanceState::Restricting,
            halt_state: Some(RuntimeShellState::SemanticGovernanceHalt),
            explanation: "semantic contradiction threshold exceeded".to_string(),
        };
    }

    if observation.coordination_divergence > policy.distributed_coordination_limit {
        return GovernanceEvaluation {
            state: GovernanceState::Restricting,
            halt_state: Some(RuntimeShellState::GovernanceCollapseHalt),
            explanation: "distributed coordination divergence exceeded policy bounds".to_string(),
        };
    }

    if observation.semantic_stability < -50.0 {
        return GovernanceEvaluation {
            state: GovernanceState::Recovering,
            halt_state: Some(RuntimeShellState::SemanticGovernanceHalt),
            explanation: "semantic stability collapsed".to_string(),
        };
    }

    if observation.convergence_score < 0.0 || observation.semantic_stability < 0.0 {
        return GovernanceEvaluation {
            state: GovernanceState::Supervising,
            halt_state: None,
            explanation: "cognition requires supervision".to_string(),
        };
    }

    GovernanceEvaluation {
        state: GovernanceState::Stable,
        halt_state: None,
        explanation: "governance stable".to_string(),
    }
}

pub fn restrict_cognition(policy: &mut CognitivePolicy, runtime: &mut BranchRuntime) {
    policy.reasoning_depth_limit = policy.reasoning_depth_limit.saturating_sub(1).max(1);
    policy.branch_budget_limit = policy.branch_budget_limit.saturating_sub(1).max(1);
    policy.repair_attempt_limit = policy.repair_attempt_limit.saturating_sub(1);
    policy.distributed_coordination_limit = policy.distributed_coordination_limit.saturating_sub(1);
    runtime.budget.remaining_depth = runtime
        .budget
        .remaining_depth
        .min(policy.reasoning_depth_limit);
    runtime.budget.remaining_branches = runtime
        .budget
        .remaining_branches
        .min(policy.branch_budget_limit);
    runtime.budget.max_active_branches = runtime
        .budget
        .max_active_branches
        .min(policy.branch_budget_limit);
}

pub fn rollback_policy(memory: &mut GovernanceMemory, policy: &mut CognitivePolicy) {
    commit_memory_event(
        memory,
        GovernanceMemoryEvent::UnstablePolicy,
        policy.policy_id.clone(),
    );
    *policy = CognitivePolicy::default();
    commit_memory_event(
        memory,
        GovernanceMemoryEvent::StablePolicy,
        policy.policy_id.clone(),
    );
}

pub fn mutate_policy(
    policy: &CognitivePolicy,
    memory: &mut GovernanceMemory,
    observation: &CognitiveObservation,
) -> Option<CognitivePolicy> {
    if policy.self_modification_limit == 0 {
        commit_memory_event(
            memory,
            GovernanceMemoryEvent::GovernanceFailure,
            format!("{}:mutation-limit-exhausted", policy.policy_id),
        );
        return None;
    }

    let mut mutated = policy.clone();
    mutated.policy_id = format!(
        "{}:m{}",
        policy.policy_id,
        memory.policy_mutation_history.len() + 1
    );
    mutated.self_modification_limit -= 1;

    if observation.contradiction_count > 0 {
        mutated.semantic_contradiction_threshold = mutated
            .semantic_contradiction_threshold
            .saturating_sub(1)
            .max(1);
    } else {
        mutated.branch_budget_limit += 1;
    }

    commit_memory_event(
        memory,
        GovernanceMemoryEvent::PolicyMutation,
        format!(
            "{}->{}:contradictions={}:semantic={:.3}",
            policy.policy_id,
            mutated.policy_id,
            observation.contradiction_count,
            observation.semantic_stability
        ),
    );
    Some(mutated)
}

pub fn stable_governance_order(
    observations: &mut [CognitiveObservation],
) -> &[CognitiveObservation] {
    observations.sort_by(|a, b| {
        b.semantic_stability
            .partial_cmp(&a.semantic_stability)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.contradiction_count.cmp(&b.contradiction_count))
            .then_with(|| {
                b.convergence_score
                    .partial_cmp(&a.convergence_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.runtime_id.cmp(&b.runtime_id))
    });
    observations
}

fn halted(halt_state: RuntimeShellState, explanation: &str) -> GovernanceEvaluation {
    GovernanceEvaluation {
        state: GovernanceState::Halted,
        halt_state: Some(halt_state),
        explanation: explanation.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Diff;
    use crate::runtime::branch::{
        BranchId, BranchSnapshot, BranchSnapshotInput, ContradictionSet, ConvergenceScore,
        RuntimeEffectSet, WorldStateSnapshot,
    };
    use crate::runtime::synthesis::ArchitectureTopology;

    fn snapshot(id: &str) -> BranchSnapshot {
        BranchSnapshot::new(BranchSnapshotInput {
            branch_id: BranchId(id.to_string()),
            parent_branch: None,
            tx_id: format!("tx-{id}"),
            target: "target".to_string(),
            runtime_state: RuntimeShellState::PreviewReady,
            projection: Diff {
                file: "target".to_string(),
                changes: vec![],
            },
            score: ConvergenceScore::zero(),
            contradictions: ContradictionSet::zero(),
            world_state: WorldStateSnapshot::zero(),
            runtime_effects: RuntimeEffectSet::zero(),
            topology: ArchitectureTopology::default(),
            depth: 0,
            created_at: 0,
        })
    }

    fn observation() -> CognitiveObservation {
        CognitiveObservation {
            runtime_id: "runtime-a".to_string(),
            active_goal: "goal".to_string(),
            convergence_score: 1.0,
            semantic_stability: 1.0,
            contradiction_count: 0,
            repair_cycles: 0,
            coordination_divergence: 0,
        }
    }

    #[test]
    fn governance_deterministic() {
        let policy = CognitivePolicy::default();
        let memory = GovernanceMemory::default();
        let obs = observation();
        let first = evaluate_governance_stability(&policy, &obs, &memory);
        let second = evaluate_governance_stability(&policy, &obs, &memory);
        assert_eq!(first, second);
    }

    #[test]
    fn governance_evaluation_is_pure() {
        let policy = CognitivePolicy::default();
        let memory = GovernanceMemory::default();
        let obs = observation();
        let before = (policy.clone(), memory.clone(), obs.clone());

        let _ = evaluate_governance_stability(&policy, &obs, &memory);

        assert_eq!(before, (policy, memory, obs));
    }

    #[test]
    fn runaway_cognition_detected() {
        let policy = CognitivePolicy {
            reasoning_depth_limit: 1,
            ..CognitivePolicy::default()
        };
        let mut obs = observation();
        obs.repair_cycles = 2;
        let eval = evaluate_governance_stability(&policy, &obs, &GovernanceMemory::default());
        assert_eq!(
            eval.halt_state,
            Some(RuntimeShellState::RunawayCognitionHalt)
        );
    }

    #[test]
    fn policy_mutation_bounded() {
        let policy = CognitivePolicy {
            self_modification_limit: 1,
            ..CognitivePolicy::default()
        };
        let mut memory = GovernanceMemory::default();
        let mutated = mutate_policy(&policy, &mut memory, &observation()).unwrap();
        assert_eq!(mutated.self_modification_limit, 0);
        assert!(mutate_policy(&mutated, &mut memory, &observation()).is_none());
    }

    #[test]
    fn semantic_governance_stable() {
        let policy = CognitivePolicy::default();
        let obs = observation();
        let eval = evaluate_governance_stability(&policy, &obs, &GovernanceMemory::default());
        assert_eq!(eval.state, GovernanceState::Stable);
        assert!(eval.halt_state.is_none());
    }

    #[test]
    fn governance_rollback_restores_stability() {
        let mut policy = CognitivePolicy {
            policy_id: "unstable".to_string(),
            reasoning_depth_limit: 0,
            ..CognitivePolicy::default()
        };
        let mut memory = GovernanceMemory::default();
        rollback_policy(&mut memory, &mut policy);
        assert_eq!(policy, CognitivePolicy::default());
        assert!(memory.unstable_policies.contains(&"unstable".to_string()));
        assert!(
            memory
                .stable_policies
                .contains(&"dbm-meta-cognitive-governance-v1".to_string())
        );
    }

    #[test]
    fn distributed_governance_consistent() {
        let committed = snapshot("root");
        let runtime = BranchRuntime::new(committed.clone());
        let node = RuntimeNode::new(
            "runtime-a".to_string(),
            crate::runtime::coordination::RuntimeRole::Planner,
        );
        let shared = SharedWorldState::default();

        let first = observe_cognition(
            "runtime-a",
            "goal",
            &committed,
            &runtime,
            std::slice::from_ref(&node),
            &shared,
        );
        let second = observe_cognition("runtime-a", "goal", &committed, &runtime, &[node], &shared);
        assert_eq!(first, second);
    }

    #[test]
    fn governance_replay_order_stable() {
        let mut observations = vec![
            CognitiveObservation {
                runtime_id: "b".to_string(),
                semantic_stability: 2.0,
                contradiction_count: 0,
                convergence_score: 1.0,
                ..observation()
            },
            CognitiveObservation {
                runtime_id: "a".to_string(),
                semantic_stability: 2.0,
                contradiction_count: 0,
                convergence_score: 1.0,
                ..observation()
            },
        ];
        stable_governance_order(&mut observations);
        assert_eq!(observations[0].runtime_id, "a");
    }

    #[test]
    fn governance_memory_ordering_deterministic() {
        let mut first = GovernanceMemory::default();
        let mut second = GovernanceMemory::default();

        for value in ["b", "a", "c", "a"] {
            commit_memory_event(
                &mut first,
                GovernanceMemoryEvent::GovernanceFailure,
                value.to_string(),
            );
        }
        for value in ["c", "b", "a", "a"] {
            commit_memory_event(
                &mut second,
                GovernanceMemoryEvent::GovernanceFailure,
                value.to_string(),
            );
        }

        assert_eq!(first.governance_failures, vec!["a", "b", "c"]);
        assert_eq!(first, second);
    }
}
