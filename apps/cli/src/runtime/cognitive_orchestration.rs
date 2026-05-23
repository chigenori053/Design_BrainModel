use std::collections::{BTreeMap, BTreeSet};

use crate::runtime::autonomous_control::{
    ExecutionTransaction, RiskLevel, RuntimeLimits, simulate_transaction,
};
use crate::runtime::execution_governance::ExecutionResult;
use crate::tui::rendering::{ProjectionSnapshot, projection_semantic_hash};

#[derive(Debug, Clone, PartialEq)]
pub struct CognitiveState {
    pub cognitive_id: String,
    pub active_goals: Vec<GoalState>,
    pub semantic_memory: SemanticMemoryState,
    pub runtime_context: RuntimeContext,
    pub attention_state: AttentionState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalState {
    pub goal_id: String,
    pub semantic_goal: String,
    pub priority: GoalPriority,
    pub status: GoalStatus,
    pub related_transactions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GoalPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    Created,
    Planned,
    Executing,
    Converging,
    Completed,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SemanticMemoryState {
    pub execution_records: Vec<ExecutionMemoryRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeContext {
    pub projection_snapshot: ProjectionSnapshot,
    pub runtime_epoch: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttentionState {
    pub focused_goals: Vec<String>,
    pub suppressed_contexts: Vec<String>,
    pub attention_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionGraph {
    pub transactions: Vec<ExecutionTransaction>,
    pub dependency_edges: Vec<TransactionDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionDependency {
    pub from_transaction: String,
    pub to_transaction: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BranchEvaluation {
    pub branch_id: String,
    pub semantic_score: f64,
    pub projected_risk: RiskLevel,
    pub convergence_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionMemoryRecord {
    pub semantic_hash: String,
    pub execution_result: ExecutionResult,
    pub convergence_result: ConvergenceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvergenceState {
    Unknown,
    Improving,
    Stable,
    Collapsed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CognitiveSimulationResult {
    pub projected_goal_state: GoalState,
    pub projected_runtime_state: ProjectionSnapshot,
    pub convergence_probability: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitiveHaltTrigger {
    GoalContradiction,
    ConvergenceCollapse,
    MemoryInconsistency,
    AttentionOverflow,
    ProjectionCorruption,
    BranchDivergenceOverflow,
    TransactionDependencyFailure,
    RuntimeLimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitiveLimits {
    pub max_active_goals: usize,
    pub max_branch_count: usize,
    pub max_transaction_depth: usize,
}

impl Default for CognitiveLimits {
    fn default() -> Self {
        Self {
            max_active_goals: 5,
            max_branch_count: 4,
            max_transaction_depth: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitiveProjection {
    pub active_goal_summary: String,
    pub convergence_state: String,
    pub branch_status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CognitiveApprovalRequest {
    pub active_goals: Vec<String>,
    pub projected_convergence: String,
    pub projected_risk: RiskLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CognitiveOrchestrationResult {
    pub cognitive_state: CognitiveState,
    pub transaction_graph: TransactionGraph,
    pub branch_evaluations: Vec<BranchEvaluation>,
    pub selected_branch: Option<BranchEvaluation>,
    pub simulation_results: Vec<CognitiveSimulationResult>,
    pub projection: CognitiveProjection,
    pub halted: bool,
    pub halt_trigger: Option<CognitiveHaltTrigger>,
}

pub fn cognitive_state(
    goals: Vec<GoalState>,
    projection_snapshot: ProjectionSnapshot,
) -> CognitiveState {
    let mut normalized_goals = goals;
    normalized_goals.sort_by(goal_order);
    let cognitive_hash = cognitive_semantic_hash(&normalized_goals, &projection_snapshot);
    let focused_goals = normalized_goals
        .iter()
        .filter(|goal| goal.status != GoalStatus::Completed && goal.status != GoalStatus::Halted)
        .take(3)
        .map(|goal| goal.goal_id.clone())
        .collect::<Vec<_>>();

    CognitiveState {
        cognitive_id: format!("cog-{cognitive_hash}"),
        active_goals: normalized_goals,
        semantic_memory: SemanticMemoryState::default(),
        runtime_context: RuntimeContext {
            projection_snapshot,
            runtime_epoch: 0,
        },
        attention_state: AttentionState {
            focused_goals,
            suppressed_contexts: Vec::new(),
            attention_score: 0.0,
        },
    }
}

pub fn goal_state(
    semantic_goal: &str,
    priority: GoalPriority,
    status: GoalStatus,
    related_transactions: Vec<String>,
) -> GoalState {
    let goal_id = format!("goal-{:016x}", stable_hash([semantic_goal]));
    GoalState {
        goal_id,
        semantic_goal: semantic_goal.to_string(),
        priority,
        status,
        related_transactions,
    }
}

pub fn orchestrate_cognition(
    state: &CognitiveState,
    graph: TransactionGraph,
    branch_evaluations: Vec<BranchEvaluation>,
    cognitive_limits: &CognitiveLimits,
    runtime_limits: &RuntimeLimits,
) -> CognitiveOrchestrationResult {
    if let Err(trigger) =
        validate_cognitive_state(state, &graph, &branch_evaluations, cognitive_limits)
    {
        return halted_result(state, graph, branch_evaluations, trigger);
    }

    let ordered_graph = deterministic_transaction_graph(graph);
    if validate_transaction_dependencies(&ordered_graph).is_err() {
        return halted_result(
            state,
            ordered_graph,
            branch_evaluations,
            CognitiveHaltTrigger::TransactionDependencyFailure,
        );
    }

    let mut updated_state = state.clone();
    updated_state.active_goals =
        advance_goal_lifecycle(&updated_state.active_goals, &ordered_graph);
    updated_state.attention_state = derive_attention_state(&updated_state.active_goals);
    updated_state.runtime_context.runtime_epoch += 1;

    let simulation_results =
        simulate_cognitive_future(&updated_state, &ordered_graph, runtime_limits);
    if simulation_results
        .iter()
        .any(|result| result.convergence_probability == 0.0)
    {
        return halted_result(
            &updated_state,
            ordered_graph,
            branch_evaluations,
            CognitiveHaltTrigger::ConvergenceCollapse,
        );
    }

    let ordered_branches = deterministic_branch_evaluations(branch_evaluations);
    let selected_branch = converge_best_branch(&ordered_branches);
    let projection = cognitive_projection(
        &updated_state,
        selected_branch.as_ref(),
        &simulation_results,
    );

    CognitiveOrchestrationResult {
        cognitive_state: updated_state,
        transaction_graph: ordered_graph,
        branch_evaluations: ordered_branches,
        selected_branch,
        simulation_results,
        projection,
        halted: false,
        halt_trigger: None,
    }
}

pub fn integrate_execution_memory(
    state: &mut CognitiveState,
    semantic_hash: String,
    execution_result: ExecutionResult,
    convergence_result: ConvergenceState,
) {
    state
        .semantic_memory
        .execution_records
        .push(ExecutionMemoryRecord {
            semantic_hash,
            execution_result,
            convergence_result,
        });
    state
        .semantic_memory
        .execution_records
        .sort_by(|a, b| a.semantic_hash.cmp(&b.semantic_hash));
    state
        .semantic_memory
        .execution_records
        .dedup_by(|a, b| a.semantic_hash == b.semantic_hash);
}

pub fn recall_execution_memory<'a>(
    state: &'a CognitiveState,
    semantic_hash: &str,
) -> Option<&'a ExecutionMemoryRecord> {
    state
        .semantic_memory
        .execution_records
        .iter()
        .find(|record| record.semantic_hash == semantic_hash)
}

pub fn converge_best_branch(branches: &[BranchEvaluation]) -> Option<BranchEvaluation> {
    deterministic_branch_evaluations(branches.to_vec())
        .into_iter()
        .filter(|branch| branch.projected_risk < RiskLevel::High)
        .max_by(|a, b| branch_rank(a).cmp(&branch_rank(b)))
}

pub fn cognitive_projection(
    state: &CognitiveState,
    selected_branch: Option<&BranchEvaluation>,
    simulation_results: &[CognitiveSimulationResult],
) -> CognitiveProjection {
    let active_goals = state
        .active_goals
        .iter()
        .filter(|goal| goal.status != GoalStatus::Completed && goal.status != GoalStatus::Halted)
        .map(|goal| goal.semantic_goal.clone())
        .collect::<Vec<_>>();
    let min_convergence = simulation_results
        .iter()
        .map(|result| result.convergence_probability)
        .fold(1.0_f64, f64::min);
    CognitiveProjection {
        active_goal_summary: if active_goals.is_empty() {
            "no active goals".to_string()
        } else {
            active_goals.join(" | ")
        },
        convergence_state: format!("{min_convergence:.3}"),
        branch_status: selected_branch
            .map(|branch| format!("selected:{}", branch.branch_id))
            .unwrap_or_else(|| "no converged branch".to_string()),
    }
}

pub fn cognitive_approval_request(
    state: &CognitiveState,
    risk: RiskLevel,
) -> CognitiveApprovalRequest {
    CognitiveApprovalRequest {
        active_goals: state
            .active_goals
            .iter()
            .map(|goal| goal.semantic_goal.clone())
            .collect(),
        projected_convergence: cognitive_projection(state, None, &[]).convergence_state,
        projected_risk: risk,
    }
}

pub fn requires_cognitive_approval(
    graph: &TransactionGraph,
    branch_apply: bool,
    remote_execution: bool,
) -> bool {
    branch_apply || remote_execution || graph.transactions.len() > 1
}

pub fn validate_branch_isolation(
    source: &BranchEvaluation,
    target: &BranchEvaluation,
) -> Result<(), CognitiveHaltTrigger> {
    if source.branch_id == target.branch_id {
        Ok(())
    } else {
        Err(CognitiveHaltTrigger::BranchDivergenceOverflow)
    }
}

fn validate_cognitive_state(
    state: &CognitiveState,
    graph: &TransactionGraph,
    branches: &[BranchEvaluation],
    limits: &CognitiveLimits,
) -> Result<(), CognitiveHaltTrigger> {
    if state.active_goals.len() > limits.max_active_goals
        || branches.len() > limits.max_branch_count
        || graph_dependency_depth(graph) > limits.max_transaction_depth
    {
        return Err(CognitiveHaltTrigger::RuntimeLimitExceeded);
    }
    if state.attention_state.attention_score > 1.0
        || state.attention_state.focused_goals.len() > limits.max_active_goals
    {
        return Err(CognitiveHaltTrigger::AttentionOverflow);
    }
    if has_goal_contradiction(&state.active_goals) {
        return Err(CognitiveHaltTrigger::GoalContradiction);
    }
    if projection_semantic_hash(&state.runtime_context.projection_snapshot)
        != state
            .runtime_context
            .projection_snapshot
            .projection_hash
            .semantic_hash
    {
        return Err(CognitiveHaltTrigger::ProjectionCorruption);
    }
    if has_memory_inconsistency(&state.semantic_memory) {
        return Err(CognitiveHaltTrigger::MemoryInconsistency);
    }
    Ok(())
}

fn validate_transaction_dependencies(graph: &TransactionGraph) -> Result<(), CognitiveHaltTrigger> {
    let transaction_ids = graph
        .transactions
        .iter()
        .map(|transaction| transaction.transaction_id.clone())
        .collect::<BTreeSet<_>>();
    for edge in &graph.dependency_edges {
        if !transaction_ids.contains(&edge.from_transaction)
            || !transaction_ids.contains(&edge.to_transaction)
        {
            return Err(CognitiveHaltTrigger::TransactionDependencyFailure);
        }
    }
    Ok(())
}

fn deterministic_transaction_graph(graph: TransactionGraph) -> TransactionGraph {
    let mut transactions = graph.transactions;
    transactions.sort_by(|a, b| a.transaction_id.cmp(&b.transaction_id));
    let mut dependency_edges = graph.dependency_edges;
    dependency_edges.sort_by(|a, b| {
        a.from_transaction
            .cmp(&b.from_transaction)
            .then_with(|| a.to_transaction.cmp(&b.to_transaction))
    });
    TransactionGraph {
        transactions,
        dependency_edges,
    }
}

fn deterministic_branch_evaluations(mut branches: Vec<BranchEvaluation>) -> Vec<BranchEvaluation> {
    branches.sort_by(|a, b| a.branch_id.cmp(&b.branch_id));
    branches
}

fn advance_goal_lifecycle(goals: &[GoalState], graph: &TransactionGraph) -> Vec<GoalState> {
    let transaction_ids = graph
        .transactions
        .iter()
        .map(|transaction| transaction.transaction_id.clone())
        .collect::<BTreeSet<_>>();
    let mut advanced = goals.to_vec();
    for goal in &mut advanced {
        let has_related_transaction = goal
            .related_transactions
            .iter()
            .any(|transaction_id| transaction_ids.contains(transaction_id));
        goal.status = match (goal.status, has_related_transaction) {
            (GoalStatus::Created, true) => GoalStatus::Planned,
            (GoalStatus::Planned, true) => GoalStatus::Executing,
            (GoalStatus::Executing, true) => GoalStatus::Converging,
            (status, _) => status,
        };
    }
    advanced.sort_by(goal_order);
    advanced
}

fn derive_attention_state(goals: &[GoalState]) -> AttentionState {
    let focused_goals = goals
        .iter()
        .filter(|goal| {
            matches!(
                goal.status,
                GoalStatus::Planned | GoalStatus::Executing | GoalStatus::Converging
            )
        })
        .take(3)
        .map(|goal| goal.goal_id.clone())
        .collect::<Vec<_>>();
    let suppressed_contexts = goals
        .iter()
        .filter(|goal| goal.status == GoalStatus::Completed)
        .map(|goal| goal.goal_id.clone())
        .collect::<Vec<_>>();
    let attention_score = (focused_goals.len() as f64 / 3.0).min(1.0);
    AttentionState {
        focused_goals,
        suppressed_contexts,
        attention_score,
    }
}

fn simulate_cognitive_future(
    state: &CognitiveState,
    graph: &TransactionGraph,
    runtime_limits: &RuntimeLimits,
) -> Vec<CognitiveSimulationResult> {
    let transaction_by_id = graph
        .transactions
        .iter()
        .map(|transaction| (transaction.transaction_id.as_str(), transaction))
        .collect::<BTreeMap<_, _>>();

    state
        .active_goals
        .iter()
        .map(|goal| {
            let mut projected_goal_state = goal.clone();
            projected_goal_state.status = match goal.status {
                GoalStatus::Created => GoalStatus::Planned,
                GoalStatus::Planned => GoalStatus::Executing,
                GoalStatus::Executing => GoalStatus::Converging,
                status => status,
            };
            let mut projected_runtime_state = state.runtime_context.projection_snapshot.clone();
            let worst_risk = goal
                .related_transactions
                .iter()
                .filter_map(|transaction_id| transaction_by_id.get(transaction_id.as_str()))
                .map(|transaction| {
                    simulate_transaction(
                        transaction,
                        &state.runtime_context.projection_snapshot,
                        runtime_limits,
                    )
                    .projected_risk
                })
                .max()
                .unwrap_or(RiskLevel::Low);
            let convergence_probability = match worst_risk {
                RiskLevel::Low => 0.95,
                RiskLevel::Medium => 0.70,
                RiskLevel::High => 0.0,
                RiskLevel::Critical => 0.0,
            };
            projected_runtime_state.workspace.operation = "cognitive-simulation".to_string();
            projected_runtime_state.workspace.status =
                format!("goal:{}:{convergence_probability:.3}", goal.goal_id);
            projected_runtime_state.runtime_state =
                projected_runtime_state.workspace.status.clone();
            projected_runtime_state.projection_hash.semantic_hash =
                projection_semantic_hash(&projected_runtime_state);
            CognitiveSimulationResult {
                projected_goal_state,
                projected_runtime_state,
                convergence_probability,
            }
        })
        .collect()
}

fn halted_result(
    state: &CognitiveState,
    graph: TransactionGraph,
    branch_evaluations: Vec<BranchEvaluation>,
    trigger: CognitiveHaltTrigger,
) -> CognitiveOrchestrationResult {
    CognitiveOrchestrationResult {
        cognitive_state: state.clone(),
        transaction_graph: deterministic_transaction_graph(graph),
        branch_evaluations: deterministic_branch_evaluations(branch_evaluations),
        selected_branch: None,
        simulation_results: Vec::new(),
        projection: CognitiveProjection {
            active_goal_summary: "cognition halted".to_string(),
            convergence_state: format!("{trigger:?}"),
            branch_status: "frozen".to_string(),
        },
        halted: true,
        halt_trigger: Some(trigger),
    }
}

fn has_goal_contradiction(goals: &[GoalState]) -> bool {
    let mut seen = BTreeMap::<String, GoalStatus>::new();
    for goal in goals {
        let normalized = goal.semantic_goal.trim().to_ascii_lowercase();
        if let Some(previous_status) = seen.insert(normalized, goal.status)
            && previous_status != goal.status
        {
            return true;
        }
    }
    false
}

fn has_memory_inconsistency(memory: &SemanticMemoryState) -> bool {
    let mut seen = BTreeSet::new();
    for record in &memory.execution_records {
        if record.semantic_hash.is_empty() || !seen.insert(record.semantic_hash.clone()) {
            return true;
        }
    }
    false
}

fn graph_dependency_depth(graph: &TransactionGraph) -> usize {
    graph
        .dependency_edges
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut depths, edge| {
            let from_depth = *depths.get(&edge.from_transaction).unwrap_or(&0);
            let next_depth = from_depth + 1;
            let current = depths.entry(edge.to_transaction.clone()).or_default();
            *current = (*current).max(next_depth);
            depths
        })
        .values()
        .copied()
        .max()
        .unwrap_or(0)
}

fn branch_rank(branch: &BranchEvaluation) -> (i64, i64, std::cmp::Reverse<RiskLevel>, String) {
    (
        score_key(branch.convergence_score),
        score_key(branch.semantic_score),
        std::cmp::Reverse(branch.projected_risk),
        std::cmp::Reverse(branch.branch_id.clone()).0,
    )
}

fn score_key(value: f64) -> i64 {
    (value.clamp(0.0, 1.0) * 1_000_000.0).round() as i64
}

fn goal_order(a: &GoalState, b: &GoalState) -> std::cmp::Ordering {
    b.priority
        .cmp(&a.priority)
        .then_with(|| a.goal_id.cmp(&b.goal_id))
}

fn cognitive_semantic_hash(goals: &[GoalState], snapshot: &ProjectionSnapshot) -> String {
    let mut parts = Vec::new();
    for goal in goals {
        let mut transactions = goal.related_transactions.clone();
        transactions.sort();
        parts.push(format!(
            "{}|{}|{:?}|{:?}|{}",
            goal.goal_id,
            goal.semantic_goal,
            goal.priority,
            goal.status,
            transactions.join(",")
        ));
    }
    parts.push(projection_semantic_hash(snapshot));
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
    use crate::runtime::autonomous_control::{
        AutonomousExecutionMode, ExecutionStep, RollbackStrategy, plan_transaction,
    };
    use crate::runtime::execution_governance::classify_command;
    use crate::runtime::shell::ResolvedExecutionTarget;
    use crate::tui::rendering::{
        DiagnosticProjection, NarrativeProjection, ProjectionHash, WorkspaceProjection,
    };

    fn projection() -> ProjectionSnapshot {
        let mut snapshot = ProjectionSnapshot {
            workspace: WorkspaceProjection {
                target: Some("apps/cli/src/core.rs".to_string()),
                operation: "preview".to_string(),
                status: "ready".to_string(),
            },
            diagnostics: DiagnosticProjection::default(),
            narrative: NarrativeProjection {
                lines: vec!["stable cognitive base".to_string()],
            },
            runtime_state: "ready".to_string(),
            projection_hash: ProjectionHash::default(),
        };
        snapshot.projection_hash.semantic_hash = projection_semantic_hash(&snapshot);
        snapshot
    }

    fn tx(intent: &str, command: &str) -> ExecutionTransaction {
        let target = ResolvedExecutionTarget::from_canonical_path("apps/cli/src/core.rs");
        plan_transaction(
            intent,
            vec![ExecutionStep {
                step_id: "step-1".to_string(),
                semantic_intent: intent.to_string(),
                resolved_target: target,
                command: command.to_string(),
                command_type: classify_command(command),
                dependency_ids: Vec::new(),
            }],
            RollbackStrategy::SemanticCheckpoint,
            AutonomousExecutionMode::GovernedExecution,
        )
    }

    fn graph(transactions: Vec<ExecutionTransaction>) -> TransactionGraph {
        TransactionGraph {
            transactions,
            dependency_edges: Vec::new(),
        }
    }

    #[test]
    fn same_goals_produce_same_orchestration() {
        let transaction = tx("status", "git status");
        let goal = goal_state(
            "stabilize runtime",
            GoalPriority::Critical,
            GoalStatus::Created,
            vec![transaction.transaction_id.clone()],
        );
        let state = cognitive_state(vec![goal], projection());

        let first = orchestrate_cognition(
            &state,
            graph(vec![transaction.clone()]),
            vec![],
            &CognitiveLimits::default(),
            &RuntimeLimits::default(),
        );
        let second = orchestrate_cognition(
            &state,
            graph(vec![transaction]),
            vec![],
            &CognitiveLimits::default(),
            &RuntimeLimits::default(),
        );

        assert_eq!(first, second);
        assert!(!first.halted);
    }

    #[test]
    fn goal_contradiction_halts_cognition() {
        let state = cognitive_state(
            vec![
                goal_state("same goal", GoalPriority::High, GoalStatus::Created, vec![]),
                goal_state(
                    "same goal",
                    GoalPriority::High,
                    GoalStatus::Completed,
                    vec![],
                ),
            ],
            projection(),
        );

        let result = orchestrate_cognition(
            &state,
            graph(vec![]),
            vec![],
            &CognitiveLimits::default(),
            &RuntimeLimits::default(),
        );

        assert!(result.halted);
        assert_eq!(
            result.halt_trigger,
            Some(CognitiveHaltTrigger::GoalContradiction)
        );
    }

    #[test]
    fn failed_transaction_halts_dependent_transaction_graph() {
        let transaction = tx("status", "git status");
        let graph = TransactionGraph {
            transactions: vec![transaction],
            dependency_edges: vec![TransactionDependency {
                from_transaction: "missing".to_string(),
                to_transaction: "also-missing".to_string(),
            }],
        };
        let state = cognitive_state(vec![], projection());

        let result = orchestrate_cognition(
            &state,
            graph,
            vec![],
            &CognitiveLimits::default(),
            &RuntimeLimits::default(),
        );

        assert!(result.halted);
        assert_eq!(
            result.halt_trigger,
            Some(CognitiveHaltTrigger::TransactionDependencyFailure)
        );
    }

    #[test]
    fn branch_convergence_selects_best_low_risk_branch_deterministically() {
        let branches = vec![
            BranchEvaluation {
                branch_id: "branch-b".to_string(),
                semantic_score: 0.8,
                projected_risk: RiskLevel::Low,
                convergence_score: 0.9,
            },
            BranchEvaluation {
                branch_id: "branch-a".to_string(),
                semantic_score: 0.8,
                projected_risk: RiskLevel::High,
                convergence_score: 1.0,
            },
        ];

        let selected = converge_best_branch(&branches).expect("branch selected");

        assert_eq!(selected.branch_id, "branch-b");
    }

    #[test]
    fn branch_isolation_blocks_cross_branch_mutation() {
        let a = BranchEvaluation {
            branch_id: "branch-a".to_string(),
            semantic_score: 1.0,
            projected_risk: RiskLevel::Low,
            convergence_score: 1.0,
        };
        let b = BranchEvaluation {
            branch_id: "branch-b".to_string(),
            semantic_score: 1.0,
            projected_risk: RiskLevel::Low,
            convergence_score: 1.0,
        };

        assert_eq!(
            validate_branch_isolation(&a, &b),
            Err(CognitiveHaltTrigger::BranchDivergenceOverflow)
        );
    }

    #[test]
    fn memory_integration_is_recallable_and_deduplicated() {
        let mut state = cognitive_state(vec![], projection());

        integrate_execution_memory(
            &mut state,
            "hash-a".to_string(),
            ExecutionResult {
                status: "ok".to_string(),
                summary: "accepted".to_string(),
            },
            ConvergenceState::Stable,
        );
        integrate_execution_memory(
            &mut state,
            "hash-a".to_string(),
            ExecutionResult {
                status: "ok".to_string(),
                summary: "accepted".to_string(),
            },
            ConvergenceState::Stable,
        );

        assert_eq!(state.semantic_memory.execution_records.len(), 1);
        assert!(recall_execution_memory(&state, "hash-a").is_some());
    }

    #[test]
    fn attention_overflow_halts_runtime() {
        let mut state = cognitive_state(vec![], projection());
        state.attention_state.attention_score = 1.5;

        let result = orchestrate_cognition(
            &state,
            graph(vec![]),
            vec![],
            &CognitiveLimits::default(),
            &RuntimeLimits::default(),
        );

        assert!(result.halted);
        assert_eq!(
            result.halt_trigger,
            Some(CognitiveHaltTrigger::AttentionOverflow)
        );
    }
}
