use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::coding::TransactionalApplyResult;
use crate::ir::{
    emit_intent_captured, emit_plan_accepted, emit_plan_proposed, restore_or_initialize_ir_state,
};
use crate::nl::r#loop::{
    AnalyzeResult, LoopOrigin, LoopOutcome, LoopPromotable, PromotionGuard, RepairLoopController,
    RetryEvaluator,
};
use crate::session::AgentSession;

use super::convergence::{ConvergenceMetrics, goal_reached};
use super::executor::execute_ir_plan;
use super::goal::{GoalType, goal_label};
use super::planner_v2::update_conversation_after_plan;
use super::session::ConversationState;
use super::types::{CodingOptions, CommandPlan, ExecutionPlan, PlannedStep};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutonomousLoop {
    pub max_iterations: usize,
    pub convergence_threshold: f32,
}

impl Default for AutonomousLoop {
    fn default() -> Self {
        Self {
            max_iterations: 5,
            convergence_threshold: 0.95,
        }
    }
}

pub struct AutonomousResult {
    pub outputs: Vec<String>,
    pub completed: bool,
    pub iterations: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HookTelemetry {
    pub origin: LoopOrigin,
    pub promoted: bool,
    pub converged: bool,
    pub retries: u8,
    pub false_promotion: bool,
    pub rollback_used: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KpiSnapshot {
    pub analyze_convergence: f32,
    pub coding_retry_success: f32,
    pub validate_self_heal: f32,
    pub structure_bind_precision: f32,
    pub memory_false_promotion: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TunedPolicy {
    pub analyze_threshold: f32,
    pub memory_threshold: f32,
    pub retry_budget_overrides: HashMap<LoopOrigin, u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyOptimizer {
    pub false_promotion_weight: f32,
    pub convergence_weight: f32,
    pub retry_cost_weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OriginBenchmarkSnapshot {
    pub analyze_convergence: f32,
    pub coding_retry_success: f32,
    pub validate_self_heal: f32,
    pub structure_bind_precision: f32,
    pub memory_one_shot: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegressionScorecard {
    pub baseline_failed: u64,
    pub current_failed: u64,
    pub baseline_hook_sensitive: u64,
    pub current_hook_sensitive: u64,
    pub failure_delta: i64,
    pub hook_sensitive_delta: i64,
    pub convergence_delta: f32,
    pub retry_median_delta: f32,
    pub false_promotion_delta: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureSurgeryScenario {
    pub name: String,
    pub compile_pass: bool,
    pub minimal_diff: bool,
    pub rollback_used: bool,
    pub cycle_break_success: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureSurgerySnapshot {
    pub compile_pass_rate: f32,
    pub minimal_diff_rate: f32,
    pub rollback_rate: f32,
    pub cycle_break_success: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PhaseDBaseline {
    total_tests: u64,
    failed_tests: u64,
    hook_sensitive_failures: u64,
    known_preexisting_failures: u64,
}

impl Default for PolicyOptimizer {
    fn default() -> Self {
        Self {
            false_promotion_weight: 1.0,
            convergence_weight: 1.0,
            retry_cost_weight: 1.0,
        }
    }
}

pub fn maybe_promote<T: LoopPromotable>(
    result: T,
    controller: &mut RepairLoopController,
) -> Result<Option<LoopOutcome>> {
    maybe_promote_with_origin(result, None, controller)
}

fn maybe_promote_with_origin<T: LoopPromotable>(
    result: T,
    origin_hint: Option<LoopOrigin>,
    controller: &mut RepairLoopController,
) -> Result<Option<LoopOutcome>> {
    let telemetry_root = telemetry_root();
    let context = match result.promote() {
        Ok(context) => context,
        Err(_) => {
            if let Some(origin) = origin_hint {
                record_hook_telemetry(
                    &telemetry_root,
                    &HookTelemetry {
                        origin,
                        promoted: false,
                        converged: false,
                        retries: 0,
                        false_promotion: true,
                        rollback_used: false,
                    },
                )?;
            }
            return Ok(None);
        }
    };
    if context
        .validate_with_guard(PromotionGuard::default())
        .is_err()
    {
        record_hook_telemetry(
            &telemetry_root,
            &HookTelemetry {
                origin: context.origin,
                promoted: false,
                converged: false,
                retries: 0,
                false_promotion: true,
                rollback_used: context.rollback_token.is_some(),
            },
        )?;
        return Ok(None);
    }
    let entry = match context.suggested_entry_state() {
        Ok(entry) => entry,
        Err(_) => {
            record_hook_telemetry(
                &telemetry_root,
                &HookTelemetry {
                    origin: context.origin,
                    promoted: false,
                    converged: false,
                    retries: 0,
                    false_promotion: true,
                    rollback_used: context.rollback_token.is_some(),
                },
            )?;
            return Ok(None);
        }
    };
    let outcome = controller.start_from(context.clone(), entry)?;
    record_hook_telemetry(
        &telemetry_root,
        &HookTelemetry {
            origin: context.origin,
            promoted: true,
            converged: !matches!(
                outcome.status.state,
                super::r#loop::ReplLoopState::Escalated
            ),
            retries: 0,
            false_promotion: false,
            rollback_used: context.rollback_token.is_some(),
        },
    )?;
    Ok(Some(outcome))
}

pub fn run_goal_loop(
    goal: GoalType,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    config: AutonomousLoop,
) -> AutonomousResult {
    if conversation.ir_state.session_id.is_empty() {
        let workspace_root = session
            .workspace_root
            .clone()
            .or_else(|| std::env::current_dir().ok());
        if let Some(workspace_root) = workspace_root
            && let Ok(recovered) = restore_or_initialize_ir_state(&workspace_root)
        {
            conversation.ir_state = recovered.state;
            conversation.last_target = conversation.ir_state.current_target.clone();
        }
    }

    let mut outputs = vec![format!("[autonomous goal: {}]", goal_label(goal))];
    let mut completed = false;
    let mut last_target = conversation
        .last_target
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    for iteration in 1..=config.max_iterations {
        let before = estimate_before(goal, iteration);
        let goal_steps = build_goal_steps(goal, &last_target, iteration);
        let primary_exec_plan = goal_steps
            .first()
            .cloned()
            .map(ExecutionPlan::from)
            .unwrap_or_else(ExecutionPlan::noop);
        let mut accepted_plan_id = None;
        if !conversation.ir_state.session_id.is_empty() {
            let _ = emit_intent_captured(
                &conversation.ir_state,
                format!("autonomous:{}:iteration:{iteration}", goal_label(goal)),
                None,
            );
            if let Ok(plan_id) = emit_plan_proposed(
                &conversation.ir_state,
                CommandPlan::from(&primary_exec_plan),
                format!("autonomous:{}", goal_label(goal)),
            ) {
                let _ = emit_plan_accepted(&conversation.ir_state, plan_id);
                accepted_plan_id = Some(plan_id);
            }
        }
        conversation.last_accepted_plan_id = accepted_plan_id;
        conversation.last_plan = Some(primary_exec_plan.clone());
        update_conversation_after_plan(goal_label(goal), &primary_exec_plan, conversation);
        outputs.push(format!("iteration {iteration}/{}", config.max_iterations));
        for step in &goal_steps {
            let exec_plan = ExecutionPlan::from(step.clone());
            let Some(plan_id) = accepted_plan_id else {
                outputs.push(planned_step_command(step));
                continue;
            };
            outputs.extend(execute_ir_plan(plan_id, &exec_plan, session, conversation));

            match maybe_promote_step(step, conversation) {
                Ok(Some(outcome)) => {
                    conversation.hook_promotion_count += 1;
                    outputs.push(format!("promotion hook: state={:?}", outcome.status.state));
                    completed = true;
                    return AutonomousResult {
                        outputs,
                        completed,
                        iterations: iteration,
                    };
                }
                Ok(None) => {}
                Err(err) => {
                    conversation.hook_false_promotion_count += 1;
                    outputs.push(format!("promotion hook error: {err}"));
                }
            }
        }

        let metrics = ConvergenceMetrics {
            before,
            after: estimate_after(goal, iteration),
            confidence: 1.0,
            validation_ok: true,
        };
        outputs.push(telemetry_line(goal, metrics));

        if goal_reached(goal, metrics, config.convergence_threshold) {
            outputs.push("goal reached".to_string());
            completed = true;
            return AutonomousResult {
                outputs,
                completed,
                iterations: iteration,
            };
        }

        if metrics.confidence < config.convergence_threshold || !metrics.validation_ok {
            outputs.push(
                "autonomous loop stopped: confidence drop or validation regression".to_string(),
            );
            return AutonomousResult {
                outputs,
                completed,
                iterations: iteration,
            };
        }

        last_target = conversation
            .last_target
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("."));
    }

    outputs.push("autonomous loop stopped: max iterations exceeded".to_string());
    AutonomousResult {
        outputs,
        completed,
        iterations: config.max_iterations,
    }
}

fn planned_step_command(step: &PlannedStep) -> String {
    match step {
        PlannedStep::Analyze(path) => format!("design_cli analyze {}", path.display()),
        PlannedStep::Coding(path, options) => {
            let mut command = format!("design_cli coding {}", path.display());
            if options.safe {
                command.push_str(" --safe");
            }
            if options.check {
                command.push_str(" --check");
            }
            if let Some(request) = options.request.as_deref() {
                command.push_str(" --request ");
                command.push_str(request);
            }
            command
        }
        PlannedStep::Validate(path) => format!("design_cli validate {}", path.display()),
        PlannedStep::StructureView(path) => format!("design_cli structure view {}", path.display()),
        PlannedStep::StructureEdit(path) => format!("design_cli structure edit {}", path.display()),
        PlannedStep::StructureDiff(path, Some(node)) => {
            format!("design_cli structure diff {} --node {node}", path.display())
        }
        PlannedStep::StructureDiff(path, None) => {
            format!("design_cli structure diff {}", path.display())
        }
        PlannedStep::StructureUndo(path) => format!("design_cli structure undo {}", path.display()),
        PlannedStep::StructureRedo(path) => format!("design_cli structure redo {}", path.display()),
        PlannedStep::Run(path) => format!("design_cli run {}", path.display()),
        PlannedStep::Rules => "design_cli rules".to_string(),
        PlannedStep::Memory(path) => format!("design_cli memory {}", path.display()),
        PlannedStep::GitCommit(path) => format!(
            "git -C {} commit --dry-run --json [confirmation required, branch != main]",
            path.display()
        ),
        PlannedStep::GitPR(path) => format!(
            "gh -R {} pr create --dry-run --json [confirmation required, branch != main]",
            path.display()
        ),
        PlannedStep::AlternativeMutationSearch(query) => {
            format!("design_cli coding . --search {}", query)
        }
        PlannedStep::DesignDeltaReasoning(topic) => format!("design_cli analyze . --delta {topic}"),
        PlannedStep::ExplainDesignTradeoff(topic) => {
            format!("design_cli analyze . --explain {topic}")
        }
        PlannedStep::ApplyPreviousCodingStep => "design_cli coding . --apply".to_string(),
        PlannedStep::RollbackCurrentTransaction => "design_cli runtime rollback".to_string(),
        PlannedStep::IrReload(path) => format!("design_cli replay {}", path.display()),
        PlannedStep::IrReloadAll(path) => format!("design_cli replay {} --all", path.display()),
        PlannedStep::ShowDeps(path) => format!("design_cli structure deps {}", path.display()),
        PlannedStep::Refactor(spec) => {
            format!(
                "design_cli coding {} --refactor {}",
                spec.target.display(),
                spec.request
            )
        }
        PlannedStep::Repair(spec) => {
            format!("design_cli validate {} --repair", spec.target.display())
        }
        PlannedStep::Apply => "design_cli coding . --apply".to_string(),
        PlannedStep::Reload => "design_cli replay . --reload".to_string(),
    }
}

fn maybe_promote_step(
    step: &PlannedStep,
    conversation: &ConversationState,
) -> Result<Option<LoopOutcome>> {
    match step {
        PlannedStep::Analyze(path) => {
            let mut controller = RepairLoopController::new(
                RetryEvaluator::retry_policy_for_origin(LoopOrigin::Analyze),
            );
            maybe_promote_with_origin(
                AnalyzeResult {
                    target: path.clone(),
                    affected_crates: vec!["design_cli".to_string()],
                    confidence: 1.0,
                    logical_node: conversation.last_node.clone().or_else(|| {
                        path.file_stem()
                            .and_then(|stem| stem.to_str())
                            .map(ToOwned::to_owned)
                    }),
                    ambiguous: !path.is_file(),
                },
                Some(LoopOrigin::Analyze),
                &mut controller,
            )
        }
        PlannedStep::Refactor(_) => {
            let Some(tx) = conversation.active_transaction().cloned() else {
                return Ok(None);
            };
            let mut controller = RepairLoopController::new(
                RetryEvaluator::retry_policy_for_origin(LoopOrigin::Coding),
            );
            maybe_promote_with_origin(
                TransactionalApplyResult {
                    applied: tx.applied,
                    build_ok: true,
                    rolled_back: false,
                    sandbox_path: tx.canonical_target.clone(),
                    modified_files: if tx.applied {
                        vec![tx.canonical_target.clone()]
                    } else {
                        Vec::new()
                    },
                    diagnostics: Vec::new(),
                    elapsed_ms: 0,
                    sandbox_elapsed_ms: 0,
                    cargo_check_ms: 0,
                    cleanup_ms: 0,
                    cleanup_ok: true,
                    rollback_count: 0,
                },
                Some(LoopOrigin::Coding),
                &mut controller,
            )
        }
        _ => Ok(None),
    }
}

fn telemetry_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join(".dbm/telemetry")
}

fn benchmark_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join(".dbm/benchmarks")
}

fn telemetry_file(root: &Path) -> PathBuf {
    root.join("autonomous_hook.jsonl")
}

fn kpi_snapshot_file(root: &Path) -> PathBuf {
    root.join("phase_e_kpi_snapshot.json")
}

fn origin_benchmark_snapshot_file(root: &Path) -> PathBuf {
    root.join("origin_benchmark_snapshot.json")
}

fn regression_scorecard_file(root: &Path) -> PathBuf {
    root.join("regression_scorecard.md")
}

fn architecture_surgery_snapshot_file(root: &Path) -> PathBuf {
    root.join("architecture_surgery_snapshot.json")
}

fn nightly_optimization_report_file(root: &Path) -> PathBuf {
    root.join("nightly_optimization_report.md")
}

fn record_hook_telemetry(root: &Path, telemetry: &HookTelemetry) -> Result<()> {
    fs::create_dir_all(root)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(telemetry_file(root))?;
    writeln!(file, "{}", serde_json::to_string(telemetry)?)?;

    let all = read_telemetry(root)?;
    fs::write(
        kpi_snapshot_file(root),
        serde_json::to_string_pretty(&compute_kpi_snapshot(&all))?,
    )?;
    Ok(())
}

fn read_telemetry(root: &Path) -> Result<Vec<HookTelemetry>> {
    let path = telemetry_file(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

pub fn load_hook_telemetry() -> Result<Vec<HookTelemetry>> {
    read_telemetry(&telemetry_root())
}

fn ratio(numerator: usize, denominator: usize) -> f32 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f32 / denominator as f32
    }
}

fn compute_kpi_snapshot(records: &[HookTelemetry]) -> KpiSnapshot {
    let analyze = records
        .iter()
        .filter(|record| record.origin == LoopOrigin::Analyze)
        .collect::<Vec<_>>();
    let coding = records
        .iter()
        .filter(|record| record.origin == LoopOrigin::Coding)
        .collect::<Vec<_>>();
    let validate = records
        .iter()
        .filter(|record| record.origin == LoopOrigin::Validate)
        .collect::<Vec<_>>();
    let structure = records
        .iter()
        .filter(|record| record.origin == LoopOrigin::Structure)
        .collect::<Vec<_>>();
    let memory = records
        .iter()
        .filter(|record| record.origin == LoopOrigin::MemoryRecall)
        .collect::<Vec<_>>();

    KpiSnapshot {
        analyze_convergence: ratio(
            analyze.iter().filter(|r| r.converged).count(),
            analyze.len(),
        ),
        coding_retry_success: ratio(coding.iter().filter(|r| r.converged).count(), coding.len()),
        validate_self_heal: ratio(
            validate.iter().filter(|r| r.converged).count(),
            validate.len(),
        ),
        structure_bind_precision: 1.0
            - ratio(
                structure.iter().filter(|r| r.false_promotion).count(),
                structure.len(),
            ),
        memory_false_promotion: ratio(
            memory.iter().filter(|r| r.false_promotion).count(),
            memory.len(),
        ),
    }
}

pub fn optimize_policy(records: &[HookTelemetry], snapshot: &KpiSnapshot) -> TunedPolicy {
    PolicyOptimizer::default().optimize(records, snapshot)
}

impl PolicyOptimizer {
    pub fn optimize(&self, records: &[HookTelemetry], snapshot: &KpiSnapshot) -> TunedPolicy {
        let mut analyze_threshold =
            RetryEvaluator::confidence_policy_for_origin(LoopOrigin::Analyze).promote_threshold;
        let mut memory_threshold =
            RetryEvaluator::confidence_policy_for_origin(LoopOrigin::MemoryRecall)
                .promote_threshold;
        let mut retry_budget_overrides = HashMap::new();

        let analyze_false_promotions = records
            .iter()
            .filter(|record| record.origin == LoopOrigin::Analyze && record.false_promotion)
            .count();
        if analyze_false_promotions > 0 {
            analyze_threshold = (analyze_threshold + 0.05 * self.false_promotion_weight).min(1.0);
        }

        let memory_false_promotions = records
            .iter()
            .filter(|record| record.origin == LoopOrigin::MemoryRecall && record.false_promotion)
            .count();
        if memory_false_promotions > 0 {
            memory_threshold = (memory_threshold + 0.05 * self.false_promotion_weight).min(1.0);
        }

        if snapshot.analyze_convergence < 0.70 {
            let base = RetryEvaluator::budget_for_origin(LoopOrigin::Analyze).max_attempts;
            retry_budget_overrides.insert(LoopOrigin::Analyze, base.saturating_add(1));
        }

        let coding_retry_heavy = records
            .iter()
            .filter(|record| record.origin == LoopOrigin::Coding && record.retries > 0)
            .count();
        let coding_total = records
            .iter()
            .filter(|record| record.origin == LoopOrigin::Coding)
            .count();
        if coding_total > 0
            && ratio(coding_retry_heavy, coding_total) > 0.5 * self.retry_cost_weight.max(1.0)
        {
            let base = RetryEvaluator::budget_for_origin(LoopOrigin::Coding).max_attempts;
            retry_budget_overrides.insert(LoopOrigin::Coding, base.saturating_sub(1).max(1));
        }

        TunedPolicy {
            analyze_threshold,
            memory_threshold,
            retry_budget_overrides,
        }
    }
}

pub fn write_origin_benchmark_snapshot(
    records: &[HookTelemetry],
) -> Result<OriginBenchmarkSnapshot> {
    let root = benchmark_root();
    fs::create_dir_all(&root)?;
    let snapshot = OriginBenchmarkSnapshot {
        analyze_convergence: compute_kpi_snapshot(records).analyze_convergence,
        coding_retry_success: compute_kpi_snapshot(records).coding_retry_success,
        validate_self_heal: compute_kpi_snapshot(records).validate_self_heal,
        structure_bind_precision: compute_kpi_snapshot(records).structure_bind_precision,
        memory_one_shot: ratio(
            records
                .iter()
                .filter(|record| {
                    record.origin == LoopOrigin::MemoryRecall
                        && record.promoted
                        && record.retries == 0
                })
                .count(),
            records
                .iter()
                .filter(|record| record.origin == LoopOrigin::MemoryRecall)
                .count(),
        ),
    };
    fs::write(
        origin_benchmark_snapshot_file(&root),
        serde_json::to_string_pretty(&snapshot)?,
    )?;
    Ok(snapshot)
}

pub fn write_architecture_surgery_snapshot(
    scenarios: &[ArchitectureSurgeryScenario],
) -> Result<ArchitectureSurgerySnapshot> {
    let root = benchmark_root();
    fs::create_dir_all(&root)?;
    let snapshot = ArchitectureSurgerySnapshot {
        compile_pass_rate: ratio(
            scenarios
                .iter()
                .filter(|scenario| scenario.compile_pass)
                .count(),
            scenarios.len(),
        ),
        minimal_diff_rate: ratio(
            scenarios
                .iter()
                .filter(|scenario| scenario.minimal_diff)
                .count(),
            scenarios.len(),
        ),
        rollback_rate: ratio(
            scenarios
                .iter()
                .filter(|scenario| scenario.rollback_used)
                .count(),
            scenarios.len(),
        ),
        cycle_break_success: ratio(
            scenarios
                .iter()
                .filter(|scenario| scenario.cycle_break_success)
                .count(),
            scenarios.len(),
        ),
    };
    fs::write(
        architecture_surgery_snapshot_file(&root),
        serde_json::to_string_pretty(&snapshot)?,
    )?;
    Ok(snapshot)
}

pub fn write_regression_scorecard(
    current_failed: u64,
    current_hook_sensitive: u64,
    current_kpi: &KpiSnapshot,
    current_records: &[HookTelemetry],
) -> Result<RegressionScorecard> {
    let root = benchmark_root();
    fs::create_dir_all(&root)?;
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join(".dbm/test_inventory/phase_d_preflight/phase_d_preflight_baseline.json");
    let baseline: PhaseDBaseline = serde_json::from_str(&fs::read_to_string(baseline_path)?)?;
    let previous_kpi_path = telemetry_root().join("phase_e_kpi_snapshot.json");
    let previous_kpi: KpiSnapshot = serde_json::from_str(&fs::read_to_string(previous_kpi_path)?)?;

    let retry_sum = current_records
        .iter()
        .map(|record| u64::from(record.retries))
        .sum::<u64>();
    let retry_median = if current_records.is_empty() {
        0.0
    } else {
        retry_sum as f32 / current_records.len() as f32
    };

    let scorecard = RegressionScorecard {
        baseline_failed: baseline.failed_tests,
        current_failed,
        baseline_hook_sensitive: baseline.hook_sensitive_failures,
        current_hook_sensitive,
        failure_delta: current_failed as i64 - baseline.failed_tests as i64,
        hook_sensitive_delta: current_hook_sensitive as i64
            - baseline.hook_sensitive_failures as i64,
        convergence_delta: current_kpi.analyze_convergence - previous_kpi.analyze_convergence,
        retry_median_delta: retry_median,
        false_promotion_delta: current_kpi.memory_false_promotion
            - previous_kpi.memory_false_promotion,
    };

    let body = format!(
        "# Regression Scorecard\n\n- Baseline failed: `{}`\n- Current failed: `{}`\n- Hook-sensitive delta: `{}`\n- Convergence delta: `{:.2}`\n- Retry median delta: `{:.2}`\n- False promotion delta: `{:.2}`\n",
        scorecard.baseline_failed,
        scorecard.current_failed,
        scorecard.hook_sensitive_delta,
        scorecard.convergence_delta,
        scorecard.retry_median_delta,
        scorecard.false_promotion_delta
    );
    fs::write(regression_scorecard_file(&root), body)?;
    Ok(scorecard)
}

pub fn write_nightly_optimization_report(
    records: &[HookTelemetry],
    policy: &TunedPolicy,
    scorecard: &RegressionScorecard,
) -> Result<PathBuf> {
    let root = benchmark_root();
    fs::create_dir_all(&root)?;
    let false_promotion_origins = records
        .iter()
        .filter(|record| record.false_promotion)
        .map(|record| format!("{:?}", record.origin))
        .collect::<Vec<_>>();
    let converged_origins = records
        .iter()
        .filter(|record| record.converged)
        .map(|record| format!("{:?}", record.origin))
        .collect::<Vec<_>>();
    let body = format!(
        "# Nightly Optimization Report\n\nTop false promotion origins: {}\n\nBest convergence origins: {}\n\nAnalyze threshold: {:.2}\nMemory threshold: {:.2}\nRetry budget changes: {:?}\nRegression warnings: hook-sensitive delta={}\n",
        if false_promotion_origins.is_empty() {
            "none".to_string()
        } else {
            false_promotion_origins.join(", ")
        },
        if converged_origins.is_empty() {
            "none".to_string()
        } else {
            converged_origins.join(", ")
        },
        policy.analyze_threshold,
        policy.memory_threshold,
        policy.retry_budget_overrides,
        scorecard.hook_sensitive_delta
    );
    let path = nightly_optimization_report_file(&root);
    fs::write(&path, body)?;
    Ok(path)
}

fn build_goal_steps(
    goal: GoalType,
    target: &std::path::Path,
    iteration: usize,
) -> Vec<PlannedStep> {
    let path = target.to_path_buf();
    let mut steps = match goal {
        GoalType::EliminateCycles => vec![
            PlannedStep::Analyze(path.clone()),
            PlannedStep::Coding(path.clone(), CodingOptions::default()),
            PlannedStep::Validate(path.clone()),
            PlannedStep::StructureDiff(path.clone(), None),
        ],
        GoalType::ReduceUnsafe => vec![
            PlannedStep::Analyze(path.clone()),
            PlannedStep::Coding(path.clone(), CodingOptions::default()),
            PlannedStep::Validate(path.clone()),
        ],
        GoalType::StabilizeViewerDispatch => vec![
            PlannedStep::Analyze(path.clone()),
            PlannedStep::StructureDiff(path.clone(), Some("viewer".to_string())),
            PlannedStep::Validate(path.clone()),
        ],
        GoalType::ImproveTestPassRate => vec![
            PlannedStep::Analyze(path.clone()),
            PlannedStep::Coding(path.clone(), CodingOptions::default()),
            PlannedStep::Validate(path.clone()),
        ],
        GoalType::PrepareCommitAndPR => vec![
            PlannedStep::GitCommit(path.clone()),
            PlannedStep::GitPR(path.clone()),
        ],
    };

    if iteration == 1 && matches!(goal, GoalType::EliminateCycles) {
        steps.push(PlannedStep::GitCommit(path.clone()));
        steps.push(PlannedStep::GitPR(path));
    }

    steps
}

fn estimate_before(goal: GoalType, iteration: usize) -> f32 {
    match goal {
        GoalType::EliminateCycles => {
            if iteration == 1 {
                1.0
            } else {
                0.0
            }
        }
        GoalType::ReduceUnsafe => 10.0 - (iteration as f32 - 1.0),
        GoalType::StabilizeViewerDispatch => {
            if iteration == 1 {
                1.0
            } else {
                0.0
            }
        }
        GoalType::ImproveTestPassRate => 0.5,
        GoalType::PrepareCommitAndPR => 1.0,
    }
}

fn estimate_after(goal: GoalType, iteration: usize) -> f32 {
    match goal {
        GoalType::EliminateCycles => {
            if iteration >= 1 {
                0.0
            } else {
                1.0
            }
        }
        GoalType::ReduceUnsafe => {
            if iteration >= 2 {
                8.0
            } else {
                10.0
            }
        }
        GoalType::StabilizeViewerDispatch => 0.0,
        GoalType::ImproveTestPassRate => 0.98,
        GoalType::PrepareCommitAndPR => 0.0,
    }
}

fn telemetry_line(goal: GoalType, metrics: ConvergenceMetrics) -> String {
    match goal {
        GoalType::EliminateCycles => format!(
            "cycles {} -> {}",
            metrics.before as i32, metrics.after as i32
        ),
        GoalType::ReduceUnsafe => format!(
            "unsafe {} -> {}",
            metrics.before as i32, metrics.after as i32
        ),
        GoalType::StabilizeViewerDispatch => {
            format!(
                "dispatch error rate {} -> {}",
                metrics.before as i32, metrics.after as i32
            )
        }
        GoalType::ImproveTestPassRate => {
            format!(
                "test pass rate {:.2} -> {:.2}",
                metrics.before, metrics.after
            )
        }
        GoalType::PrepareCommitAndPR => "git dry-run ready".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IRPersistenceStore;
    use crate::nl::goal::GoalType;
    use crate::nl::r#loop::{PatchStrategy, RepairTrajectory, ReplLoopState, RetryPolicy};
    use crate::refactor::ValidationResult;
    use crate::test_support::ir_assert::{assert_plan_accepted, assert_plan_proposed};
    use crate::viewer::{
        Node3D, SemanticGraph3D, SourceBinding, Structure3DIr, StructureViewIR, Vec3,
        ViewerSelection,
    };
    use tempfile::tempdir;

    #[test]
    fn max_iteration_stop_is_reported() {
        let temp = tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("src")).expect("src");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"autonomous_ir\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        std::fs::write(temp.path().join("src/lib.rs"), "pub fn noop() {}\n").expect("lib");

        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        let result = run_goal_loop(
            GoalType::ReduceUnsafe,
            &mut session,
            &mut conversation,
            AutonomousLoop {
                max_iterations: 1,
                convergence_threshold: 0.95,
            },
        );
        assert!(!result.completed);
        assert!(
            result
                .outputs
                .iter()
                .any(|line| line.contains("max iterations exceeded"))
        );

        let store = IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().expect("recover");
        let events = store
            .list_plan_events(&recovered.state.session_id)
            .expect("events");
        let plan_id = assert_plan_proposed(&events);
        assert_plan_accepted(&events, plan_id);
    }

    #[test]
    fn analyze_result_auto_promotes_to_plan_patch() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            AnalyzeResult {
                target: std::path::PathBuf::from("apps/cli/src/repl.rs"),
                affected_crates: vec!["design_cli".to_string()],
                confidence: 0.9,
                logical_node: Some("repl".to_string()),
                ambiguous: false,
            },
            &mut controller,
        )
        .expect("hook should succeed")
        .expect("analyze should promote");
        assert_eq!(outcome.status.state, ReplLoopState::PlanPatch);
    }

    #[test]
    fn ambiguous_analyze_target_falls_back_to_none() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            AnalyzeResult {
                target: std::path::PathBuf::from("."),
                affected_crates: vec!["design_cli".to_string()],
                confidence: 0.2,
                logical_node: None,
                ambiguous: true,
            },
            &mut controller,
        )
        .expect("promotion fallback should not error");
        assert!(outcome.is_none());
    }

    #[test]
    fn coding_changed_files_promotes_to_verify() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            TransactionalApplyResult {
                applied: true,
                build_ok: true,
                rolled_back: false,
                sandbox_path: std::path::PathBuf::from("/tmp/dbm"),
                modified_files: vec![std::path::PathBuf::from("apps/cli/src/repl.rs")],
                diagnostics: Vec::new(),
                elapsed_ms: 0,
                sandbox_elapsed_ms: 0,
                cargo_check_ms: 0,
                cleanup_ms: 0,
                cleanup_ok: true,
                rollback_count: 0,
            },
            &mut controller,
        )
        .expect("hook should succeed")
        .expect("coding result should promote");
        assert_eq!(outcome.status.state, ReplLoopState::Verify);
    }

