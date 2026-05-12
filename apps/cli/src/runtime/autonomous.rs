use crate::runtime::branch::{BranchId, BranchRuntime, BranchSnapshot};

/// Lifecycle of an autonomous execution session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuityState {
    Active,
    Repairing,
    Recovering,
    Halted,
    Converged,
}

/// State of an active autonomous execution session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSession {
    pub session_id: String,
    pub root_goal: String,
    pub active_plan: String,
    pub current_branch: BranchId,
    pub execution_step: usize,
    pub failure_count: usize,
    pub repair_attempts: usize,
    pub continuity_state: ContinuityState,
}

impl ExecutionSession {
    pub fn new(session_id: String, goal: String, root_branch: BranchId) -> Self {
        Self {
            session_id,
            root_goal: goal,
            active_plan: String::new(),
            current_branch: root_branch,
            execution_step: 0,
            failure_count: 0,
            repair_attempts: 0,
            continuity_state: ContinuityState::Active,
        }
    }
}

/// Details of a specific autonomous repair attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct RepairCycle {
    pub failure_signature: String,
    pub attempted_repairs: usize,
    pub successful_repairs: usize,
    pub rollback_count: usize,
    pub verification_recovery_score: f32,
}

impl RepairCycle {
    pub fn new(signature: String) -> Self {
        Self {
            failure_signature: signature,
            attempted_repairs: 0,
            successful_repairs: 0,
            rollback_count: 0,
            verification_recovery_score: 0.0,
        }
    }
}

/// Persistent memory for grounding autonomous repairs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecutionMemory {
    pub successful_repairs: Vec<String>,
    pub failed_repairs: Vec<String>,
    pub recurring_failures: Vec<String>,
    pub convergence_history: Vec<f32>,
}

impl ExecutionMemory {
    pub fn record_success(&mut self, repair: String) {
        if !self.successful_repairs.contains(&repair) {
            self.successful_repairs.push(repair);
            self.successful_repairs.sort();
        }
    }

    pub fn record_failure(&mut self, repair: String) {
        if !self.failed_repairs.contains(&repair) {
            self.failed_repairs.push(repair);
            self.failed_repairs.sort();
        }
    }

    pub fn record_recurring_failure(&mut self, signature: String) {
        if !self.recurring_failures.contains(&signature) {
            self.recurring_failures.push(signature);
            self.recurring_failures.sort();
        }
    }
}

/// Detect various types of execution failures grounded in the world model.
pub fn detect_execution_failure(snapshot: &BranchSnapshot) -> Option<String> {
    if snapshot.runtime_effects.verification_failures > 0 {
        return Some(format!(
            "VERIFICATION_FAILURE:{}",
            snapshot.runtime_effects.verification_failures
        ));
    }

    if snapshot.score.world_consistency.causal_consistency < -50.0 {
        return Some("CAUSAL_INCONSISTENCY".to_string());
    }

    if snapshot.score.world_consistency.dependency_consistency < -5.0 {
        return Some("DEPENDENCY_BREAKAGE".to_string());
    }

    None
}

/// Mock repair branch generation for autonomous recovery exploration.
pub fn generate_repair_branch(
    runtime: &mut BranchRuntime,
    failure_signature: &str,
) -> Option<BranchSnapshot> {
    if runtime.budget.remaining_branches == 0 {
        return None;
    }

    // Rule 1: failure triggers repair branch generation.
    let parent = &runtime.committed_branch;
    let mut repair_snapshot = parent.clone();
    repair_snapshot.branch_id = BranchId(format!("{}-repair", parent.branch_id.0));
    repair_snapshot.parent_branch = Some(parent.branch_id.clone());
    repair_snapshot.tx_id = format!("{}-repair-tx", parent.tx_id);
    repair_snapshot.depth += 1;

    // Simulate "repairing" the failure signature.
    if failure_signature.starts_with("VERIFICATION_FAILURE") {
        repair_snapshot.runtime_effects.verification_failures = 0;
        repair_snapshot
            .score
            .world_consistency
            .verification_consistency = 15.0; // Improved
    }

    Some(repair_snapshot)
}

/// Evaluate if a repair branch improves convergence without regression.
pub fn evaluate_repair_convergence(runtime: &BranchRuntime, repair: &BranchSnapshot) -> bool {
    // Rule 10.2: Regression promotion is forbidden.
    repair.score.total() > runtime.committed_branch.score.total()
}

/// Atomically promote a successful repair branch and preserve continuity.
pub fn repair_commit(
    runtime: &mut BranchRuntime,
    session: &mut ExecutionSession,
    memory: &mut ExecutionMemory,
) -> bool {
    // Rule 3: verification recovery success promotes repair.
    if runtime.commit_branch() {
        session.continuity_state = ContinuityState::Active;
        session.repair_attempts = 0;
        memory.record_success(runtime.committed_branch.tx_id.clone());
        true
    } else {
        false
    }
}
