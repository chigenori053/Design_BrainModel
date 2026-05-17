use std::collections::BTreeSet;

use crate::runtime::execution_governance::{
    CommandType, ExecutionAuditRecord, ExecutionMode as GovernedExecutionMode, ExecutionRequest,
    ExecutionResult, ExecutionState, audit_execution, classify_command, command_policy,
    semantic_intent_matches_command, validate_execution_request,
};
use crate::runtime::shell::ResolvedExecutionTarget;
use crate::tui::rendering::{ProjectionSnapshot, projection_semantic_hash};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousExecutionMode {
    DryRun,
    GovernedExecution,
    RestrictedAutonomy,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackStrategy {
    None,
    SemanticCheckpoint,
    HaltOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLimits {
    pub max_commands: usize,
    pub max_mutations: usize,
    pub max_execution_time_ms: u64,
    pub max_transaction_depth: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_commands: 8,
            max_mutations: 2,
            max_execution_time_ms: 30_000,
            max_transaction_depth: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTransaction {
    pub transaction_id: String,
    pub semantic_hash: String,
    pub steps: Vec<ExecutionStep>,
    pub rollback_strategy: RollbackStrategy,
    pub execution_mode: AutonomousExecutionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionStep {
    pub step_id: String,
    pub semantic_intent: String,
    pub resolved_target: ResolvedExecutionTarget,
    pub command: String,
    pub command_type: CommandType,
    pub dependency_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationResult {
    pub projected_state: ProjectionSnapshot,
    pub projected_risk: RiskLevel,
    pub semantic_consistency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRuntime {
    pub branch_id: String,
    pub projection_snapshot: ProjectionSnapshot,
    pub execution_state: ExecutionState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackCheckpoint {
    pub projection_snapshot: ProjectionSnapshot,
    pub semantic_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousHaltTrigger {
    SemanticMismatch,
    ForbiddenCommand,
    DependencyFailure,
    ProjectionCorruption,
    RuntimeOverflow,
    SimulationRiskOverflow,
    TargetMutation,
    HaltedMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousExecutionAudit {
    pub transaction_id: String,
    pub semantic_hash: String,
    pub executed_steps: Vec<ExecutionAuditRecord>,
    pub simulation_result: SimulationResult,
    pub halted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousExecutionProjection {
    pub transaction_summary: String,
    pub simulation_status: String,
    pub execution_state: ExecutionState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub semantic_summary: String,
    pub risk_level: RiskLevel,
    pub execution_preview: String,
}

pub fn plan_transaction(
    semantic_intent: &str,
    steps: Vec<ExecutionStep>,
    rollback_strategy: RollbackStrategy,
    execution_mode: AutonomousExecutionMode,
) -> ExecutionTransaction {
    let semantic_hash = transaction_semantic_hash(semantic_intent, &steps);
    ExecutionTransaction {
        transaction_id: format!("tx-{semantic_hash}"),
        semantic_hash,
        steps,
        rollback_strategy,
        execution_mode,
    }
}

pub fn rollback_checkpoint(snapshot: ProjectionSnapshot) -> RollbackCheckpoint {
    RollbackCheckpoint {
        semantic_hash: projection_semantic_hash(&snapshot),
        projection_snapshot: snapshot,
    }
}

pub fn restore_rollback_checkpoint(checkpoint: &RollbackCheckpoint) -> ProjectionSnapshot {
    let mut snapshot = checkpoint.projection_snapshot.clone();
    snapshot.projection_hash.semantic_hash = projection_semantic_hash(&snapshot);
    snapshot
}

pub fn simulate_transaction(
    transaction: &ExecutionTransaction,
    base_projection: &ProjectionSnapshot,
    limits: &RuntimeLimits,
) -> SimulationResult {
    let limit_failure = validate_runtime_limits(transaction, limits).is_err();
    let dependency_failure = validate_all_dependencies(transaction).is_err();
    let semantic_consistency = transaction
        .steps
        .iter()
        .all(|step| semantic_intent_matches_command(&step.semantic_intent, &step.command));
    let risk = transaction.steps.iter().fold(RiskLevel::Low, |risk, step| {
        risk.max(risk_for_command_type(classify_command(&step.command)))
    });
    let mut projected_risk = risk;
    if limit_failure {
        projected_risk = projected_risk.max(RiskLevel::Critical);
    }
    if dependency_failure || !semantic_consistency {
        projected_risk = projected_risk.max(RiskLevel::High);
    }

    let mut projected_state = base_projection.clone();
    projected_state.workspace.operation = "autonomous-simulation".to_string();
    projected_state.workspace.status = match projected_risk {
        RiskLevel::Low | RiskLevel::Medium => "simulation accepted".to_string(),
        RiskLevel::High | RiskLevel::Critical => "simulation rejected".to_string(),
    };
    projected_state.runtime_state = projected_state.workspace.status.clone();
    projected_state.narrative.lines = vec![
        format!("transaction: {}", transaction.transaction_id),
        format!("steps: {}", transaction.steps.len()),
        format!("risk: {projected_risk:?}"),
    ];
    projected_state.projection_hash.semantic_hash = projection_semantic_hash(&projected_state);

    SimulationResult {
        projected_state,
        projected_risk,
        semantic_consistency,
    }
}

pub fn execute_autonomous_transaction(
    transaction: &ExecutionTransaction,
    base_projection: &ProjectionSnapshot,
    limits: &RuntimeLimits,
) -> AutonomousExecutionAudit {
    let simulation_result = simulate_transaction(transaction, base_projection, limits);
    if transaction.execution_mode == AutonomousExecutionMode::Halted
        || simulation_result.projected_risk >= RiskLevel::High
    {
        return AutonomousExecutionAudit {
            transaction_id: transaction.transaction_id.clone(),
            semantic_hash: transaction.semantic_hash.clone(),
            executed_steps: Vec::new(),
            simulation_result,
            halted: true,
        };
    }

    let mode = governed_mode(transaction.execution_mode);
    let projection_hash = &simulation_result
        .projected_state
        .projection_hash
        .semantic_hash;
    let mut completed = BTreeSet::new();
    let mut records = Vec::new();

    for step in &transaction.steps {
        if validate_step_dependencies(step, &completed).is_err() {
            return halted_audit(transaction, simulation_result, records);
        }

        let request = ExecutionRequest {
            semantic_intent: step.semantic_intent.clone(),
            resolved_target: step.resolved_target.clone(),
            command: step.command.clone(),
            command_type: step.command_type,
        };
        let validation = validate_execution_request(&request, mode, &step.resolved_target);
        let approved = validation.allowed && !requires_human_approval(step);
        let record = audit_execution(
            &request,
            projection_hash,
            approved,
            ExecutionResult {
                status: if approved { "approved" } else { "rejected" }.to_string(),
                summary: validation.errors.join("; "),
            },
        );
        records.push(record);

        if !approved {
            return halted_audit(transaction, simulation_result, records);
        }
        completed.insert(step.step_id.clone());
    }

    AutonomousExecutionAudit {
        transaction_id: transaction.transaction_id.clone(),
        semantic_hash: transaction.semantic_hash.clone(),
        executed_steps: records,
        simulation_result,
        halted: false,
    }
}

pub fn autonomous_projection(audit: &AutonomousExecutionAudit) -> AutonomousExecutionProjection {
    AutonomousExecutionProjection {
        transaction_summary: format!("{}:{}", audit.transaction_id, audit.executed_steps.len()),
        simulation_status: format!("{:?}", audit.simulation_result.projected_risk),
        execution_state: if audit.halted {
            ExecutionState::Halted
        } else if audit.executed_steps.iter().all(|record| record.approved) {
            ExecutionState::Allowed
        } else {
            ExecutionState::Rejected
        },
    }
}

pub fn approval_request(step: &ExecutionStep, risk_level: RiskLevel) -> ApprovalRequest {
    ApprovalRequest {
        semantic_summary: step.semantic_intent.clone(),
        risk_level,
        execution_preview: semantic_command_preview(&step.command),
    }
}

pub fn requires_human_approval(step: &ExecutionStep) -> bool {
    let policy = command_policy(classify_command(&step.command));
    policy.requires_confirmation
        || step.command_type == CommandType::Dangerous
        || step.command.contains("push")
        || step.command.contains("merge")
}

pub fn validate_runtime_limits(
    transaction: &ExecutionTransaction,
    limits: &RuntimeLimits,
) -> Result<(), AutonomousHaltTrigger> {
    let mutation_count = transaction
        .steps
        .iter()
        .filter(|step| {
            matches!(
                classify_command(&step.command),
                CommandType::SafeWrite | CommandType::Dangerous
            )
        })
        .count();
    let max_depth = transaction
        .steps
        .iter()
        .map(|step| step.dependency_ids.len())
        .max()
        .unwrap_or(0);

    if transaction.steps.len() > limits.max_commands
        || mutation_count > limits.max_mutations
        || max_depth > limits.max_transaction_depth
        || limits.max_execution_time_ms == 0
    {
        Err(AutonomousHaltTrigger::RuntimeOverflow)
    } else {
        Ok(())
    }
}

pub fn validate_all_dependencies(
    transaction: &ExecutionTransaction,
) -> Result<(), AutonomousHaltTrigger> {
    let mut completed = BTreeSet::new();
    for step in &transaction.steps {
        validate_step_dependencies(step, &completed)?;
        completed.insert(step.step_id.clone());
    }
    Ok(())
}

pub fn branch_runtime(
    branch_id: &str,
    projection_snapshot: ProjectionSnapshot,
    execution_state: ExecutionState,
) -> BranchRuntime {
    BranchRuntime {
        branch_id: branch_id.to_string(),
        projection_snapshot,
        execution_state,
    }
}

fn validate_step_dependencies(
    step: &ExecutionStep,
    completed: &BTreeSet<String>,
) -> Result<(), AutonomousHaltTrigger> {
    if step
        .dependency_ids
        .iter()
        .all(|dependency| completed.contains(dependency))
    {
        Ok(())
    } else {
        Err(AutonomousHaltTrigger::DependencyFailure)
    }
}

fn halted_audit(
    transaction: &ExecutionTransaction,
    simulation_result: SimulationResult,
    executed_steps: Vec<ExecutionAuditRecord>,
) -> AutonomousExecutionAudit {
    AutonomousExecutionAudit {
        transaction_id: transaction.transaction_id.clone(),
        semantic_hash: transaction.semantic_hash.clone(),
        executed_steps,
        simulation_result,
        halted: true,
    }
}

fn governed_mode(mode: AutonomousExecutionMode) -> GovernedExecutionMode {
    match mode {
        AutonomousExecutionMode::DryRun => GovernedExecutionMode::DryRun,
        AutonomousExecutionMode::GovernedExecution
        | AutonomousExecutionMode::RestrictedAutonomy => GovernedExecutionMode::GovernedExecute,
        AutonomousExecutionMode::Halted => GovernedExecutionMode::Halted,
    }
}

fn risk_for_command_type(command_type: CommandType) -> RiskLevel {
    match command_type {
        CommandType::SafeRead => RiskLevel::Low,
        CommandType::SafeWrite => RiskLevel::Medium,
        CommandType::Dangerous => RiskLevel::High,
        CommandType::Forbidden => RiskLevel::Critical,
    }
}

fn semantic_command_preview(command: &str) -> String {
    match classify_command(command) {
        CommandType::SafeRead => "read-only command".to_string(),
        CommandType::SafeWrite => "bounded mutation command".to_string(),
        CommandType::Dangerous => "remote or high-risk command".to_string(),
        CommandType::Forbidden => "forbidden command".to_string(),
    }
}

fn transaction_semantic_hash(semantic_intent: &str, steps: &[ExecutionStep]) -> String {
    let mut parts = Vec::new();
    parts.push(format!("intent={semantic_intent}"));
    for step in steps {
        let mut dependencies = step.dependency_ids.clone();
        dependencies.sort();
        parts.push(format!(
            "{}|{}|{}|{}|{}|{}",
            step.step_id,
            step.semantic_intent,
            step.resolved_target.canonical_target.path,
            step.resolved_target.semantic_hash,
            step.command,
            dependencies.join(",")
        ));
    }
    format!("{:016x}", stable_hash(parts.iter().map(String::as_str)))
}

fn stable_hash<'a>(values: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        hash ^= value.len() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::rendering::{
        DiagnosticProjection, NarrativeProjection, ProjectionHash, WorkspaceProjection,
    };

    fn target() -> ResolvedExecutionTarget {
        ResolvedExecutionTarget::from_canonical_path("apps/cli/src/core.rs")
    }

    fn projection() -> ProjectionSnapshot {
        let mut snapshot = ProjectionSnapshot {
            workspace: WorkspaceProjection {
                target: Some("apps/cli/src/core.rs".to_string()),
                operation: "preview".to_string(),
                status: "ready".to_string(),
            },
            diagnostics: DiagnosticProjection::default(),
            narrative: NarrativeProjection {
                lines: vec!["stable base".to_string()],
            },
            runtime_state: "ready".to_string(),
            projection_hash: ProjectionHash::default(),
        };
        snapshot.projection_hash.semantic_hash = projection_semantic_hash(&snapshot);
        snapshot
    }

    fn step(step_id: &str, intent: &str, command: &str, deps: Vec<&str>) -> ExecutionStep {
        ExecutionStep {
            step_id: step_id.to_string(),
            semantic_intent: intent.to_string(),
            resolved_target: target(),
            command: command.to_string(),
            command_type: classify_command(command),
            dependency_ids: deps.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn dry_run_simulates_without_approval() {
        let tx = plan_transaction(
            "inspect status",
            vec![step("status", "status", "git status", vec![])],
            RollbackStrategy::SemanticCheckpoint,
            AutonomousExecutionMode::DryRun,
        );

        let audit = execute_autonomous_transaction(&tx, &projection(), &RuntimeLimits::default());

        assert!(audit.halted);
        assert_eq!(audit.executed_steps.len(), 1);
        assert!(!audit.executed_steps[0].approved);
        assert_eq!(audit.simulation_result.projected_risk, RiskLevel::Low);
    }

    #[test]
    fn forbidden_command_halts_before_execution() {
        let tx = plan_transaction(
            "force push",
            vec![step(
                "push",
                "run tests",
                "git push --force origin main",
                vec![],
            )],
            RollbackStrategy::HaltOnly,
            AutonomousExecutionMode::GovernedExecution,
        );

        let audit = execute_autonomous_transaction(&tx, &projection(), &RuntimeLimits::default());

        assert!(audit.halted);
        assert!(audit.executed_steps.is_empty());
        assert_eq!(audit.simulation_result.projected_risk, RiskLevel::Critical);
    }

    #[test]
    fn missing_dependency_halts_transaction() {
        let tx = plan_transaction(
            "inspect log",
            vec![step("log", "log", "git log", vec!["status"])],
            RollbackStrategy::SemanticCheckpoint,
            AutonomousExecutionMode::GovernedExecution,
        );

        let audit = execute_autonomous_transaction(&tx, &projection(), &RuntimeLimits::default());

        assert!(audit.halted);
        assert_eq!(audit.simulation_result.projected_risk, RiskLevel::High);
    }

    #[test]
    fn runtime_limits_reject_unbounded_transactions() {
        let tx = plan_transaction(
            "inspect repository",
            vec![
                step("status", "status", "git status", vec![]),
                step("diff", "diff", "git diff", vec!["status"]),
            ],
            RollbackStrategy::SemanticCheckpoint,
            AutonomousExecutionMode::GovernedExecution,
        );
        let limits = RuntimeLimits {
            max_commands: 1,
            ..RuntimeLimits::default()
        };

        let simulation = simulate_transaction(&tx, &projection(), &limits);

        assert_eq!(simulation.projected_risk, RiskLevel::Critical);
    }

    #[test]
    fn same_input_produces_same_audit_and_projection() {
        let tx = plan_transaction(
            "inspect status",
            vec![step("status", "status", "git status", vec![])],
            RollbackStrategy::SemanticCheckpoint,
            AutonomousExecutionMode::GovernedExecution,
        );
        let base = projection();

        let first = execute_autonomous_transaction(&tx, &base, &RuntimeLimits::default());
        let second = execute_autonomous_transaction(&tx, &base, &RuntimeLimits::default());

        assert_eq!(first, second);
        assert_eq!(
            autonomous_projection(&first),
            autonomous_projection(&second)
        );
    }

    #[test]
    fn rollback_checkpoint_restores_projection_hash() {
        let base = projection();
        let checkpoint = rollback_checkpoint(base.clone());
        let restored = restore_rollback_checkpoint(&checkpoint);

        assert_eq!(restored, base);
        assert_eq!(
            checkpoint.semantic_hash,
            projection_semantic_hash(&restored)
        );
    }
}