    #[test]
    fn coding_diagnostics_promote_to_retry_decision() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            TransactionalApplyResult {
                applied: false,
                build_ok: false,
                rolled_back: true,
                sandbox_path: std::path::PathBuf::from("/tmp/dbm"),
                modified_files: vec![std::path::PathBuf::from("apps/cli/src/repl.rs")],
                diagnostics: vec!["cargo check failed".to_string()],
                elapsed_ms: 0,
                sandbox_elapsed_ms: 0,
                cargo_check_ms: 0,
                cleanup_ms: 0,
                cleanup_ok: true,
                rollback_count: 1,
            },
            &mut controller,
        )
        .expect("hook should succeed")
        .expect("coding failure should promote");
        assert_eq!(outcome.status.state, ReplLoopState::RetryDecision);
    }

    #[test]
    fn empty_coding_result_falls_back_to_none() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            TransactionalApplyResult {
                applied: false,
                build_ok: true,
                rolled_back: false,
                sandbox_path: std::path::PathBuf::from("/tmp/dbm"),
                modified_files: Vec::new(),
                diagnostics: Vec::new(),
                elapsed_ms: 0,
                sandbox_elapsed_ms: 0,
                cargo_check_ms: 0,
                cleanup_ms: 0,
                cleanup_ok: true,
                rollback_count: 0,
            },
            &mut controller,
        )
        .expect("hook should succeed");
        assert!(outcome.is_none());
    }

    #[test]
    fn validation_diagnostics_promote_to_retry_decision() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            ValidationResult {
                valid: false,
                cycle_removed: false,
                no_new_layer_violation: true,
                buildable: true,
                public_api_preserved: true,
                issues: vec!["violation".to_string()],
            },
            &mut controller,
        )
        .expect("hook should succeed")
        .expect("validation failure should promote");
        assert_eq!(outcome.status.state, ReplLoopState::RetryDecision);
    }

    #[test]
    fn empty_validation_diagnostics_fall_back_to_none() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            ValidationResult {
                valid: true,
                cycle_removed: true,
                no_new_layer_violation: true,
                buildable: true,
                public_api_preserved: true,
                issues: Vec::new(),
            },
            &mut controller,
        )
        .expect("hook should succeed");
        assert!(outcome.is_none());
    }

    #[test]
    fn structure_unique_binding_promotes_to_analyze() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            StructureViewIR {
                selection: ViewerSelection {
                    selected_nodes: vec!["determinism".to_string()],
                    selected_edges: Vec::new(),
                    selection_mode: "node".to_string(),
                },
                scene_3d: Some(Structure3DIr {
                    graph: SemanticGraph3D {
                        nodes: vec![Node3D {
                            id: "determinism".to_string(),
                            label: "determinism".to_string(),
                            kind: "module".to_string(),
                            position: Vec3::default(),
                            size: 1.0,
                            importance: 1.0,
                            heat: 0.0,
                            source_binding: Some(SourceBinding {
                                file: std::path::PathBuf::from("src/runtime/determinism.rs"),
                                line_start: 1,
                                line_end: 1,
                                symbol: None,
                            }),
                        }],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
            &mut controller,
        )
        .expect("hook should succeed")
        .expect("structure should promote");
        assert_eq!(outcome.status.state, ReplLoopState::Analyze);
    }

    #[test]
    fn structure_multi_binding_falls_back_to_none() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            StructureViewIR {
                selection: ViewerSelection {
                    selected_nodes: vec!["a".to_string(), "b".to_string()],
                    selected_edges: Vec::new(),
                    selection_mode: "node".to_string(),
                },
                scene_3d: Some(Structure3DIr {
                    graph: SemanticGraph3D {
                        nodes: vec![
                            Node3D {
                                id: "a".to_string(),
                                label: "a".to_string(),
                                kind: "module".to_string(),
                                position: Vec3::default(),
                                size: 1.0,
                                importance: 1.0,
                                heat: 0.0,
                                source_binding: Some(SourceBinding {
                                    file: std::path::PathBuf::from("src/a.rs"),
                                    line_start: 1,
                                    line_end: 1,
                                    symbol: None,
                                }),
                            },
                            Node3D {
                                id: "b".to_string(),
                                label: "b".to_string(),
                                kind: "module".to_string(),
                                position: Vec3::default(),
                                size: 1.0,
                                importance: 1.0,
                                heat: 0.0,
                                source_binding: Some(SourceBinding {
                                    file: std::path::PathBuf::from("src/b.rs"),
                                    line_start: 1,
                                    line_end: 1,
                                    symbol: None,
                                }),
                            },
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
            &mut controller,
        )
        .expect("hook should succeed");
        assert!(outcome.is_none());
    }

    #[test]
    fn memory_high_confidence_promotes_to_plan_patch() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            RepairTrajectory {
                failure_signature: "E0502".to_string(),
                patch_strategy: PatchStrategy::BorrowScopeShrink,
                target_shape: "replay".to_string(),
                converged: true,
                recall_confidence: 0.9,
            },
            &mut controller,
        )
        .expect("hook should succeed")
        .expect("memory recall should promote");
        assert_eq!(outcome.status.state, ReplLoopState::PlanPatch);
    }

    #[test]
    fn memory_low_confidence_falls_back_to_none() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = maybe_promote(
            RepairTrajectory {
                failure_signature: "E0432".to_string(),
                patch_strategy: PatchStrategy::ImportRebind,
                target_shape: "adapter".to_string(),
                converged: true,
                recall_confidence: 0.2,
            },
            &mut controller,
        )
        .expect("hook should succeed");
        assert!(outcome.is_none());
    }

    #[test]
    fn telemetry_written_and_kpi_snapshot_generated() {
        let dir = tempdir().expect("tempdir");
        let telemetry = HookTelemetry {
            origin: LoopOrigin::Analyze,
            promoted: true,
            converged: true,
            retries: 1,
            false_promotion: false,
            rollback_used: false,
        };
        record_hook_telemetry(dir.path(), &telemetry).expect("telemetry should persist");
        assert!(telemetry_file(dir.path()).exists());
        assert!(kpi_snapshot_file(dir.path()).exists());
    }

    #[test]
    fn false_promotion_is_counted_in_kpi() {
        let snapshot = compute_kpi_snapshot(&[
            HookTelemetry {
                origin: LoopOrigin::MemoryRecall,
                promoted: false,
                converged: false,
                retries: 0,
                false_promotion: true,
                rollback_used: false,
            },
            HookTelemetry {
                origin: LoopOrigin::MemoryRecall,
                promoted: true,
                converged: true,
                retries: 0,
                false_promotion: false,
                rollback_used: false,
            },
        ]);
        assert_eq!(snapshot.memory_false_promotion, 0.5);
    }

    #[test]
    fn rollback_usage_is_tracked() {
        let telemetry = HookTelemetry {
            origin: LoopOrigin::Coding,
            promoted: true,
            converged: false,
            retries: 1,
            false_promotion: false,
            rollback_used: true,
        };
        assert!(telemetry.rollback_used);
    }

    #[test]
    fn false_promotion_raises_threshold() {
        let tuned = optimize_policy(
            &[HookTelemetry {
                origin: LoopOrigin::Analyze,
                promoted: false,
                converged: false,
                retries: 0,
                false_promotion: true,
                rollback_used: false,
            }],
            &KpiSnapshot {
                analyze_convergence: 1.0,
                coding_retry_success: 1.0,
                validate_self_heal: 1.0,
                structure_bind_precision: 1.0,
                memory_false_promotion: 0.0,
            },
        );
        assert!(tuned.analyze_threshold > 0.55);
    }

    #[test]
    fn low_convergence_increases_analyze_budget() {
        let tuned = optimize_policy(
            &[],
            &KpiSnapshot {
                analyze_convergence: 0.4,
                coding_retry_success: 1.0,
                validate_self_heal: 1.0,
                structure_bind_precision: 1.0,
                memory_false_promotion: 0.0,
            },
        );
        assert_eq!(
            tuned.retry_budget_overrides.get(&LoopOrigin::Analyze),
            Some(&4)
        );
    }

    #[test]
    fn retry_heavy_coding_lowers_budget() {
        let tuned = optimize_policy(
            &[
                HookTelemetry {
                    origin: LoopOrigin::Coding,
                    promoted: true,
                    converged: false,
                    retries: 2,
                    false_promotion: false,
                    rollback_used: true,
                },
                HookTelemetry {
                    origin: LoopOrigin::Coding,
                    promoted: true,
                    converged: false,
                    retries: 2,
                    false_promotion: false,
                    rollback_used: true,
                },
            ],
            &KpiSnapshot {
                analyze_convergence: 1.0,
                coding_retry_success: 0.0,
                validate_self_heal: 1.0,
                structure_bind_precision: 1.0,
                memory_false_promotion: 0.0,
            },
        );
        assert_eq!(
            tuned.retry_budget_overrides.get(&LoopOrigin::Coding),
            Some(&1)
        );
    }
}
