use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::coding::{
    CodeChangeSet, CodingExecutionResult, CodingOptions as ExecutionCodingOptions, DiffLine,
    build_unified_diff_preview, create_validation_sandbox, execute_code_change_set,
    generate_code_change_set, render_code_diff_lines,
};
use crate::design_delta::{self, explain};
use crate::ir::{
    ArtifactRef, ExecutionStatus, IRPersistenceArtifact, MemoryContext, MemoryOutcome,
    StepExecutionResultPayload, assert_accepted_plan_exists, emit_artifact_produced,
    emit_execution_result, emit_memory_outcome, emit_memory_referenced, emit_step_completed,
    emit_step_scheduled, emit_step_started, log_ir_bypass_warning, persist_ir_transition,
    query_memory_context, store_execution_memory,
};
use crate::nl::session::ConversationState;
use crate::nl::types::{CodingIntent, CodingOptions, CommandPlan, PlannedStep};
use crate::nl_executor::run_design_command;
use crate::refactor::{
    GuiAction, GuiActionMode, PatchScope, RefactorTarget, create_refactor_plan, rollback_apply,
    snapshot_workspace,
};
use crate::runner::{fixed_env, resolve_command};
use crate::service::{
    AnalysisDependency, AnalysisModule, AnalysisReport, analyze_path,
    dto::{ActionKind, SessionAppliedDiff, SessionAppliedFileDiff},
};
use crate::session::AgentSession;
use crate::state::State;
use uuid::Uuid;

const DESIGN_DELTA_MIN_CHANGED_LINES: usize = 2;
const DESIGN_DELTA_MAX_FILES: usize = 5;
const DESIGN_DELTA_HEURISTIC_SIGNAL_THRESHOLD: usize = 2;
const WORLD_MODEL_TOP_K: usize = 3;
const MULTISTEP_TOP_K: usize = 5;
const MULTISTEP_MAX_DEPTH: usize = 3;
const MULTISTEP_MAX_BRANCH: usize = 3;
const MULTISTEP_BEAM_WIDTH: usize = 3;
const MULTISTEP_MAX_TOTAL_NODES: usize = 20;
const TOTAL_EXPLORATION_TOP_K: usize = 10;
const SELECTION_W1_SYMBOLIC: f32 = 0.1;
const SELECTION_W2_EMBEDDING: f32 = 0.1;
const SELECTION_W3_COMPILE: f32 = 0.3;
const SELECTION_W4_TEST: f32 = 0.2;
const SELECTION_W5_SEMANTIC: f32 = 0.1;
const SELECTION_W6_INTENT: f32 = 0.1;
const SELECTION_W7_SUCCESS_RATE: f32 = 0.1;
const SELECTION_W_ERROR_PENALTY: f32 = 0.05;
const SELECTION_W_DIFF_PENALTY: f32 = 0.05;
const TEST_SUCCESS_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CandidateOrigin {
    Memory,
    RuleBased,
    WorldModel,
}

#[derive(Debug, Clone)]
struct CandidatePatch {
    memory_id: String,
    origin: CandidateOrigin,
    patch: CodeChangeSet,
    symbolic_score: f32,
    embedding_score: f32,
    intent_score: f32,
    success_rate: f32,
    timestamp: u64,
    target: RefactorTarget,
    canonical_target: PathBuf,
    diff_preview: crate::coding::UnifiedDiffPreview,
    snapshot: SessionAppliedDiff,
    pre_validation: DesignDeltaValidation,
}

#[derive(Debug, Clone, PartialEq)]
struct SimulationResult {
    compile_ok: bool,
    error_count: usize,
    warnings: usize,
    diff_size: usize,
    test_result: TestResult,
    semantic_score: f32,
    intent_score: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct EvaluationScore {
    total_score: f32,
    compile_score: f32,
    test_score: f32,
    semantic_score: f32,
    intent_score: f32,
    error_penalty: f32,
    diff_penalty: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TestResult {
    passed: usize,
    failed: usize,
    skipped: usize,
}

struct ExplorationInput<'a> {
    intent: Option<&'a CodingIntent>,
    context: &'a AnalysisReport,
    spec: &'a str,
    delta: &'a design_delta::DesignDelta,
    step_index: usize,
    memory_context: &'a MemoryContext,
}

#[derive(Debug, Clone)]
struct GeneratedCandidate {
    target: RefactorTarget,
    origin: CandidateOrigin,
    actions: Vec<ExplorationAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExplorationAction {
    kind: ActionKind,
    target_node: String,
    target: RefactorTarget,
}

#[derive(Debug, Clone)]
struct ExplorationState {
    applied_actions: Vec<ExplorationAction>,
    depth: usize,
    origin: CandidateOrigin,
    heuristic_score: f32,
}

impl GeneratedCandidate {
    fn single(target: RefactorTarget, origin: CandidateOrigin) -> Self {
        let action = exploration_action_for_target(&target);
        Self {
            target,
            origin,
            actions: vec![action],
        }
    }
}

trait CandidateGenerator {
    fn generate(&self, input: &ExplorationInput<'_>) -> Vec<GeneratedCandidate>;
}

struct RuleBasedGenerator;

struct WorldModelGenerator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyOutcome {
    CompileSuccess,
    CompileWithWarnings,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesignDeltaValidation {
    meaningful: bool,
    within_scope: bool,
    heuristic_allowed: bool,
    already_applied: bool,
    ir_consistent: bool,
    dependency_improved: bool,
    build_ok: bool,
    rolled_back: bool,
    issues: Vec<String>,
}

impl DesignDeltaValidation {
    fn success() -> Self {
        Self {
            meaningful: true,
            within_scope: true,
            heuristic_allowed: true,
            already_applied: false,
            ir_consistent: true,
            dependency_improved: true,
            build_ok: true,
            rolled_back: false,
            issues: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffMeaning {
    changed_lines: usize,
    meaningful_lines: usize,
    comment_only: bool,
    import_only: bool,
}

pub fn render_plan_summary(plan: &CommandPlan) -> String {
    render_plan_summary_with_label(plan, "nl_v2")
}

pub fn render_plan_summary_with_label(plan: &CommandPlan, label: &str) -> String {
    format!("[planner: {label}] {} steps", plan.steps.len())
}

pub fn describe_plan_labels(plan: &CommandPlan) -> Vec<String> {
    plan.steps
        .iter()
        .map(|step| to_canonical_command(step).2)
        .collect()
}

#[deprecated(note = "Use execute_ir_plan instead")]
pub fn execute_plan(
    _plan: &CommandPlan,
    _session: &mut AgentSession,
    _conversation: &mut ConversationState,
) -> Vec<String> {
    panic!("Forbidden: direct execution path. Use IR-based execution.");
}

pub fn execute_ir_plan(
    plan_id: Uuid,
    plan: &CommandPlan,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> Vec<String> {
    if let Err(err) = assert_accepted_plan_exists(&conversation.ir_state, plan_id) {
        log_ir_bypass_warning("execution attempted without plan_id");
        panic!("{err}");
    }

    let mut outputs = Vec::new();

    for (index, step) in plan.steps.iter().enumerate() {
        if cfg!(test)
            && let Some(snapshot) = executor_received_path_snapshot(step)
        {
            outputs.push(format!("[step {index}] {snapshot}"));
        }

        // ── Phase 2: StepScheduled ──────────────────────────────────────────
        let step_id = emit_step_scheduled(
            &conversation.ir_state,
            plan_id,
            index,
            step_kind_label(step),
        )
        .unwrap_or_else(|_| uuid::Uuid::new_v4());

        let memory_context =
            query_memory_context(&conversation.ir_state, step, index).unwrap_or_default();
        let _ = emit_memory_referenced(&conversation.ir_state, step_id, memory_context.ids());

        let _ = emit_step_started(&conversation.ir_state, step_id);
        // ───────────────────────────────────────────────────────────────────

        // R2: ApplyPreviousCodingStep は generic executor を bypass して直接処理する。
        if matches!(step, PlannedStep::ApplyPreviousCodingStep) {
            let output = execute_apply_previous_coding_step(conversation);
            let success = output.starts_with("Applied: true");
            let _ = emit_step_completed(
                &conversation.ir_state,
                step_id,
                if success {
                    ExecutionStatus::Success
                } else {
                    ExecutionStatus::Failure
                },
            );
            let _ = emit_execution_result(
                &conversation.ir_state,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            record_execution_memory(
                &conversation.ir_state,
                step,
                index,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            outputs.push(format!("[step {index}] apply_previous_coding\n{output}"));
            continue;
        }
        if matches!(step, PlannedStep::RollbackCurrentTransaction) {
            let output = execute_ir_rollback(conversation);
            let rollback_artifact = ArtifactRef {
                artifact_kind: "rollback".to_string(),
                artifact_id: format!("rollback:{step_id}"),
                description: Some("transaction rollback".to_string()),
            };
            let _ = emit_artifact_produced(&conversation.ir_state, rollback_artifact.clone());
            let _ = emit_artifact_rolled_back_for_step(&conversation.ir_state, rollback_artifact);
            let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Success);
            let _ = emit_execution_result(
                &conversation.ir_state,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            record_execution_memory(
                &conversation.ir_state,
                step,
                index,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            outputs.push(format!(
                "[step {index}] rollback_current_transaction\n{output}"
            ));
            continue;
        }

        if let PlannedStep::DesignDeltaReasoning(spec) = step {
            let output = execute_design_delta_reasoning_step(
                spec,
                Some(step_id),
                index,
                &memory_context,
                session,
                conversation,
            );
            let status = if output.starts_with("Error") {
                ExecutionStatus::Failure
            } else {
                ExecutionStatus::Success
            };
            let _ = emit_step_completed(&conversation.ir_state, step_id, status);
            let _ = emit_execution_result(
                &conversation.ir_state,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            record_execution_memory(
                &conversation.ir_state,
                step,
                index,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            outputs.push(format!("[step {index}] design_delta_reasoning\n{output}"));
            continue;
        }
        if let PlannedStep::AlternativeMutationSearch(spec) = step {
            let output = execute_alternative_mutation_search_step(spec, session, conversation);
            let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Success);
            let _ = emit_execution_result(
                &conversation.ir_state,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            record_execution_memory(
                &conversation.ir_state,
                step,
                index,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            outputs.push(format!(
                "[step {index}] alternative_mutation_search\n{output}"
            ));
            continue;
        }
        if let PlannedStep::ExplainDesignTradeoff(prompt) = step {
            let output = execute_design_tradeoff_explanation_step(prompt, session, conversation);
            let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Success);
            let _ = emit_execution_result(
                &conversation.ir_state,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            record_execution_memory(
                &conversation.ir_state,
                step,
                index,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            outputs.push(format!("[step {index}] explain_design_tradeoff\n{output}"));
            continue;
        }

        if let Err(err) = ensure_viewer_session_is_fresh(step, conversation) {
            let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Failure);
            let _ = emit_execution_result(
                &conversation.ir_state,
                StepExecutionResultPayload {
                    step_id,
                    stdout: None,
                    stderr: Some(err.clone()),
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            record_execution_memory(
                &conversation.ir_state,
                step,
                index,
                StepExecutionResultPayload {
                    step_id,
                    stdout: None,
                    stderr: Some(err.clone()),
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            );
            outputs.push(format!("[step {index}] Error: {err}"));
            break;
        }
        let (command, args, label, skip_exec) = to_canonical_command(step);
        let result = if skip_exec {
            Ok(String::new())
        } else {
            run_design_command(command, &args)
        };
        match result {
            Ok(output) => {
                // Coding step の dry-run 完了後、transaction を記録する (R3)。
                let mut step_artifacts: Vec<ArtifactRef> = Vec::new();
                if let PlannedStep::Coding(path, opts) = step {
                    if opts.check {
                        record_coding_transaction(path, opts, &output, conversation);
                        let artifact = ArtifactRef {
                            artifact_kind: "code_diff".to_string(),
                            artifact_id: format!("diff:{step_id}"),
                            description: Some(format!("coding preview: {}", path.display())),
                        };
                        let _ = emit_artifact_produced(&conversation.ir_state, artifact.clone());
                        step_artifacts.push(artifact);
                    }
                }
                update_state_after_step(step, conversation);
                let _ =
                    emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Success);
                let result_payload = StepExecutionResultPayload {
                    step_id,
                    stdout: Some(output.clone()),
                    stderr: None,
                    structured_output: None,
                    artifacts: step_artifacts,
                };
                let _ = emit_execution_result(&conversation.ir_state, result_payload.clone());
                record_execution_memory(&conversation.ir_state, step, index, result_payload);
                outputs.push(format!("[step {index}] {label}\n{output}"));
            }
            Err(err) => {
                let _ =
                    emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Failure);
                let _ = emit_execution_result(
                    &conversation.ir_state,
                    StepExecutionResultPayload {
                        step_id,
                        stdout: None,
                        stderr: Some(err.clone()),
                        structured_output: None,
                        artifacts: Vec::new(),
                    },
                );
                record_execution_memory(
                    &conversation.ir_state,
                    step,
                    index,
                    StepExecutionResultPayload {
                        step_id,
                        stdout: None,
                        stderr: Some(err.clone()),
                        structured_output: None,
                        artifacts: Vec::new(),
                    },
                );
                outputs.push(format!("[step {index}] Error: {err}"));
                break;
            }
        }
    }

    outputs
}

fn record_execution_memory(
    ir_state: &crate::service::dto::IRState,
    step: &PlannedStep,
    step_index: usize,
    result: StepExecutionResultPayload,
) {
    let _ = store_execution_memory(ir_state, step, step_index, &result);
}

fn emit_artifact_rolled_back_for_step(
    ir_state: &crate::service::dto::IRState,
    artifact: ArtifactRef,
) -> Result<String, String> {
    use crate::ir::emit_artifact_rolled_back;
    emit_artifact_rolled_back(ir_state, artifact)
}

fn step_kind_label(step: &PlannedStep) -> &'static str {
    match step {
        PlannedStep::Analyze(_) => "analyze",
        PlannedStep::Coding(_, _) => "coding",
        PlannedStep::Validate(_) => "validate",
        PlannedStep::StructureView(_) => "structure_view",
        PlannedStep::StructureEdit(_) => "structure_edit",
        PlannedStep::StructureDiff(_, _) => "structure_diff",
        PlannedStep::StructureUndo(_) => "structure_undo",
        PlannedStep::StructureRedo(_) => "structure_redo",
        PlannedStep::Run(_) => "run",
        PlannedStep::Rules => "rules",
        PlannedStep::Memory(_) => "memory",
        PlannedStep::GitCommit(_) => "git_commit",
        PlannedStep::GitPR(_) => "git_pr",
        PlannedStep::AlternativeMutationSearch(_) => "alternative_mutation_search",
        PlannedStep::DesignDeltaReasoning(_) => "design_delta_reasoning",
        PlannedStep::ExplainDesignTradeoff(_) => "explain_design_tradeoff",
        PlannedStep::ApplyPreviousCodingStep => "apply_previous_coding",
        PlannedStep::RollbackCurrentTransaction => "rollback_current_transaction",
    }
}

pub fn executor_received_path_snapshot(step: &PlannedStep) -> Option<String> {
    match step {
        PlannedStep::Analyze(path)
        | PlannedStep::Coding(path, _)
        | PlannedStep::Validate(path)
        | PlannedStep::StructureView(path)
        | PlannedStep::StructureEdit(path)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path)
        | PlannedStep::Run(path)
        | PlannedStep::Memory(path)
        | PlannedStep::GitCommit(path)
        | PlannedStep::GitPR(path) => Some(format!(
            "snapshot executor received path: {}",
            path.display()
        )),
        PlannedStep::StructureDiff(path, _) => Some(format!(
            "snapshot executor received path: {}",
            path.display()
        )),
        PlannedStep::Rules
        | PlannedStep::ApplyPreviousCodingStep
        | PlannedStep::RollbackCurrentTransaction
        | PlannedStep::DesignDeltaReasoning(_)
        | PlannedStep::AlternativeMutationSearch(_)
        | PlannedStep::ExplainDesignTradeoff(_) => None,
    }
}

/// R4: 前回 checked coding transaction を --apply へ昇格して実行する。
///
/// R5: patch_count == 0 → no-op
/// R6: applied == true → no-op
fn execute_apply_previous_coding_step(conversation: &mut ConversationState) -> String {
    let tx = match conversation.active_transaction() {
        Some(tx) => tx.clone(),
        None => return "Applied: false\nReason: no previous coding transaction".to_string(),
    };

    if tx.applied && !tx.pending {
        return "Applied: false\nReason: already applied".to_string();
    }

    let Some((path, request, safe)) = last_preview_coding_context(conversation) else {
        return "Applied: false\nReason: missing preview context".to_string();
    };

    // R4: --check → --apply の deterministic 変換。canonical path / request は last_plan から再構築する。
    let path_str = path.display().to_string();
    let is_file_target =
        path_str.ends_with(".rs") || path_str.ends_with(".toml") || path_str.ends_with(".md");
    let mut args = if is_file_target {
        vec![".".to_string(), "--target".to_string(), path_str]
    } else {
        vec![path_str]
    };
    if let Some(request) = request {
        args.push("--request".to_string());
        args.push(request);
    }
    if safe {
        args.push("--safe".to_string());
    }
    args.push("--apply".to_string());

    match run_design_command("coding", &args) {
        Ok(output) => {
            let before = conversation.ir_state.clone();
            conversation.mark_transaction_applied(None);
            conversation.note_target(tx.canonical_target.clone());
            let _ = persist_ir_transition(
                &before,
                &conversation.ir_state,
                crate::service::dto::ActionKind::Apply,
                "apply_previous_coding",
                IRPersistenceArtifact::default(),
            );
            format!("Applied: true\n{output}")
        }
        Err(err) => format!("Applied: false\nReason: {err}"),
    }
}

fn last_preview_coding_context(
    conversation: &ConversationState,
) -> Option<(PathBuf, Option<String>, bool)> {
    let step = conversation
        .last_plan
        .as_ref()?
        .steps
        .iter()
        .rev()
        .find_map(|step| {
            if let PlannedStep::Coding(path, opts) = step {
                Some((path.clone(), opts.clone()))
            } else {
                None
            }
        })?;
    Some((step.0, step.1.request, step.1.safe))
}

fn execute_ir_rollback(conversation: &mut ConversationState) -> String {
    let before = conversation.ir_state.clone();
    conversation.rollback_current_transaction();
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        crate::service::dto::ActionKind::Rollback,
        "rollback_current_transaction",
        IRPersistenceArtifact::default(),
    );
    "[rollback] reverted current IR transaction".to_string()
}

fn record_coding_transaction(
    path: &PathBuf,
    opts: &CodingOptions,
    output: &str,
    conversation: &mut ConversationState,
) {
    let _ = serde_json::from_str::<Value>(output).ok();
    let _ = opts;
    let before = conversation.ir_state.clone();
    conversation.start_preview_transaction(path.clone());
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        crate::service::dto::ActionKind::CodingPreview,
        format!("preview {}", path.display()),
        IRPersistenceArtifact::default(),
    );
}

fn to_canonical_command(step: &PlannedStep) -> (&'static str, Vec<String>, String, bool) {
    match step {
        PlannedStep::Analyze(path) => (
            "analyze",
            vec![path.display().to_string()],
            format!("design_cli analyze {}", path.display()),
            false,
        ),
        PlannedStep::Coding(path, options) => {
            let path_str = path.display().to_string();
            let is_file_target = path_str.ends_with(".rs")
                || path_str.ends_with(".toml")
                || path_str.ends_with(".md");
            let mut args = if is_file_target {
                vec![".".to_string(), "--target".to_string(), path_str.clone()]
            } else {
                vec![path_str.clone()]
            };
            if let Some(request) = &options.request {
                args.push("--request".to_string());
                args.push(request.clone());
            }
            if options.safe {
                args.push("--safe".to_string());
            }
            if options.check {
                args.push("--check".to_string());
            }
            (
                "coding",
                args,
                if is_file_target {
                    format!(
                        "design_cli coding . --target {} --safe --check",
                        path.display()
                    )
                } else {
                    format!("design_cli coding {} --safe --check", path.display())
                },
                false,
            )
        }
        PlannedStep::Validate(path) => (
            "validate",
            vec![path.display().to_string()],
            format!("design_cli validate {}", path.display()),
            false,
        ),
        PlannedStep::StructureView(path) => (
            "structure",
            vec![
                "view".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure view {}", path.display()),
            false,
        ),
        PlannedStep::StructureEdit(path) => (
            "structure",
            vec![
                "edit".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure edit {}", path.display()),
            false,
        ),
        PlannedStep::StructureDiff(path, node) => (
            "structure",
            build_structure_diff_args(path, node.as_deref()),
            format!(
                "design_cli structure dispatch {} --event <generated diff>",
                path.display()
            ),
            false,
        ),
        PlannedStep::StructureUndo(path) => (
            "structure",
            vec![
                "undo".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure undo {}", path.display()),
            false,
        ),
        PlannedStep::StructureRedo(path) => (
            "structure",
            vec![
                "redo".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure redo {}", path.display()),
            false,
        ),
        PlannedStep::Run(path) => (
            "run",
            vec![path.display().to_string()],
            format!("design_cli run {}", path.display()),
            false,
        ),
        PlannedStep::Rules => (
            "rules",
            vec!["list".to_string()],
            "design_cli rules list".to_string(),
            false,
        ),
        PlannedStep::Memory(path) => (
            "memory",
            vec!["import".to_string(), path.display().to_string()],
            format!("design_cli memory import {}", path.display()),
            false,
        ),
        PlannedStep::GitCommit(path) => (
            "execute",
            vec![
                "commit changes".to_string(),
                "--path".to_string(),
                path.display().to_string(),
                "--dry-run".to_string(),
                "--json".to_string(),
            ],
            format!(
                "design_cli execute \"commit changes\" --path {} --dry-run --json [confirmation required, branch != main]",
                path.display()
            ),
            true,
        ),
        PlannedStep::GitPR(path) => (
            "execute",
            vec![
                "push and create pr".to_string(),
                "--path".to_string(),
                path.display().to_string(),
                "--dry-run".to_string(),
                "--auto-remote".to_string(),
                "--json".to_string(),
            ],
            format!(
                "design_cli execute \"push and create pr\" --path {} --dry-run --auto-remote --json [confirmation required, branch != main]",
                path.display()
            ),
            true,
        ),
        PlannedStep::DesignDeltaReasoning(spec) => (
            "design-delta",
            vec![spec.clone()],
            "design delta reasoning loop".to_string(),
            true,
        ),
        PlannedStep::AlternativeMutationSearch(spec) => (
            "design-delta-search",
            vec![spec.clone()],
            "alternative mutation search loop".to_string(),
            true,
        ),
        PlannedStep::ExplainDesignTradeoff(prompt) => (
            "design-delta-explain",
            vec![prompt.clone()],
            "design tradeoff explanation loop".to_string(),
            true,
        ),
        // ApplyPreviousCodingStep は execute_plan で直接処理されるため、
        // to_canonical_command には到達しない。describe_plan_labels 用の記述のみ。
        PlannedStep::ApplyPreviousCodingStep => (
            "coding",
            vec!["--apply".to_string()],
            "design_cli coding --apply [previous transaction]".to_string(),
            true,
        ),
        PlannedStep::RollbackCurrentTransaction => (
            "rollback",
            Vec::new(),
            "design_cli rollback [ir transaction]".to_string(),
            true,
        ),
    }
}

fn build_structure_diff_args(path: &PathBuf, node: Option<&str>) -> Vec<String> {
    let event = GuiAction {
        action: "refactor".to_string(),
        target: "auto".to_string(),
        node: node.map(|node| node.to_string()),
        project_root: None,
        selected_nodes: Vec::new(),
        selected_edges: Vec::new(),
        mode: GuiActionMode::Preview,
    };
    let event_path = std::env::temp_dir().join(format!(
        "design_cli_structure_diff_{}.json",
        uuid::Uuid::new_v4()
    ));
    if let Ok(serialized) = serde_json::to_string(&event) {
        let _ = std::fs::write(&event_path, serialized);
    }
    vec![
        "dispatch".to_string(),
        path.display().to_string(),
        "--event".to_string(),
        event_path.display().to_string(),
        "--json".to_string(),
    ]
}

fn update_state_after_step(step: &PlannedStep, conversation: &mut ConversationState) {
    match step {
        PlannedStep::Analyze(path)
        | PlannedStep::Coding(path, _)
        | PlannedStep::Validate(path)
        | PlannedStep::StructureView(path)
        | PlannedStep::StructureEdit(path)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path)
        | PlannedStep::Run(path)
        | PlannedStep::Memory(path)
        | PlannedStep::GitCommit(path)
        | PlannedStep::GitPR(path) => conversation.last_target = Some(path.clone()),
        PlannedStep::DesignDeltaReasoning(_)
        | PlannedStep::AlternativeMutationSearch(_)
        | PlannedStep::ExplainDesignTradeoff(_)
        | PlannedStep::RollbackCurrentTransaction => {}
        PlannedStep::StructureDiff(path, node) => {
            conversation.last_target = Some(path.clone());
            if let Some(node) = node {
                conversation.last_node = Some(node.clone());
            }
        }
        PlannedStep::Rules => {}
        // ApplyPreviousCodingStep は execute_plan 内で直接処理されるため、
        // update_state_after_step には到達しない。last_target は apply 実行時に更新済み。
        PlannedStep::ApplyPreviousCodingStep => {}
    }

    if matches!(
        step,
        PlannedStep::StructureView(_)
            | PlannedStep::StructureEdit(_)
            | PlannedStep::StructureDiff(_, _)
            | PlannedStep::StructureUndo(_)
            | PlannedStep::StructureRedo(_)
    ) && let Some(path) = &conversation.last_target
        && let Ok(raw) = run_design_command(
            "structure",
            &[
                "session".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
        )
        && let Ok(json) = serde_json::from_str::<Value>(&raw)
        && let Some(session_id) = json.get("session_id").and_then(Value::as_str)
    {
        conversation.last_viewer_session = Some(session_id.to_string());
    }
}

fn execute_design_delta_reasoning_step(
    spec: &str,
    step_id: Option<Uuid>,
    step_index: usize,
    memory_context: &MemoryContext,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    log::debug!("Executing DesignDeltaReasoning");
    let root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));

    session.state = State::SpecReceived;
    let output = match design_delta::run_reasoning_loop(&root, spec) {
        Ok(output) => output,
        Err(err) => return format!("Accepted: false\nReason: {err}"),
    };

    session.state = State::DesignDeltaReady;
    session.design_baseline = Some(output.baseline.clone());
    conversation.last_design_delta = Some(output.delta.clone());

    session.state = State::MutationPlanned;
    session.active_mutation_plan = Some(output.mutation_plan.clone());
    conversation.active_mutation_plan = Some(output.mutation_plan.clone());

    session.state = State::RationalityScored;
    session.last_rationality_score = Some(output.rationality.clone());
    conversation.last_rationality_score = Some(output.rationality.clone());

    if !output.accepted {
        session.state = State::Error;
        return format!(
            "Accepted: false\nRationality total: {:.2}\nFallback retries: {}\nReason: below threshold",
            output.rationality.total, output.retries
        );
    }

    session.state = State::PatchPlanReady;
    conversation.last_patch_plan = Some(output.patch_plan.clone());
    conversation.last_analysis_summary = Some(output.patch_plan.summary.clone());

    let mut lines = vec![
        "Accepted: true".to_string(),
        format!(
            "Impacted crates: {}",
            output.delta.impacted_crates.join(", ")
        ),
        format!("Rationality total: {:.2}", output.rationality.total),
        format!(
            "Expected tests: {}",
            output.patch_plan.expected_tests.join(" | ")
        ),
    ];

    session.state = State::TestPlanReady;
    let execution_output = execute_design_delta_refactor(
        spec,
        step_id,
        step_index,
        memory_context,
        &output.delta,
        &output.mutation_plan,
        conversation,
    );
    if execution_output.contains("Error:") {
        session.state = State::Repairing;
    } else {
        session.state = State::CommitReady;
    }
    lines.push(execution_output);
    lines.join("\n")
}

fn execute_alternative_mutation_search_step(
    spec: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    let root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));

    session.state = State::SpecReceived;
    let output = match design_delta::run_alternative_search_loop(&root, spec) {
        Ok(output) => output,
        Err(err) => return format!("Accepted: false\nReason: {err}"),
    };

    session.state = State::MutationCandidatesReady;
    if let Some(search_result) = &output.search_result {
        session.mutation_candidates = search_result.candidates.clone();
        conversation.mutation_candidates = search_result.candidates.clone();
        session.mutation_search_depth = 3;
        conversation.mutation_search_depth = 3;
        session.last_mutation_search_result = Some(search_result.clone());
        conversation.last_mutation_search_result = Some(search_result.clone());
    }

    session.state = State::MutationRankingReady;
    if let Some(search_result) = &output.search_result
        && let Some(selected) = search_result.selected.clone()
    {
        session.selected_mutation = Some(selected.clone());
        conversation.selected_mutation = Some(selected);
    }

    session.state = State::BestMutationSelected;
    session.design_baseline = Some(output.baseline.clone());
    conversation.last_design_delta = Some(output.delta.clone());
    session.active_mutation_plan = Some(output.mutation_plan.clone());
    conversation.active_mutation_plan = Some(output.mutation_plan.clone());
    session.last_rationality_score = Some(output.rationality.clone());
    conversation.last_rationality_score = Some(output.rationality.clone());

    if !output.accepted {
        session.state = State::Error;
        return format!(
            "Accepted: false\nCandidates: {}\nRationality total: {:.2}\nReason: below search threshold",
            output
                .search_result
                .as_ref()
                .map(|r| r.candidates.len())
                .unwrap_or(0),
            output.rationality.total
        );
    }

    session.state = State::PatchPlanReady;
    conversation.last_patch_plan = Some(output.patch_plan.clone());
    conversation.last_analysis_summary = Some(output.patch_plan.summary.clone());

    let selected_id = session
        .selected_mutation
        .as_ref()
        .map(|candidate| candidate.id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let mut lines = vec![
        format!(
            "Candidates: {}",
            output
                .search_result
                .as_ref()
                .map(|r| r.candidates.len())
                .unwrap_or(0)
        ),
        format!("Selected mutation: {selected_id}"),
        format!("Rationality total: {:.2}", output.rationality.total),
    ];

    session.state = State::TestPlanReady;
    let execution_output = execute_design_delta_refactor(
        spec,
        None,
        0,
        &MemoryContext::default(),
        &output.delta,
        &output.mutation_plan,
        conversation,
    );
    if execution_output.contains("Error:") {
        session.state = State::Repairing;
    } else {
        session.state = State::CommitReady;
    }
    lines.push(execution_output);
    lines.join("\n")
}

fn execute_design_delta_refactor(
    spec: &str,
    step_id: Option<Uuid>,
    step_index: usize,
    memory_context: &MemoryContext,
    delta: &design_delta::DesignDelta,
    mutation_plan: &design_delta::MutationPlan,
    conversation: &mut ConversationState,
) -> String {
    log::debug!("Executing DesignDeltaReasoning");
    let root = if delta.workspace_root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        delta.workspace_root.clone()
    };
    let analysis_before = match analyze_path(&root) {
        Ok(report) => report,
        Err(err) => return format!("Error: failed to analyze design delta target: {err}"),
    };
    let active_intent = conversation
        .last_plan
        .as_ref()
        .and_then(|plan| plan.intent.clone());
    let candidates = expand_candidate_patches(
        spec,
        step_index,
        memory_context,
        active_intent.as_ref(),
        delta,
        mutation_plan,
        &analysis_before,
        conversation,
    );
    if candidates.is_empty() {
        return "No changes detected.".to_string();
    }
    let evaluated = candidates
        .into_iter()
        .map(|candidate| {
            let simulation = simulate_candidate_patch(&root, &candidate);
            let evaluation = evaluate_candidate_patch(&candidate, &simulation);
            (candidate, simulation, evaluation)
        })
        .collect::<Vec<_>>();
    let Some((best, simulation, evaluation)) = select_best_candidate_patch(evaluated) else {
        return "No changes detected.".to_string();
    };
    log::debug!(
        "Selected candidate {} with score {:?}",
        best.memory_id,
        evaluation
    );
    if !best.pre_validation.issues.is_empty() {
        return render_validation_result(&best.pre_validation);
    }
    if !simulation.compile_ok {
        let validation = DesignDeltaValidation {
            build_ok: false,
            rolled_back: true,
            issues: vec![format!(
                "selection rejected candidate {}: simulation failed",
                best.memory_id
            )],
            ..DesignDeltaValidation::success()
        };
        return render_validation_result(&validation);
    }
    if !simulation_passes_validation(&simulation) {
        let validation = DesignDeltaValidation {
            build_ok: false,
            rolled_back: true,
            issues: vec![format!(
                "selection rejected candidate {}: semantic validation failed",
                best.memory_id
            )],
            ..DesignDeltaValidation::success()
        };
        return render_validation_result(&validation);
    }

    apply_selected_candidate(
        &root,
        step_id,
        &simulation,
        spec,
        mutation_plan,
        &analysis_before,
        best,
        conversation,
    )
}

fn simulation_passes_validation(simulation: &SimulationResult) -> bool {
    simulation.compile_ok
        && simulation.test_result.failed == 0
        && test_score(&simulation.test_result) > TEST_SUCCESS_THRESHOLD
        && simulation.semantic_score > 0.0
        && simulation.intent_score > 0.0
}

fn apply_selected_candidate(
    root: &Path,
    step_id: Option<Uuid>,
    simulation: &SimulationResult,
    _spec: &str,
    _mutation_plan: &design_delta::MutationPlan,
    analysis_before: &AnalysisReport,
    candidate: CandidatePatch,
    conversation: &mut ConversationState,
) -> String {
    let diff_text = render_design_delta_diff(&candidate.diff_preview);
    let before = conversation.ir_state.clone();
    conversation.start_preview_transaction(candidate.canonical_target.clone());
    let rollback_snapshot = match snapshot_workspace(
        root,
        &candidate
            .patch
            .changes
            .iter()
            .map(|change| PathBuf::from(&change.file_path))
            .collect::<Vec<_>>(),
    ) {
        Ok(snapshot_state) => snapshot_state,
        Err(err) => return format!("Error: failed to snapshot workspace: {err}"),
    };
    let execution =
        match execute_code_change_set(root, &candidate.patch, &selection_apply_options(), None) {
            Ok(result) => result,
            Err(err) => return format!("Error: failed to execute change_set: {err}"),
        };
    if !execution.applied {
        record_candidate_outcome(
            &candidate,
            step_id,
            evaluate_apply_outcome(&execution, simulation),
            conversation,
        );
        let validation = DesignDeltaValidation {
            build_ok: execution.build_ok,
            rolled_back: execution.rolled_back,
            issues: vec![
                execution
                    .reason
                    .unwrap_or_else(|| "build validation failed".to_string()),
            ],
            ..DesignDeltaValidation::success()
        };
        conversation.rollback_current_transaction();
        return render_validation_result(&validation);
    }

    let mut post_validation = validate_post_execution(
        analysis_before,
        &candidate.target,
        &candidate.canonical_target,
        root,
    );
    if !post_validation.issues.is_empty() {
        if let Err(err) = rollback_apply(&rollback_snapshot) {
            return format!("Error: rollback failed after validation error: {err}");
        }
        post_validation.rolled_back = true;
        post_validation.build_ok = false;
        record_candidate_outcome(&candidate, step_id, ApplyOutcome::Failure, conversation);
        conversation.rollback_current_transaction();
        let _ = persist_ir_transition(
            &before,
            &conversation.ir_state,
            ActionKind::Rollback,
            "design_delta_reasoning_validation_failed",
            IRPersistenceArtifact::default(),
        );
        return render_validation_result(&post_validation);
    }

    conversation.note_target(candidate.canonical_target.clone());
    conversation.mark_transaction_applied(Some(candidate.snapshot.clone()));
    if let Some(tx) = conversation.active_transaction_mut() {
        tx.latest_build_ok = Some(execution.build_ok);
    }
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        ActionKind::Apply,
        "design_delta_reasoning",
        IRPersistenceArtifact {
            diff_ref: Some(candidate.snapshot.clone()),
            build_ok: Some(execution.build_ok),
            validation_ok: Some(true),
            ..IRPersistenceArtifact::default()
        },
    );
    record_candidate_outcome(
        &candidate,
        step_id,
        evaluate_apply_outcome(&execution, simulation),
        conversation,
    );
    format!(
        "DIFF\n{diff_text}\nRESULT\nRefactoring applied successfully.\n{} files changed, +{} -{} lines.",
        candidate.snapshot.files_changed,
        candidate.snapshot.lines_added,
        candidate.snapshot.lines_removed
    )
}

fn selection_apply_options() -> ExecutionCodingOptions {
    ExecutionCodingOptions {
        apply: true,
        check: true,
        no_build: false,
        backup: true,
        format: false,
        safe_mode: true,
        auto_commit: false,
        confirm_commit: false,
        prompt_commit: false,
        auto_push: false,
        confirm_push: false,
        auto_pr: false,
        confirm_pr: false,
        pr_base: "main".to_string(),
        patch_scope: PatchScope::WorkspaceWide,
        explicit_target: None,
    }
}

fn expand_candidate_patches(
    spec: &str,
    step_index: usize,
    memory_context: &MemoryContext,
    intent: Option<&CodingIntent>,
    delta: &design_delta::DesignDelta,
    mutation_plan: &design_delta::MutationPlan,
    analysis_before: &AnalysisReport,
    conversation: &ConversationState,
) -> Vec<CandidatePatch> {
    let exploration_input = ExplorationInput {
        intent,
        context: analysis_before,
        spec,
        delta,
        step_index,
        memory_context,
    };
    let candidate_targets = generate_exploration_candidates(&exploration_input);
    let candidate_memories = candidate_memories_for_step(memory_context, step_index);
    let mut candidates = Vec::new();
    for generated in candidate_targets.into_iter().take(TOTAL_EXPLORATION_TOP_K) {
        let target = generated.target.clone();
        if target_already_satisfied(analysis_before, &target) {
            continue;
        }
        let Some((change_set, canonical_target, diff_preview, snapshot, confidence, descriptor)) =
            materialize_generated_candidate(analysis_before, &generated)
        else {
            continue;
        };
        let diff_meaning = classify_diff_meaning(&diff_preview);
        let pre_validation = validate_pre_execution(
            spec,
            delta,
            mutation_plan,
            &change_set,
            &diff_meaning,
            &canonical_target,
            &snapshot,
            conversation,
        );
        let (memory_id, timestamp, symbolic_score, embedding_score, success_rate) =
            candidate_memory_scores(&descriptor, &candidate_memories);
        let intent_score = evaluate_intent_consistency(intent, &diff_preview);
        let origin = if memory_id.starts_with("candidate:") {
            generated.origin
        } else {
            CandidateOrigin::Memory
        };
        candidates.push(CandidatePatch {
            memory_id,
            origin,
            patch: change_set,
            symbolic_score: symbolic_score + confidence,
            embedding_score,
            intent_score,
            success_rate,
            timestamp,
            target,
            canonical_target,
            diff_preview,
            snapshot,
            pre_validation,
        });
    }
    candidates.sort_by(|lhs, rhs| {
        rhs.symbolic_score
            .partial_cmp(&lhs.symbolic_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                rhs.embedding_score
                    .partial_cmp(&lhs.embedding_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then(candidate_origin_rank(&lhs.origin).cmp(&candidate_origin_rank(&rhs.origin)))
            .then(lhs.timestamp.cmp(&rhs.timestamp))
            .then(lhs.memory_id.cmp(&rhs.memory_id))
    });
    candidates.truncate(TOTAL_EXPLORATION_TOP_K);
    candidates
}

fn simulate_candidate_patch(root: &Path, candidate: &CandidatePatch) -> SimulationResult {
    let sandbox_root = match create_validation_sandbox(root) {
        Ok(path) => path,
        Err(_) => {
            return SimulationResult {
                compile_ok: false,
                error_count: 1,
                warnings: 0,
                diff_size: candidate.patch.summary.total_changes.max(1),
                test_result: TestResult::default(),
                semantic_score: 0.0,
                intent_score: candidate.intent_score,
            };
        }
    };
    let result = execute_code_change_set(
        &sandbox_root,
        &candidate.patch,
        &selection_apply_options(),
        None,
    );
    let simulation = match result {
        Ok(result) => simulation_result_from_execution(candidate, &result, &sandbox_root),
        Err(_) => SimulationResult {
            compile_ok: false,
            error_count: 1,
            warnings: 0,
            diff_size: candidate.patch.summary.total_changes.max(1),
            test_result: TestResult::default(),
            semantic_score: 0.0,
            intent_score: candidate.intent_score,
        },
    };
    let _ = std::fs::remove_dir_all(&sandbox_root);
    simulation
}

fn run_selection_tests(root: &Path, candidate: &CandidatePatch) -> TestResult {
    if !root.join("Cargo.toml").exists() {
        return TestResult {
            passed: 0,
            failed: 0,
            skipped: 1,
        };
    }
    let command = match resolve_command("cargo") {
        Ok(command) => command,
        Err(_) => {
            return TestResult {
                passed: 0,
                failed: 1,
                skipped: 0,
            };
        }
    };
    let mut args = vec!["test".to_string()];
    if candidate
        .canonical_target
        .components()
        .any(|component| component.as_os_str() == "apps")
    {
        args.extend([
            "-p".to_string(),
            "design_cli".to_string(),
            "--lib".to_string(),
        ]);
    } else {
        args.extend(["--workspace".to_string(), "--lib".to_string()]);
    }
    args.push("--quiet".to_string());
    let output = std::process::Command::new(command)
        .args(args)
        .current_dir(root)
        .envs(fixed_env())
        .output();
    let Ok(output) = output else {
        return TestResult {
            passed: 0,
            failed: 1,
            skipped: 0,
        };
    };
    parse_test_result(
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

fn simulation_result_from_execution(
    candidate: &CandidatePatch,
    execution: &CodingExecutionResult,
    sandbox_root: &Path,
) -> SimulationResult {
    let error_count = execution
        .reason
        .as_deref()
        .map(count_error_lines)
        .unwrap_or(0)
        .max(
            execution
                .transactional_apply
                .as_ref()
                .map(|tx| tx.diagnostics.iter().map(|d| count_error_lines(d)).sum())
                .unwrap_or(0),
        );
    let warnings = execution
        .reason
        .as_deref()
        .map(count_warning_lines)
        .unwrap_or(0);
    let test_result = if execution.build_ok {
        run_selection_tests(sandbox_root, candidate)
    } else {
        TestResult::default()
    };
    SimulationResult {
        compile_ok: execution.build_ok,
        error_count,
        warnings,
        diff_size: candidate.patch.summary.total_changes.max(1),
        test_result,
        semantic_score: evaluate_semantic_score(&candidate.diff_preview),
        intent_score: candidate.intent_score,
    }
}

fn evaluate_apply_outcome(
    execution: &CodingExecutionResult,
    simulation: &SimulationResult,
) -> ApplyOutcome {
    if !(execution.applied && execution.build_ok && !execution.rolled_back) {
        return ApplyOutcome::Failure;
    }
    if test_score(&simulation.test_result) <= TEST_SUCCESS_THRESHOLD
        || simulation.test_result.failed > 0
    {
        return ApplyOutcome::Failure;
    }
    if execution
        .reason
        .as_deref()
        .map(count_warning_lines)
        .unwrap_or(0)
        > 0
        || execution
            .transactional_apply
            .as_ref()
            .map(|tx| {
                tx.diagnostics
                    .iter()
                    .map(|d| count_warning_lines(d))
                    .sum::<usize>()
            })
            .unwrap_or(0)
            > 0
    {
        ApplyOutcome::CompileWithWarnings
    } else {
        ApplyOutcome::CompileSuccess
    }
}

fn record_candidate_outcome(
    candidate: &CandidatePatch,
    step_id: Option<Uuid>,
    outcome: ApplyOutcome,
    conversation: &ConversationState,
) {
    let Some(step_id) = step_id else {
        return;
    };
    if candidate.memory_id.starts_with("candidate:") {
        return;
    }
    let memory_outcome = match outcome {
        ApplyOutcome::CompileSuccess => MemoryOutcome::CompileSuccess,
        ApplyOutcome::CompileWithWarnings => MemoryOutcome::CompileWithWarnings,
        ApplyOutcome::Failure => MemoryOutcome::Failure,
    };
    let _ = emit_memory_outcome(
        &conversation.ir_state,
        step_id,
        candidate.memory_id.clone(),
        memory_outcome,
    );
}

fn evaluate_candidate_patch(
    candidate: &CandidatePatch,
    simulation: &SimulationResult,
) -> EvaluationScore {
    let compile_score = if simulation.compile_ok { 1.0 } else { 0.0 };
    let test_score = test_score(&simulation.test_result);
    let error_penalty = (simulation.error_count as f32 + 1.0).ln();
    let diff_penalty = (simulation.diff_size as f32 + 1.0).ln();
    let total_score = SELECTION_W1_SYMBOLIC * candidate.symbolic_score
        + SELECTION_W2_EMBEDDING * candidate.embedding_score
        + SELECTION_W3_COMPILE * compile_score
        + SELECTION_W4_TEST * test_score
        + SELECTION_W5_SEMANTIC * simulation.semantic_score
        + SELECTION_W6_INTENT * simulation.intent_score
        + SELECTION_W7_SUCCESS_RATE * candidate.success_rate
        - SELECTION_W_ERROR_PENALTY * error_penalty
        - SELECTION_W_DIFF_PENALTY * diff_penalty;
    EvaluationScore {
        total_score,
        compile_score,
        test_score,
        semantic_score: simulation.semantic_score,
        intent_score: simulation.intent_score,
        error_penalty,
        diff_penalty,
    }
}

fn select_best_candidate_patch(
    mut evaluated: Vec<(CandidatePatch, SimulationResult, EvaluationScore)>,
) -> Option<(CandidatePatch, SimulationResult, EvaluationScore)> {
    evaluated.sort_by(
        |(lhs_candidate, _, lhs_score), (rhs_candidate, _, rhs_score)| {
            rhs_score
                .total_score
                .partial_cmp(&lhs_score.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    candidate_origin_rank(&lhs_candidate.origin)
                        .cmp(&candidate_origin_rank(&rhs_candidate.origin)),
                )
                .then(lhs_candidate.timestamp.cmp(&rhs_candidate.timestamp))
                .then(lhs_candidate.memory_id.cmp(&rhs_candidate.memory_id))
        },
    );
    evaluated.into_iter().next()
}

fn candidate_origin_rank(origin: &CandidateOrigin) -> u8 {
    match origin {
        CandidateOrigin::Memory => 0,
        CandidateOrigin::RuleBased => 1,
        CandidateOrigin::WorldModel => 2,
    }
}

impl CandidateGenerator for RuleBasedGenerator {
    fn generate(&self, input: &ExplorationInput<'_>) -> Vec<GeneratedCandidate> {
        expand_refactor_targets(
            &input.context.modules,
            &input.context.dependencies,
            input.spec,
            input.delta,
        )
        .into_iter()
        .map(|target| GeneratedCandidate::single(target, CandidateOrigin::RuleBased))
        .collect()
    }
}

impl CandidateGenerator for WorldModelGenerator {
    fn generate(&self, input: &ExplorationInput<'_>) -> Vec<GeneratedCandidate> {
        let mut candidates = Vec::new();
        if let Some(intent) = input.intent {
            match intent {
                CodingIntent::FixBug { target, .. } => {
                    let module = module_name_for_target(input.context, target);
                    candidates.push(GeneratedCandidate::single(
                        RefactorTarget::IntroduceService(module.clone()),
                        CandidateOrigin::WorldModel,
                    ));
                    candidates.push(GeneratedCandidate::single(
                        RefactorTarget::RenameBoundary(module),
                        CandidateOrigin::WorldModel,
                    ));
                }
                CodingIntent::Refactor { .. } => {
                    if let Some(violation) = input.context.violations.first() {
                        candidates.push(GeneratedCandidate::single(
                            RefactorTarget::LayerViolation(format!(
                                "{}->{}",
                                violation.from, violation.to
                            )),
                            CandidateOrigin::WorldModel,
                        ));
                    }
                    if input.context.modules.len() >= 2 {
                        let modules = input
                            .context
                            .modules
                            .iter()
                            .take(2)
                            .map(|module| module.name.clone())
                            .collect::<Vec<_>>();
                        candidates.push(GeneratedCandidate::single(
                            RefactorTarget::MergeModule(modules),
                            CandidateOrigin::WorldModel,
                        ));
                    }
                }
                CodingIntent::AddFeature { target, .. } => {
                    let module = module_name_for_target(input.context, target);
                    candidates.push(GeneratedCandidate::single(
                        RefactorTarget::IntroduceService(module.clone()),
                        CandidateOrigin::WorldModel,
                    ));
                    candidates.push(GeneratedCandidate::single(
                        RefactorTarget::FileMove(target.clone()),
                        CandidateOrigin::WorldModel,
                    ));
                }
            }
        }

        if candidates.is_empty() {
            if let Some(module) = input.context.modules.first() {
                candidates.push(GeneratedCandidate::single(
                    RefactorTarget::IntroduceService(module.name.clone()),
                    CandidateOrigin::WorldModel,
                ));
            }
            if let Some(edge) = input.context.dependencies.first() {
                candidates.push(GeneratedCandidate::single(
                    RefactorTarget::ExtractInterface {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                    },
                    CandidateOrigin::WorldModel,
                ));
            }
        }

        let memory_bias = candidate_memories_for_step(input.memory_context, input.step_index)
            .iter()
            .filter_map(|entry| world_model_target_from_memory(entry))
            .take(1)
            .collect::<Vec<_>>();
        candidates.extend(
            memory_bias
                .into_iter()
                .map(|target| GeneratedCandidate::single(target, CandidateOrigin::WorldModel)),
        );

        let mut seen = std::collections::BTreeSet::new();
        let mut ordered = candidates
            .into_iter()
            .filter(|candidate| seen.insert(refactor_target_descriptor(&candidate.target)))
            .collect::<Vec<_>>();
        ordered.truncate(WORLD_MODEL_TOP_K);
        ordered
    }
}

fn generate_exploration_candidates(input: &ExplorationInput<'_>) -> Vec<GeneratedCandidate> {
    let generators: [&dyn CandidateGenerator; 2] = [&RuleBasedGenerator, &WorldModelGenerator];
    let mut merged = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for generator in generators {
        for candidate in generator.generate(input) {
            let key = generated_candidate_descriptor(&candidate);
            if seen.insert(key) {
                merged.push(candidate);
            }
        }
    }
    if allows_multistep_exploration(input.spec) {
        let multistep = generate_multistep_candidates(input, &merged);
        for candidate in multistep {
            let key = generated_candidate_descriptor(&candidate);
            if seen.insert(key) {
                merged.push(candidate);
            }
        }
    }
    merged.truncate(TOTAL_EXPLORATION_TOP_K);
    merged
}

fn materialize_generated_candidate(
    analysis_before: &AnalysisReport,
    generated: &GeneratedCandidate,
) -> Option<(
    CodeChangeSet,
    PathBuf,
    crate::coding::UnifiedDiffPreview,
    SessionAppliedDiff,
    f32,
    String,
)> {
    let root = PathBuf::from(&analysis_before.root);
    let mut patches = Vec::new();
    let mut confidence = 0.0f32;
    for action in &generated.actions {
        let Ok(plan) = create_refactor_plan(analysis_before, action.target.clone()) else {
            return None;
        };
        patches.extend(plan.patches);
        confidence += plan.confidence;
    }
    if patches.is_empty() {
        return None;
    }
    let change_set = generate_code_change_set(&root, &patches).ok()?;
    if change_set.changes.is_empty() {
        return None;
    }
    let diff_preview = build_unified_diff_preview(&root, &change_set).ok()?;
    let snapshot = build_session_diff_snapshot(&diff_preview);
    let canonical_target = change_set
        .canonical_target
        .clone()
        .or_else(|| {
            change_set
                .changes
                .first()
                .map(|change| PathBuf::from(&change.file_path))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    let average_confidence = confidence / generated.actions.len().max(1) as f32;
    Some((
        change_set,
        canonical_target,
        diff_preview,
        snapshot,
        average_confidence,
        generated_candidate_descriptor(generated),
    ))
}

fn generate_multistep_candidates(
    input: &ExplorationInput<'_>,
    seeds: &[GeneratedCandidate],
) -> Vec<GeneratedCandidate> {
    let mut queue = std::collections::VecDeque::new();
    for seed in seeds {
        let heuristic_score = heuristic_score_for_actions(input, &seed.actions);
        queue.push_back(ExplorationState {
            applied_actions: seed.actions.clone(),
            depth: seed.actions.len(),
            origin: seed.origin.clone(),
            heuristic_score,
        });
    }

    let mut visited = std::collections::BTreeSet::new();
    let mut generated = Vec::new();
    let mut expanded_nodes = 0usize;

    for current_depth in 1..MULTISTEP_MAX_DEPTH {
        if queue.is_empty()
            || expanded_nodes >= MULTISTEP_MAX_TOTAL_NODES
            || generated.len() >= MULTISTEP_TOP_K
        {
            break;
        }
        let current_level = queue.into_iter().collect::<Vec<_>>();
        let mut next_level = Vec::new();

        for state in current_level {
            if state.depth != current_depth || state.depth >= MULTISTEP_MAX_DEPTH {
                continue;
            }
            let Some(last_action) = state.applied_actions.last() else {
                continue;
            };

            let next_actions = enumerate_follow_up_actions(input, last_action, &state)
                .into_iter()
                .filter(|next| {
                    !state
                        .applied_actions
                        .iter()
                        .any(|existing| existing.target == next.target)
                })
                .filter(|next| !is_noop_action(input, next))
                .filter(|next| !increases_invalidity(next, &state))
                .filter(|next| early_validate_action(input, next))
                .take(MULTISTEP_MAX_BRANCH)
                .collect::<Vec<_>>();

            for next in next_actions {
                if expanded_nodes >= MULTISTEP_MAX_TOTAL_NODES || generated.len() >= MULTISTEP_TOP_K
                {
                    break;
                }
                let mut actions = state.applied_actions.clone();
                actions.push(next.clone());
                let descriptor = actions
                    .iter()
                    .map(|action| refactor_target_descriptor(&action.target))
                    .collect::<Vec<_>>()
                    .join("=>");
                if !visited.insert(descriptor) {
                    continue;
                }
                let origin =
                    merge_candidate_origins(&state.origin, &candidate_origin_for_action(&next));
                let heuristic_score = heuristic_score_for_actions(input, &actions);
                let candidate = GeneratedCandidate {
                    target: next.target.clone(),
                    origin: origin.clone(),
                    actions: actions.clone(),
                };
                generated.push(candidate);
                next_level.push(ExplorationState {
                    applied_actions: actions,
                    depth: state.depth + 1,
                    origin,
                    heuristic_score,
                });
                expanded_nodes += 1;
            }
        }

        next_level.sort_by(|lhs, rhs| {
            rhs.heuristic_score
                .partial_cmp(&lhs.heuristic_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    action_sequence_descriptor(&lhs.applied_actions)
                        .cmp(&action_sequence_descriptor(&rhs.applied_actions)),
                )
        });
        next_level.truncate(MULTISTEP_BEAM_WIDTH);
        queue = next_level.into();
    }

    generated.sort_by(|lhs, rhs| {
        heuristic_score_for_actions(input, &rhs.actions)
            .partial_cmp(&heuristic_score_for_actions(input, &lhs.actions))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(generated_candidate_descriptor(lhs).cmp(&generated_candidate_descriptor(rhs)))
    });
    generated.truncate(MULTISTEP_TOP_K);
    generated
}

fn enumerate_follow_up_actions(
    input: &ExplorationInput<'_>,
    last_action: &ExplorationAction,
    state: &ExplorationState,
) -> Vec<ExplorationAction> {
    let mut targets = Vec::new();
    match &last_action.target {
        RefactorTarget::ExtractInterface { from, to } => {
            targets.push(RefactorTarget::RemoveDependency {
                from: from.clone(),
                to: to.clone(),
            });
            targets.push(RefactorTarget::IntroduceService(to.clone()));
        }
        RefactorTarget::RemoveDependency { from, to } => {
            targets.push(RefactorTarget::ExtractInterface {
                from: from.clone(),
                to: to.clone(),
            });
            targets.push(RefactorTarget::IntroduceService(to.clone()));
        }
        RefactorTarget::ModuleSplit(module) => {
            targets.push(RefactorTarget::IntroduceService(module.clone()));
            targets.push(RefactorTarget::RenameBoundary(module.clone()));
        }
        RefactorTarget::MergeModule(modules) => {
            if let Some(module) = modules.first() {
                targets.push(RefactorTarget::RenameBoundary(module.clone()));
                targets.push(RefactorTarget::IntroduceService(module.clone()));
            }
        }
        RefactorTarget::LayerViolation(detail) => {
            if let Some((from, to)) = detail.split_once("->") {
                targets.push(RefactorTarget::ExtractInterface {
                    from: from.to_string(),
                    to: to.to_string(),
                });
                targets.push(RefactorTarget::RemoveDependency {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
        }
        RefactorTarget::RenameBoundary(module) => {
            targets.push(RefactorTarget::IntroduceService(module.clone()));
            targets.push(RefactorTarget::ModuleSplit(module.clone()));
        }
        RefactorTarget::IntroduceService(module) => {
            targets.push(RefactorTarget::RenameBoundary(module.clone()));
            targets.push(RefactorTarget::ModuleSplit(module.clone()));
        }
        RefactorTarget::FileMove(path) => {
            let module = module_name_for_target(input.context, path);
            targets.push(RefactorTarget::RenameBoundary(module.clone()));
            targets.push(RefactorTarget::IntroduceService(module));
        }
        RefactorTarget::Cycle => {
            targets.extend(expand_refactor_targets(
                &input.context.modules,
                &input.context.dependencies,
                input.spec,
                input.delta,
            ));
        }
    }

    if state.depth == 1 {
        if let Some(intent) = input.intent {
            match intent {
                CodingIntent::FixBug { target, .. } => {
                    let module = module_name_for_target(input.context, target);
                    targets.push(RefactorTarget::IntroduceService(module.clone()));
                    targets.push(RefactorTarget::RenameBoundary(module));
                }
                CodingIntent::Refactor { .. } => {
                    targets.extend(expand_refactor_targets(
                        &input.context.modules,
                        &input.context.dependencies,
                        input.spec,
                        input.delta,
                    ));
                }
                CodingIntent::AddFeature { target, .. } => {
                    targets.push(RefactorTarget::FileMove(target.clone()));
                }
            }
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    targets
        .into_iter()
        .filter(|target| seen.insert(refactor_target_descriptor(target)))
        .map(|target| exploration_action_for_target(&target))
        .collect()
}

fn heuristic_score_for_actions(input: &ExplorationInput<'_>, actions: &[ExplorationAction]) -> f32 {
    let local_syntax_gain = actions
        .iter()
        .map(local_syntax_gain_for_action)
        .sum::<f32>();
    let intent_alignment_estimate = if actions.is_empty() {
        0.0
    } else {
        actions
            .iter()
            .map(|action| intent_alignment_estimate(input.intent, action))
            .sum::<f32>()
            / actions.len() as f32
    };
    let complexity_penalty = (actions.len() as f32 + 1.0).ln();
    local_syntax_gain + intent_alignment_estimate - 0.3 * complexity_penalty
}

fn local_syntax_gain_for_action(action: &ExplorationAction) -> f32 {
    match action.target {
        RefactorTarget::ExtractInterface { .. } => 1.0,
        RefactorTarget::RemoveDependency { .. } => 0.9,
        RefactorTarget::IntroduceService(_) => 0.8,
        RefactorTarget::RenameBoundary(_) => 0.6,
        RefactorTarget::ModuleSplit(_) => 0.5,
        RefactorTarget::MergeModule(_) => 0.4,
        RefactorTarget::LayerViolation(_) => 0.7,
        RefactorTarget::FileMove(_) => 0.3,
        RefactorTarget::Cycle => 0.2,
    }
}

fn intent_alignment_estimate(intent: Option<&CodingIntent>, action: &ExplorationAction) -> f32 {
    let Some(intent) = intent else {
        return 0.5;
    };
    match intent {
        CodingIntent::FixBug { .. } => match action.target {
            RefactorTarget::ExtractInterface { .. }
            | RefactorTarget::RemoveDependency { .. }
            | RefactorTarget::IntroduceService(_) => 0.9,
            RefactorTarget::RenameBoundary(_) => 0.6,
            RefactorTarget::ModuleSplit(_) => 0.3,
            RefactorTarget::MergeModule(_) | RefactorTarget::FileMove(_) => 0.2,
            RefactorTarget::LayerViolation(_) => 0.7,
            RefactorTarget::Cycle => 0.1,
        },
        CodingIntent::Refactor { .. } => match action.target {
            RefactorTarget::RenameBoundary(_)
            | RefactorTarget::ModuleSplit(_)
            | RefactorTarget::MergeModule(_)
            | RefactorTarget::IntroduceService(_) => 0.9,
            RefactorTarget::LayerViolation(_) => 0.8,
            RefactorTarget::ExtractInterface { .. } | RefactorTarget::RemoveDependency { .. } => {
                0.7
            }
            RefactorTarget::FileMove(_) => 0.5,
            RefactorTarget::Cycle => 0.4,
        },
        CodingIntent::AddFeature { .. } => match action.target {
            RefactorTarget::IntroduceService(_) | RefactorTarget::FileMove(_) => 0.9,
            RefactorTarget::RenameBoundary(_) => 0.5,
            RefactorTarget::ModuleSplit(_) => 0.4,
            RefactorTarget::ExtractInterface { .. } | RefactorTarget::RemoveDependency { .. } => {
                0.3
            }
            RefactorTarget::MergeModule(_) => 0.2,
            RefactorTarget::LayerViolation(_) => 0.4,
            RefactorTarget::Cycle => 0.1,
        },
    }
}

fn is_noop_action(input: &ExplorationInput<'_>, action: &ExplorationAction) -> bool {
    target_already_satisfied(input.context, &action.target)
}

fn increases_invalidity(action: &ExplorationAction, state: &ExplorationState) -> bool {
    matches!(action.target, RefactorTarget::MergeModule(_))
        && state
            .applied_actions
            .iter()
            .any(|existing| matches!(existing.target, RefactorTarget::ModuleSplit(_)))
}

fn early_validate_action(input: &ExplorationInput<'_>, action: &ExplorationAction) -> bool {
    let focus = action.target_node.to_lowercase();
    let syntax_ok = !focus.is_empty() && !focus.contains(" ");
    let structural_ok = match &action.target {
        RefactorTarget::ExtractInterface { from, to }
        | RefactorTarget::RemoveDependency { from, to } => input
            .context
            .dependencies
            .iter()
            .any(|dependency| dependency.from == *from && dependency.to == *to),
        RefactorTarget::MergeModule(modules) => modules.len() >= 2,
        RefactorTarget::FileMove(path) => !path.as_os_str().is_empty(),
        _ => true,
    };
    syntax_ok && structural_ok
}

fn action_sequence_descriptor(actions: &[ExplorationAction]) -> String {
    actions
        .iter()
        .map(|action| refactor_target_descriptor(&action.target))
        .collect::<Vec<_>>()
        .join("=>")
}

fn exploration_action_for_target(target: &RefactorTarget) -> ExplorationAction {
    ExplorationAction {
        kind: ActionKind::Refactor,
        target_node: target_focus_node(target),
        target: target.clone(),
    }
}

fn target_focus_node(target: &RefactorTarget) -> String {
    match target {
        RefactorTarget::Cycle => "workspace".to_string(),
        RefactorTarget::ExtractInterface { from, to }
        | RefactorTarget::RemoveDependency { from, to } => format!("{from}->{to}"),
        RefactorTarget::ModuleSplit(module)
        | RefactorTarget::LayerViolation(module)
        | RefactorTarget::RenameBoundary(module)
        | RefactorTarget::IntroduceService(module) => module.clone(),
        RefactorTarget::MergeModule(modules) => modules.join(","),
        RefactorTarget::FileMove(path) => path.display().to_string(),
    }
}

fn candidate_origin_for_action(action: &ExplorationAction) -> CandidateOrigin {
    match action.target {
        RefactorTarget::IntroduceService(_)
        | RefactorTarget::RenameBoundary(_)
        | RefactorTarget::MergeModule(_)
        | RefactorTarget::LayerViolation(_)
        | RefactorTarget::FileMove(_) => CandidateOrigin::WorldModel,
        RefactorTarget::Cycle
        | RefactorTarget::ExtractInterface { .. }
        | RefactorTarget::RemoveDependency { .. }
        | RefactorTarget::ModuleSplit(_) => CandidateOrigin::RuleBased,
    }
}

fn merge_candidate_origins(lhs: &CandidateOrigin, rhs: &CandidateOrigin) -> CandidateOrigin {
    if lhs == &CandidateOrigin::WorldModel || rhs == &CandidateOrigin::WorldModel {
        CandidateOrigin::WorldModel
    } else {
        CandidateOrigin::RuleBased
    }
}

fn generated_candidate_descriptor(candidate: &GeneratedCandidate) -> String {
    candidate
        .actions
        .iter()
        .map(|action| refactor_target_descriptor(&action.target))
        .collect::<Vec<_>>()
        .join("=>")
}

fn allows_multistep_exploration(spec: &str) -> bool {
    let lower = spec.to_lowercase();
    if lower.contains("extractinterface(")
        || lower.contains("removedependency(")
        || lower.contains("splitmodule(")
        || lower.contains("mergemodule(")
        || lower.contains("renameboundary(")
        || lower.contains("introduceservice(")
        || lower.contains("filemove(")
    {
        return false;
    }
    true
}

fn module_name_for_target(report: &AnalysisReport, target: &Path) -> String {
    let target_str = target.display().to_string();
    report
        .modules
        .iter()
        .find(|module| {
            target_str.contains(&module.source_path) || module.source_path.contains(&target_str)
        })
        .map(|module| module.name.clone())
        .or_else(|| report.modules.first().map(|module| module.name.clone()))
        .unwrap_or_else(|| "core".to_string())
}

fn world_model_target_from_memory(entry: &crate::ir::MemoryEntry) -> Option<RefactorTarget> {
    entry.metadata.tags.iter().find_map(|tag| {
        if tag.contains("design_delta_reasoning") || tag.contains("refactor") {
            Some(RefactorTarget::IntroduceService("core".to_string()))
        } else {
            None
        }
    })
}

fn expand_refactor_targets(
    modules: &[AnalysisModule],
    dependencies: &[AnalysisDependency],
    spec: &str,
    delta: &design_delta::DesignDelta,
) -> Vec<RefactorTarget> {
    let mut targets = Vec::new();
    let primary = select_design_delta_target(modules, dependencies, spec, delta);
    targets.push(primary);
    let impacted_module = select_impacted_module(modules, delta)
        .or_else(|| modules.first().map(|module| module.name.clone()))
        .unwrap_or_else(|| "core".to_string());
    let impacted_edge = select_impacted_dependency(dependencies, delta).or_else(|| {
        dependencies
            .first()
            .map(|dependency| (dependency.from.clone(), dependency.to.clone()))
    });
    if let Some((from, to)) = impacted_edge {
        targets.push(RefactorTarget::ExtractInterface {
            from: from.clone(),
            to: to.clone(),
        });
        targets.push(RefactorTarget::RemoveDependency { from, to });
    }
    targets.push(RefactorTarget::ModuleSplit(impacted_module));

    let mut seen = std::collections::BTreeSet::new();
    targets
        .into_iter()
        .filter(|target| seen.insert(refactor_target_descriptor(target)))
        .collect()
}

fn candidate_memories_for_step<'a>(
    memory_context: &'a MemoryContext,
    step_index: usize,
) -> Vec<&'a crate::ir::MemoryEntry> {
    memory_context
        .entries
        .iter()
        .filter(|entry| entry.metadata.step_index < step_index)
        .collect()
}

fn candidate_memory_scores(
    descriptor: &str,
    memories: &[&crate::ir::MemoryEntry],
) -> (String, u64, f32, f32, f32) {
    let descriptor_embedding = deterministic_selection_embedding(descriptor);
    memories
        .iter()
        .map(|entry| {
            let symbolic_score = candidate_symbolic_score(descriptor, entry);
            let embedding_score = candidate_embedding_score(&descriptor_embedding, entry);
            let success_rate = candidate_success_rate(entry);
            (
                entry.memory_id.clone(),
                entry.metadata.timestamp,
                symbolic_score,
                embedding_score,
                success_rate,
            )
        })
        .max_by(|lhs, rhs| {
            let lhs_total = lhs.2 + lhs.3 + lhs.4;
            let rhs_total = rhs.2 + rhs.3 + rhs.4;
            lhs_total
                .partial_cmp(&rhs_total)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| rhs.1.cmp(&lhs.1))
                .then_with(|| rhs.0.cmp(&lhs.0))
        })
        .unwrap_or_else(|| (format!("candidate:{descriptor}"), 0, 0.0, 0.0, 0.0))
}

fn candidate_symbolic_score(descriptor: &str, entry: &crate::ir::MemoryEntry) -> f32 {
    let descriptor_tokens = selection_tokens(descriptor);
    descriptor_tokens
        .iter()
        .filter(|token| {
            entry.metadata.tags.iter().any(|tag| {
                let lower = tag.to_lowercase();
                lower.contains(token.as_str())
            }) || entry
                .content
                .to_string()
                .to_lowercase()
                .contains(token.as_str())
        })
        .count() as f32
}

fn candidate_embedding_score(descriptor_embedding: &[f32], entry: &crate::ir::MemoryEntry) -> f32 {
    let memory_embedding = entry
        .embedding
        .clone()
        .unwrap_or_else(|| deterministic_selection_embedding(&entry.content.to_string()));
    normalized_cosine_similarity(descriptor_embedding, &memory_embedding)
}

fn candidate_success_rate(entry: &crate::ir::MemoryEntry) -> f32 {
    let success = entry.success_count as f32 / 2.0;
    let failure = entry.failure_count as f32 / 2.0;
    (success + 1.0) / (success + failure + 2.0)
}

fn test_score(result: &TestResult) -> f32 {
    if result.passed == 0 && result.failed == 0 {
        return 1.0;
    }
    result.passed as f32 / (result.passed + result.failed + 1) as f32
}

fn parse_test_result(stdout: &str, stderr: &str) -> TestResult {
    let joined = format!("{stdout}\n{stderr}");
    for line in joined.lines() {
        if let Some(rest) = line.trim().strip_prefix("test result: ") {
            let passed = extract_count(rest, "passed").unwrap_or(0);
            let failed = extract_count(rest, "failed").unwrap_or(0);
            let skipped = extract_count(rest, "ignored")
                .or_else(|| extract_count(rest, "skipped"))
                .unwrap_or(0);
            return TestResult {
                passed,
                failed,
                skipped,
            };
        }
    }
    TestResult {
        passed: 0,
        failed: 1,
        skipped: 0,
    }
}

fn extract_count(line: &str, label: &str) -> Option<usize> {
    line.split(',')
        .map(str::trim)
        .find_map(|chunk| chunk.strip_suffix(label).map(str::trim))
        .and_then(|value| value.parse::<usize>().ok())
}

fn evaluate_semantic_score(diff_preview: &crate::coding::UnifiedDiffPreview) -> f32 {
    let mut score = 1.0f32;
    let mut public_api_delta = 0usize;
    let mut unsafe_added = 0usize;
    let mut unused_added = 0usize;
    for file in &diff_preview.files {
        for hunk in &file.hunks {
            for line in &hunk.lines {
                if let DiffLine::Added(value) = line {
                    let trimmed = value.trim();
                    if trimmed.starts_with("pub ") {
                        public_api_delta += 1;
                    }
                    if trimmed.contains("unsafe") {
                        unsafe_added += 1;
                    }
                    if trimmed.contains("unused") || trimmed.contains("_unused") {
                        unused_added += 1;
                    }
                }
            }
        }
    }
    if public_api_delta > 0 {
        score -= 0.2;
    }
    if unsafe_added > 0 {
        score -= 0.3;
    }
    if unused_added > 0 {
        score -= 0.1;
    }
    if diff_preview.summary.file_count > 3 {
        score -= 0.1;
    }
    score.clamp(0.0, 1.0)
}

fn evaluate_intent_consistency(
    intent: Option<&CodingIntent>,
    diff_preview: &crate::coding::UnifiedDiffPreview,
) -> f32 {
    let Some(intent) = intent else {
        return 0.5;
    };
    let added = diff_preview.summary.added_lines;
    let removed = diff_preview.summary.removed_lines;
    let files = diff_preview.summary.file_count;
    let public_api_delta = diff_preview
        .files
        .iter()
        .flat_map(|file| file.hunks.iter())
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| {
            matches!(line, DiffLine::Added(value) if value.trim().starts_with("pub "))
                || matches!(line, DiffLine::Removed(value) if value.trim().starts_with("pub "))
        })
        .count();
    match intent {
        CodingIntent::FixBug { .. } => {
            if files <= 2 && added + removed <= 40 {
                1.0
            } else {
                0.4
            }
        }
        CodingIntent::Refactor { .. } => {
            if public_api_delta == 0 {
                1.0
            } else {
                0.3
            }
        }
        CodingIntent::AddFeature { .. } => {
            if added > removed && added >= 3 {
                1.0
            } else {
                0.4
            }
        }
    }
}

fn refactor_target_descriptor(target: &RefactorTarget) -> String {
    match target {
        RefactorTarget::Cycle => "cycle".to_string(),
        RefactorTarget::ExtractInterface { from, to } => format!("extract-interface:{from}:{to}"),
        RefactorTarget::RemoveDependency { from, to } => format!("remove-dependency:{from}:{to}"),
        RefactorTarget::ModuleSplit(module) => format!("module-split:{module}"),
        RefactorTarget::MergeModule(modules) => format!("merge-module:{}", modules.join(",")),
        RefactorTarget::LayerViolation(detail) => format!("layer-violation:{detail}"),
        RefactorTarget::RenameBoundary(module) => format!("rename-boundary:{module}"),
        RefactorTarget::IntroduceService(module) => format!("introduce-service:{module}"),
        RefactorTarget::FileMove(path) => format!("file-move:{}", path.display()),
    }
}

fn count_error_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("error[") || lower.contains("error:")
        })
        .count()
        .max(usize::from(text.to_lowercase().contains("failed")))
}

fn count_warning_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| line.to_lowercase().contains("warning"))
        .count()
}

fn selection_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn deterministic_selection_embedding(text: &str) -> Vec<f32> {
    const DIMENSION: usize = 8;
    let mut embedding = vec![0.0; DIMENSION];
    for token in selection_tokens(text) {
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest as _;
        hasher.update(token.as_bytes());
        let digest = hasher.finalize();
        let bucket = (digest[0] as usize) % DIMENSION;
        embedding[bucket] += 1.0 + (digest[1] as f32 / 255.0);
    }
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > 0.0 {
        for value in &mut embedding {
            *value /= norm;
        }
    }
    embedding
}

fn normalized_cosine_similarity(lhs: &[f32], rhs: &[f32]) -> f32 {
    let len = lhs.len().min(rhs.len());
    if len == 0 {
        return 0.5;
    }
    let dot = lhs
        .iter()
        .zip(rhs.iter())
        .take(len)
        .map(|(l, r)| l * r)
        .sum::<f32>();
    let lhs_norm = lhs
        .iter()
        .take(len)
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    let rhs_norm = rhs
        .iter()
        .take(len)
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if lhs_norm == 0.0 || rhs_norm == 0.0 {
        0.5
    } else {
        ((dot / (lhs_norm * rhs_norm)) + 1.0) / 2.0
    }
}

fn select_design_delta_target(
    modules: &[AnalysisModule],
    dependencies: &[AnalysisDependency],
    spec: &str,
    delta: &design_delta::DesignDelta,
) -> RefactorTarget {
    let lower = spec.to_lowercase();
    let module_name = select_impacted_module(modules, delta)
        .or_else(|| modules.first().map(|module| module.name.clone()))
        .unwrap_or_else(|| "core".to_string());
    let edge = select_impacted_dependency(dependencies, delta)
        .or_else(|| {
            dependencies
                .first()
                .map(|dependency| (dependency.from.clone(), dependency.to.clone()))
        })
        .unwrap_or_else(|| (module_name.clone(), "ports".to_string()));

    if lower.contains("splitmodule")
        || lower.contains("module split")
        || lower.contains("split module")
        || lower.contains("分割")
    {
        return RefactorTarget::ModuleSplit(module_name);
    }
    if lower.contains("removedependency")
        || lower.contains("remove dependency")
        || lower.contains("依存")
    {
        return RefactorTarget::RemoveDependency {
            from: edge.0,
            to: edge.1,
        };
    }
    if lower.contains("extractinterface")
        || lower.contains("extract interface")
        || lower.contains("trait")
        || lower.contains("interface")
        || lower.contains("抽象")
    {
        return RefactorTarget::ExtractInterface {
            from: edge.0,
            to: edge.1,
        };
    }
    if !delta.introduced_interfaces.is_empty() {
        return RefactorTarget::ExtractInterface {
            from: edge.0,
            to: edge.1,
        };
    }
    if !delta.dependency_moves.is_empty() {
        return RefactorTarget::RemoveDependency {
            from: edge.0,
            to: edge.1,
        };
    }
    RefactorTarget::ModuleSplit(module_name)
}

fn select_impacted_module(
    modules: &[AnalysisModule],
    delta: &design_delta::DesignDelta,
) -> Option<String> {
    modules
        .iter()
        .find(|module| {
            delta.impacted_crates.iter().any(|crate_name| {
                module.name.eq_ignore_ascii_case(crate_name)
                    || module
                        .source_path
                        .to_lowercase()
                        .contains(&crate_name.to_lowercase())
            })
        })
        .map(|module| module.name.clone())
}

fn select_impacted_dependency(
    dependencies: &[AnalysisDependency],
    delta: &design_delta::DesignDelta,
) -> Option<(String, String)> {
    dependencies
        .iter()
        .find(|dependency| {
            delta.impacted_crates.iter().any(|crate_name| {
                dependency.from.eq_ignore_ascii_case(crate_name)
                    || dependency.to.eq_ignore_ascii_case(crate_name)
                    || dependency
                        .from
                        .to_lowercase()
                        .contains(&crate_name.to_lowercase())
                    || dependency
                        .to
                        .to_lowercase()
                        .contains(&crate_name.to_lowercase())
            })
        })
        .map(|dependency| (dependency.from.clone(), dependency.to.clone()))
}

fn build_session_diff_snapshot(
    diff_preview: &crate::coding::UnifiedDiffPreview,
) -> SessionAppliedDiff {
    SessionAppliedDiff {
        summary: format!(
            "{} files changed, +{} -{} lines",
            diff_preview.summary.file_count,
            diff_preview.summary.added_lines,
            diff_preview.summary.removed_lines
        ),
        files: diff_preview
            .files
            .iter()
            .map(|diff| SessionAppliedFileDiff {
                file_path: diff.file.display().to_string(),
                unified_diff_excerpt: render_code_diff_lines(diff).join("\n"),
            })
            .collect(),
        files_changed: diff_preview.summary.file_count,
        lines_added: diff_preview.summary.added_lines,
        lines_removed: diff_preview.summary.removed_lines,
    }
}

fn render_design_delta_diff(diff_preview: &crate::coding::UnifiedDiffPreview) -> String {
    let mut lines = Vec::new();
    for diff in &diff_preview.files {
        lines.extend(render_code_diff_lines(diff));
    }
    if lines.is_empty() {
        "(empty diff)".to_string()
    } else {
        lines.join("\n")
    }
}

fn validate_pre_execution(
    spec: &str,
    delta: &design_delta::DesignDelta,
    mutation_plan: &design_delta::MutationPlan,
    change_set: &crate::coding::CodeChangeSet,
    diff_meaning: &DiffMeaning,
    canonical_target: &PathBuf,
    snapshot: &SessionAppliedDiff,
    conversation: &ConversationState,
) -> DesignDeltaValidation {
    let mut validation = DesignDeltaValidation::success();

    if diff_meaning.changed_lines < DESIGN_DELTA_MIN_CHANGED_LINES
        || diff_meaning.meaningful_lines == 0
        || diff_meaning.comment_only
        || diff_meaning.import_only
    {
        validation.meaningful = false;
        validation.issues.push("trivial diff rejected".to_string());
    }

    if change_set.changes.len() > DESIGN_DELTA_MAX_FILES {
        validation.within_scope = false;
        validation.issues.push(format!(
            "scope limit exceeded: {} files",
            change_set.changes.len()
        ));
    }

    let heuristic_signal = delta.impacted_crates.len()
        + delta.introduced_interfaces.len()
        + delta.dependency_moves.len()
        + delta.api_changes.len()
        + mutation_plan.target_files.len();
    let heuristic_request = is_heuristic_design_delta_request(spec);
    if heuristic_request && heuristic_signal < DESIGN_DELTA_HEURISTIC_SIGNAL_THRESHOLD {
        validation.heuristic_allowed = false;
        validation
            .issues
            .push("heuristic signal too weak for apply".to_string());
    }

    if same_as_active_transaction(canonical_target, snapshot, conversation) {
        validation.already_applied = true;
        validation
            .issues
            .push("same design delta was already applied".to_string());
    }

    validation
}

fn validate_post_execution(
    analysis_before: &AnalysisReport,
    target: &RefactorTarget,
    canonical_target: &PathBuf,
    root: &Path,
) -> DesignDeltaValidation {
    let mut validation = DesignDeltaValidation::success();
    let analysis_after = match analyze_path(root) {
        Ok(report) => report,
        Err(err) => {
            validation.ir_consistent = false;
            validation.build_ok = false;
            validation
                .issues
                .push(format!("post-apply analysis failed: {err}"));
            return validation;
        }
    };

    if analysis_after.cycles.cycles.len() > analysis_before.cycles.cycles.len() {
        validation.ir_consistent = false;
        validation
            .issues
            .push("cycle count regressed after apply".to_string());
    }

    if !dependency_improved_for_target(analysis_before, &analysis_after, target) {
        validation.dependency_improved = false;
        validation
            .issues
            .push("dependency graph did not improve for selected issue".to_string());
    }

    let canonical_path = if canonical_target.is_absolute() {
        canonical_target.clone()
    } else {
        root.join(canonical_target)
    };
    if !canonical_path.exists() {
        validation.ir_consistent = false;
        validation
            .issues
            .push("IR and code target diverged after apply".to_string());
    }

    validation
}

fn classify_diff_meaning(diff_preview: &crate::coding::UnifiedDiffPreview) -> DiffMeaning {
    let mut changed_lines = 0usize;
    let mut meaningful_lines = 0usize;
    let mut all_nonempty_are_comments = true;
    let mut all_nonempty_are_imports = true;

    for diff in &diff_preview.files {
        for line in render_code_diff_lines(diff) {
            if !(line.starts_with('+') || line.starts_with('-'))
                || line.starts_with("+++")
                || line.starts_with("---")
            {
                continue;
            }
            let body = line[1..].trim();
            if body.is_empty() {
                continue;
            }
            changed_lines += 1;
            let comment_like = is_comment_like_line(body);
            let import_like = is_import_like_line(body);
            if !comment_like && !import_like {
                meaningful_lines += 1;
            }
            if !comment_like {
                all_nonempty_are_comments = false;
            }
            if !import_like {
                all_nonempty_are_imports = false;
            }
        }
    }

    DiffMeaning {
        changed_lines,
        meaningful_lines,
        comment_only: changed_lines > 0 && all_nonempty_are_comments,
        import_only: changed_lines > 0 && all_nonempty_are_imports,
    }
}

fn is_comment_like_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

fn is_import_like_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("use ") || trimmed.starts_with("pub use ") || trimmed.starts_with("import ")
}

fn is_heuristic_design_delta_request(spec: &str) -> bool {
    let lower = spec.to_lowercase();
    ![
        "extractinterface",
        "extract interface",
        "removedependency",
        "remove dependency",
        "splitmodule",
        "split module",
        "trait",
        "interface",
        "依存",
        "分割",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn same_as_active_transaction(
    canonical_target: &PathBuf,
    snapshot: &SessionAppliedDiff,
    conversation: &ConversationState,
) -> bool {
    let Some(active) = conversation.active_transaction() else {
        return false;
    };
    active.canonical_target == *canonical_target
        && active
            .latest_diff_ref
            .as_ref()
            .map(|diff| diff == snapshot)
            .unwrap_or(false)
}

fn dependency_improved_for_target(
    before: &AnalysisReport,
    after: &AnalysisReport,
    target: &RefactorTarget,
) -> bool {
    match target {
        RefactorTarget::RemoveDependency { from, to } => {
            dependency_count(after, from, to) < dependency_count(before, from, to)
        }
        RefactorTarget::ExtractInterface { from, to } => {
            dependency_count(after, from, to) < dependency_count(before, from, to)
                || after.modules.iter().any(|module| {
                    module.name.contains("interface") || module.source_path.contains("interface")
                })
        }
        RefactorTarget::ModuleSplit(module) => {
            after.modules.len() > before.modules.len()
                || after.modules.iter().any(|candidate| {
                    candidate.name == format!("{module}_core")
                        || candidate.name == format!("{module}_api")
                })
        }
        _ => true,
    }
}

fn dependency_count(report: &AnalysisReport, from: &str, to: &str) -> usize {
    report
        .dependencies
        .iter()
        .filter(|dependency| dependency.from == from && dependency.to == to)
        .count()
}

fn target_already_satisfied(analysis: &AnalysisReport, target: &RefactorTarget) -> bool {
    match target {
        RefactorTarget::RemoveDependency { from, to } => dependency_count(analysis, from, to) == 0,
        RefactorTarget::ExtractInterface { from, to } => {
            dependency_count(analysis, from, to) == 0
                && analysis.modules.iter().any(|module| {
                    module.name.contains("interface") || module.source_path.contains("interface")
                })
        }
        RefactorTarget::ModuleSplit(module) => analysis.modules.iter().any(|candidate| {
            candidate.name == format!("{module}_core") || candidate.name == format!("{module}_api")
        }),
        _ => false,
    }
}

fn render_validation_result(validation: &DesignDeltaValidation) -> String {
    if validation.issues.is_empty() {
        return "No changes detected.".to_string();
    }
    if validation.rolled_back || !validation.build_ok {
        return format!(
            "RESULT\nRolled back due to validation failure.\n{}",
            validation.issues.join(" | ")
        );
    }
    "No changes detected.".to_string()
}

fn execute_design_tradeoff_explanation_step(
    _prompt: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    let search_result = conversation
        .last_mutation_search_result
        .clone()
        .or_else(|| session.last_mutation_search_result.clone());
    let Some(search_result) = search_result else {
        return "No tradeoff explanation available. Run alternative mutation search first."
            .to_string();
    };

    let Some(explanation) = explain::explain_tradeoff(&search_result) else {
        return "No tradeoff explanation available. Selected and rejected mutations were not retained.".to_string();
    };
    session.last_tradeoff_explanation = Some(explanation.clone());
    conversation.last_tradeoff_explanation = Some(explanation.clone());
    explain::render_pr_ready_block(&explanation)
}

fn ensure_viewer_session_is_fresh(
    step: &PlannedStep,
    conversation: &ConversationState,
) -> Result<(), String> {
    let path = match step {
        PlannedStep::StructureDiff(path, _)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path) => path,
        _ => return Ok(()),
    };

    let Some(expected) = conversation.last_viewer_session.as_deref() else {
        return Ok(());
    };

    let raw = run_design_command(
        "structure",
        &[
            "session".to_string(),
            path.display().to_string(),
            "--json".to_string(),
        ],
    )?;
    let json: Value = serde_json::from_str(&raw).map_err(|err| err.to_string())?;
    let current = json
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if current == expected {
        Ok(())
    } else {
        Err(format!(
            "stale viewer session: expected {expected}, current {current}"
        ))
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{
        DesignDelta, MutationCandidate, MutationPlan, MutationSearchResult, MutationStrategy,
        RationalityScore,
    };
    use crate::ir::{
        MemoryContext, MemoryEntry, MemoryMetadata, MemoryType, emit_plan_accepted,
        emit_plan_proposed, restore_or_initialize_ir_state,
    };
    use crate::nl::types::{CodingOptions, PlannedStep};
    use crate::service::dto::{ActionKind, IRActiveTransaction, SessionAppliedDiff};
    use tempfile::tempdir;

    fn write_design_delta_fixture(root: &std::path::Path) {
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"apply_cycle\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("write lib");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug;\npub fn render() {}\n",
        )
        .expect("write renderer");
        fs::write(
            root.join("src/debug.rs"),
            "use crate::renderer;\npub fn debug() {}\n",
        )
        .expect("write debug");
    }

    fn extract_interface_delta(root: &std::path::Path) -> DesignDelta {
        DesignDelta {
            workspace_root: root.to_path_buf(),
            impacted_crates: vec!["apply_cycle".to_string()],
            introduced_interfaces: vec!["renderer_debug_interface".to_string()],
            ..DesignDelta::default()
        }
    }

    fn empty_session_diff() -> SessionAppliedDiff {
        SessionAppliedDiff {
            summary: String::new(),
            files: Vec::new(),
            files_changed: 0,
            lines_added: 0,
            lines_removed: 0,
        }
    }

    fn empty_change_set() -> CodeChangeSet {
        CodeChangeSet {
            patches: Vec::new(),
            changes: Vec::new(),
            summary: crate::coding::ChangeSummary::default(),
            canonical_target: None,
        }
    }

    #[test]
    fn summary_uses_nl_label() {
        let plan = CommandPlan {
            intent: None,
            steps: vec![PlannedStep::Analyze(PathBuf::from("."))],
        };
        assert_eq!(render_plan_summary(&plan), "[planner: nl_v2] 1 steps");
    }

    #[test]
    fn coding_step_maps_to_safe_command() {
        let (_, args, label, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("."),
            CodingOptions::default(),
        ));
        assert_eq!(
            args,
            vec![".".to_string(), "--safe".to_string(), "--check".to_string()]
        );
        assert!(label.contains("--safe --check"));
    }

    #[test]
    fn coding_step_with_file_target_remaps_to_target_flag() {
        let (_, args, label, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("src/coding.rs"),
            CodingOptions::default(),
        ));
        assert_eq!(
            args,
            vec![
                ".".to_string(),
                "--target".to_string(),
                "src/coding.rs".to_string(),
                "--safe".to_string(),
                "--check".to_string(),
            ]
        );
        assert!(label.contains("design_cli coding . --target src/coding.rs --safe --check"));
    }

    #[test]
    fn coding_step_passes_request_through_hidden_flag() {
        let (_, args, _, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("."),
            CodingOptions {
                request: Some("repl.rs の planner_v2 接続を修正して".to_string()),
                ..CodingOptions::default()
            },
        ));
        assert_eq!(args[0], ".");
        assert_eq!(args[1], "--request");
        assert_eq!(args[2], "repl.rs の planner_v2 接続を修正して");
    }

    #[test]
    fn executor_snapshot_preserves_exact_coding_rs_path() {
        let snapshot = executor_received_path_snapshot(&PlannedStep::Analyze(PathBuf::from(
            "apps/cli/src/coding.rs",
        )))
        .expect("snapshot");
        assert!(snapshot.ends_with("apps/cli/src/coding.rs"), "{snapshot}");
    }

    #[test]
    fn executor_snapshot_preserves_exact_runtime_vm_lib_path() {
        let snapshot = executor_received_path_snapshot(&PlannedStep::Analyze(PathBuf::from(
            "crates/runtime/runtime_vm/src/lib.rs",
        )))
        .expect("snapshot");
        assert!(
            snapshot.ends_with("crates/runtime/runtime_vm/src/lib.rs"),
            "{snapshot}"
        );
    }

    #[test]
    fn explain_tradeoff_step_emits_pr_ready_block() {
        let mut session = AgentSession::new();
        let mut conversation = ConversationState::default();
        let selected = MutationCandidate {
            id: "trait-extraction-1".to_string(),
            plan: MutationPlan {
                delta: DesignDelta {
                    impacted_crates: vec!["design_cli".to_string()],
                    ..DesignDelta::default()
                },
                target_files: vec![PathBuf::from("apps/cli/Cargo.toml")],
                expected_tests: vec!["cargo test -p design_cli".to_string()],
                rollback_units: vec!["crate::design_cli".to_string()],
            },
            expected_score: Some(RationalityScore {
                maintainability: 0.91,
                extensibility: 0.92,
                risk: 0.80,
                boundary_integrity: 0.93,
                rollback_complexity: 0.87,
                total: 0.89,
            }),
            strategy: MutationStrategy::TraitExtraction,
        };
        let rejected = MutationCandidate {
            id: "adapter-insertion-1".to_string(),
            plan: selected.plan.clone(),
            expected_score: Some(RationalityScore {
                maintainability: 0.84,
                extensibility: 0.80,
                risk: 0.82,
                boundary_integrity: 0.79,
                rollback_complexity: 0.86,
                total: 0.83,
            }),
            strategy: MutationStrategy::AdapterInsertion,
        };
        conversation.last_mutation_search_result = Some(MutationSearchResult {
            candidates: vec![selected.clone(), rejected.clone()],
            selected: Some(selected),
            rejected: vec![rejected.id.clone()],
        });

        let output = execute_design_tradeoff_explanation_step(
            "設計トレードオフを要約して",
            &mut session,
            &mut conversation,
        );
        assert!(output.contains("Selected mutation: trait-extraction-1"));
        assert!(output.contains("Tradeoff summary:"));
        assert!(output.contains("Rejected:"));
        assert!(conversation.last_tradeoff_explanation.is_some());
    }

    #[test]
    fn rollback_current_transaction_clears_ir_state() {
        let mut conversation = ConversationState::default();
        conversation.last_target = Some(PathBuf::from("src/previous.rs"));
        conversation.set_active_transaction(IRActiveTransaction {
            transaction_id: "tx:src/coding.rs".to_string(),
            canonical_target: PathBuf::from("src/coding.rs"),
            pending: false,
            applied: true,
            validated: false,
            rollback_available: true,
            latest_diff_ref: None,
            latest_build_ok: None,
        });

        let output = execute_ir_rollback(&mut conversation);

        assert_eq!(output, "[rollback] reverted current IR transaction");
        assert!(conversation.active_transaction().is_none());
        assert_eq!(conversation.last_target, Some(PathBuf::from(".")));
        assert_eq!(
            conversation.ir_state.next_allowed_actions,
            vec![
                ActionKind::CodingPreview,
                ActionKind::Analyze,
                ActionKind::Refactor
            ]
        );
    }

    #[test]
    fn rollback_executor_route_is_direct() {
        let source = include_str!("executor.rs");
        let token_a = ["com", "pat"].concat();
        let token_b = ["sh", "im"].concat();
        let token_c = ["fall", "back route"].concat();
        assert!(!source.contains(&token_a));
        assert!(!source.contains(&token_b));
        assert!(!source.contains(&token_c));
    }

    #[test]
    #[should_panic(expected = "Forbidden: direct execution path. Use IR-based execution.")]
    fn direct_execute_is_forbidden() {
        let mut session = AgentSession::new();
        let mut conversation = ConversationState::default();
        let plan = CommandPlan {
            intent: None,
            steps: vec![PlannedStep::Analyze(PathBuf::from("."))],
        };
        let _ = execute_plan(&plan, &mut session, &mut conversation);
    }

    #[test]
    #[should_panic(expected = "IR bypass detected: plan")]
    fn execute_without_plan_fails() {
        let temp = tempdir().expect("tempdir");
        let recovered = restore_or_initialize_ir_state(temp.path()).expect("recover");
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let plan = CommandPlan {
            intent: None,
            steps: vec![PlannedStep::Analyze(PathBuf::from("."))],
        };

        let _ = execute_ir_plan(uuid::Uuid::new_v4(), &plan, &mut session, &mut conversation);
    }

    #[test]
    fn execute_ir_plan_with_accepted_plan_does_not_panic() {
        let temp = tempdir().expect("tempdir");
        let recovered = restore_or_initialize_ir_state(temp.path()).expect("recover");
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let plan = CommandPlan {
            intent: None,
            steps: vec![PlannedStep::Analyze(PathBuf::from("."))],
        };
        let plan_id = emit_plan_proposed(&conversation.ir_state, plan.clone(), "nl_executor_test")
            .expect("proposed");
        emit_plan_accepted(&conversation.ir_state, plan_id).expect("accepted");

        let _ = execute_ir_plan(plan_id, &plan, &mut session, &mut conversation);
    }

    #[test]
    fn design_delta_reasoning_executes_refactor_and_emits_diff() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);

        let mut conversation = ConversationState::default();
        let delta = extract_interface_delta(root);
        let output = execute_design_delta_refactor(
            "ExtractInterface(renderer -> debug) を適用して",
            None,
            1,
            &MemoryContext::default(),
            &delta,
            &MutationPlan::default(),
            &mut conversation,
        );

        assert!(output.contains("DIFF"));
        assert!(output.contains("--- "));
        assert!(output.contains("RESULT"));
        assert!(output.contains("Refactoring applied successfully."));
        assert!(
            conversation
                .active_transaction()
                .and_then(|tx| tx.latest_diff_ref.as_ref())
                .is_some()
        );
    }

    #[test]
    fn design_delta_reasoning_is_not_reapplied_twice() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);

        let mut conversation = ConversationState::default();
        let delta = extract_interface_delta(root);
        let first = execute_design_delta_refactor(
            "ExtractInterface(renderer -> debug) を適用して",
            None,
            1,
            &MemoryContext::default(),
            &delta,
            &MutationPlan::default(),
            &mut conversation,
        );
        let second = execute_design_delta_refactor(
            "ExtractInterface(renderer -> debug) を適用して",
            None,
            1,
            &MemoryContext::default(),
            &delta,
            &MutationPlan::default(),
            &mut conversation,
        );

        assert!(first.contains("Refactoring applied successfully."));
        assert_eq!(second, "No changes detected.");
    }

    #[test]
    fn design_delta_reasoning_rolls_back_on_build_failure() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        fs::write(root.join("src/broken.rs"), "pub fn broken( {\n").expect("broken file");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\npub mod broken;\n",
        )
        .expect("write broken lib");
        let original_renderer =
            fs::read_to_string(root.join("src/renderer.rs")).expect("read renderer");

        let mut conversation = ConversationState::default();
        let delta = extract_interface_delta(root);
        let output = execute_design_delta_refactor(
            "ExtractInterface(renderer -> debug) を適用して",
            None,
            1,
            &MemoryContext::default(),
            &delta,
            &MutationPlan::default(),
            &mut conversation,
        );

        assert!(output.contains("Rolled back due to validation failure."));
        assert_eq!(
            fs::read_to_string(root.join("src/renderer.rs")).expect("renderer after rollback"),
            original_renderer
        );
        assert!(conversation.active_transaction().is_none());
    }

    #[test]
    fn design_delta_rejects_trivial_import_only_diff() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let diff = crate::coding::UnifiedDiffPreview {
            files: vec![crate::coding::CodeDiff {
                file: PathBuf::from("src/lib.rs"),
                before_label: "a/src/lib.rs".to_string(),
                after_label: "b/src/lib.rs".to_string(),
                hunks: vec![crate::coding::Hunk {
                    header: "@@ -1 +1 @@".to_string(),
                    lines: vec![
                        crate::coding::DiffLine::Removed("use crate::old;".to_string()),
                        crate::coding::DiffLine::Added("use crate::new;".to_string()),
                    ],
                }],
                added_lines: 1,
                removed_lines: 1,
            }],
            summary: crate::coding::UnifiedDiffSummary {
                file_count: 1,
                added_lines: 1,
                removed_lines: 1,
                skipped_binary_files: 0,
                truncated: false,
            },
        };

        let meaning = classify_diff_meaning(&diff);
        let validation = validate_pre_execution(
            "修正して",
            &extract_interface_delta(root),
            &MutationPlan::default(),
            &crate::coding::CodeChangeSet {
                patches: Vec::new(),
                changes: Vec::new(),
                summary: crate::coding::ChangeSummary::default(),
                canonical_target: None,
            },
            &meaning,
            &PathBuf::from("src/lib.rs"),
            &SessionAppliedDiff {
                summary: String::new(),
                files: Vec::new(),
                files_changed: 0,
                lines_added: 0,
                lines_removed: 0,
            },
            &ConversationState::default(),
        );

        assert!(!validation.issues.is_empty());
        assert_eq!(
            render_validation_result(&validation),
            "No changes detected."
        );
    }

    #[test]
    fn selection_improves_accuracy() {
        let failing = CandidatePatch {
            memory_id: "m1".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.9,
            embedding_score: 0.9,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 10,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let succeeding = CandidatePatch {
            memory_id: "m2".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.4,
            embedding_score: 0.4,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 20,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let selected = select_best_candidate_patch(vec![
            (
                failing.clone(),
                SimulationResult {
                    compile_ok: false,
                    error_count: 4,
                    warnings: 0,
                    diff_size: 10,
                    test_result: TestResult::default(),
                    semantic_score: 0.2,
                    intent_score: 0.5,
                },
                evaluate_candidate_patch(
                    &failing,
                    &SimulationResult {
                        compile_ok: false,
                        error_count: 4,
                        warnings: 0,
                        diff_size: 10,
                        test_result: TestResult::default(),
                        semantic_score: 0.2,
                        intent_score: 0.5,
                    },
                ),
            ),
            (
                succeeding.clone(),
                SimulationResult {
                    compile_ok: true,
                    error_count: 0,
                    warnings: 0,
                    diff_size: 2,
                    test_result: TestResult {
                        passed: 3,
                        failed: 0,
                        skipped: 0,
                    },
                    semantic_score: 1.0,
                    intent_score: 0.8,
                },
                evaluate_candidate_patch(
                    &succeeding,
                    &SimulationResult {
                        compile_ok: true,
                        error_count: 0,
                        warnings: 0,
                        diff_size: 2,
                        test_result: TestResult {
                            passed: 3,
                            failed: 0,
                            skipped: 0,
                        },
                        semantic_score: 1.0,
                        intent_score: 0.8,
                    },
                ),
            ),
        ])
        .expect("selected");

        assert_eq!(selected.0.memory_id, "m2");
    }

    #[test]
    fn deterministic_selection() {
        let candidate = CandidatePatch {
            memory_id: "m1".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.5,
            embedding_score: 0.6,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 5,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let evaluated = vec![(
            candidate.clone(),
            SimulationResult {
                compile_ok: true,
                error_count: 0,
                warnings: 0,
                diff_size: 1,
                test_result: TestResult {
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                },
                semantic_score: 1.0,
                intent_score: 0.5,
            },
            evaluate_candidate_patch(
                &candidate,
                &SimulationResult {
                    compile_ok: true,
                    error_count: 0,
                    warnings: 0,
                    diff_size: 1,
                    test_result: TestResult {
                        passed: 1,
                        failed: 0,
                        skipped: 0,
                    },
                    semantic_score: 1.0,
                    intent_score: 0.5,
                },
            ),
        )];
        let lhs = select_best_candidate_patch(evaluated.clone()).expect("lhs");
        let rhs = select_best_candidate_patch(evaluated).expect("rhs");
        assert_eq!(lhs.0.memory_id, rhs.0.memory_id);
    }

    #[test]
    fn respects_step_index_constraint() {
        let context = MemoryContext {
            entries: vec![
                MemoryEntry {
                    memory_id: "past".to_string(),
                    source_event: Uuid::new_v4(),
                    memory_type: MemoryType::SemanticHint,
                    content: serde_json::json!({ "summary": "extract interface" }),
                    embedding: None,
                    success_count: 0,
                    failure_count: 0,
                    metadata: MemoryMetadata {
                        timestamp: 1,
                        step_index: 0,
                        relevance: 1.0,
                        tags: vec!["extract".to_string()],
                    },
                },
                MemoryEntry {
                    memory_id: "future".to_string(),
                    source_event: Uuid::new_v4(),
                    memory_type: MemoryType::SemanticHint,
                    content: serde_json::json!({ "summary": "remove dependency" }),
                    embedding: None,
                    success_count: 0,
                    failure_count: 0,
                    metadata: MemoryMetadata {
                        timestamp: 2,
                        step_index: 3,
                        relevance: 1.0,
                        tags: vec!["remove".to_string()],
                    },
                },
            ],
        };
        let filtered = candidate_memories_for_step(&context, 2);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].memory_id, "past");
    }

    #[test]
    fn stable_ordering() {
        let first = CandidatePatch {
            memory_id: "a".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.5,
            embedding_score: 0.5,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 10,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let second = CandidatePatch {
            memory_id: "b".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.5,
            embedding_score: 0.5,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 10,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let sim = SimulationResult {
            compile_ok: true,
            error_count: 0,
            warnings: 0,
            diff_size: 1,
            test_result: TestResult {
                passed: 1,
                failed: 0,
                skipped: 0,
            },
            semantic_score: 1.0,
            intent_score: 0.5,
        };
        let selected = select_best_candidate_patch(vec![
            (
                second.clone(),
                sim.clone(),
                evaluate_candidate_patch(&second, &sim),
            ),
            (
                first.clone(),
                sim.clone(),
                evaluate_candidate_patch(&first, &sim),
            ),
        ])
        .expect("selected");
        assert_eq!(selected.0.memory_id, "a");
    }

    #[test]
    fn success_increases_score() {
        let baseline = CandidatePatch {
            memory_id: "m1".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.5,
            embedding_score: 0.5,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 1,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let learned = CandidatePatch {
            success_rate: 0.5,
            ..baseline.clone()
        };
        let sim = SimulationResult {
            compile_ok: true,
            error_count: 0,
            warnings: 0,
            diff_size: 1,
            test_result: TestResult {
                passed: 1,
                failed: 0,
                skipped: 0,
            },
            semantic_score: 1.0,
            intent_score: 0.5,
        };

        assert!(
            evaluate_candidate_patch(&learned, &sim).total_score
                > evaluate_candidate_patch(&baseline, &sim).total_score
        );
    }

    #[test]
    fn failure_decreases_score() {
        let healthy = CandidatePatch {
            memory_id: "m1".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.5,
            embedding_score: 0.5,
            intent_score: 0.5,
            success_rate: 0.5,
            timestamp: 1,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let failed = CandidatePatch {
            success_rate: 0.0,
            ..healthy.clone()
        };
        let sim = SimulationResult {
            compile_ok: true,
            error_count: 0,
            warnings: 0,
            diff_size: 1,
            test_result: TestResult {
                passed: 1,
                failed: 0,
                skipped: 0,
            },
            semantic_score: 1.0,
            intent_score: 0.5,
        };

        assert!(
            evaluate_candidate_patch(&failed, &sim).total_score
                < evaluate_candidate_patch(&healthy, &sim).total_score
        );
    }

    #[test]
    fn deterministic_learning() {
        let candidate = CandidatePatch {
            memory_id: "m1".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.4,
            embedding_score: 0.4,
            intent_score: 0.5,
            success_rate: 0.5,
            timestamp: 2,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let sim = SimulationResult {
            compile_ok: true,
            error_count: 0,
            warnings: 0,
            diff_size: 2,
            test_result: TestResult {
                passed: 2,
                failed: 0,
                skipped: 0,
            },
            semantic_score: 1.0,
            intent_score: 0.5,
        };

        let lhs = evaluate_candidate_patch(&candidate, &sim);
        let rhs = evaluate_candidate_patch(&candidate, &sim);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn no_learning_without_apply() {
        let candidate = CandidatePatch {
            memory_id: "mem:simulated".to_string(),
            origin: CandidateOrigin::Memory,
            patch: empty_change_set(),
            symbolic_score: 0.3,
            embedding_score: 0.3,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 1,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let before = candidate.success_rate;
        let _ = simulate_candidate_patch(Path::new("."), &candidate);
        assert_eq!(candidate.success_rate, before);
    }

    #[test]
    fn test_improves_selection() {
        let weak = CandidatePatch {
            memory_id: "weak".to_string(),
            origin: CandidateOrigin::RuleBased,
            patch: empty_change_set(),
            symbolic_score: 0.4,
            embedding_score: 0.4,
            intent_score: 0.5,
            success_rate: 0.0,
            timestamp: 1,
            target: RefactorTarget::Cycle,
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let strong = CandidatePatch {
            memory_id: "strong".to_string(),
            timestamp: 2,
            ..weak.clone()
        };
        let weak_eval = evaluate_candidate_patch(
            &weak,
            &SimulationResult {
                compile_ok: true,
                error_count: 0,
                warnings: 0,
                diff_size: 1,
                test_result: TestResult {
                    passed: 0,
                    failed: 2,
                    skipped: 0,
                },
                semantic_score: 1.0,
                intent_score: 0.5,
            },
        );
        let strong_eval = evaluate_candidate_patch(
            &strong,
            &SimulationResult {
                compile_ok: true,
                error_count: 0,
                warnings: 0,
                diff_size: 1,
                test_result: TestResult {
                    passed: 3,
                    failed: 0,
                    skipped: 0,
                },
                semantic_score: 1.0,
                intent_score: 0.5,
            },
        );
        assert!(strong_eval.total_score > weak_eval.total_score);
    }

    #[test]
    fn semantic_penalty_applied() {
        let safe = crate::coding::UnifiedDiffPreview::default();
        let unsafe_diff = crate::coding::UnifiedDiffPreview {
            files: vec![crate::coding::CodeDiff {
                file: PathBuf::from("src/lib.rs"),
                before_label: "a/src/lib.rs".to_string(),
                after_label: "b/src/lib.rs".to_string(),
                hunks: vec![crate::coding::Hunk {
                    header: "@@ -1 +1 @@".to_string(),
                    lines: vec![crate::coding::DiffLine::Added(
                        "pub unsafe fn mutate() {}".to_string(),
                    )],
                }],
                added_lines: 1,
                removed_lines: 0,
            }],
            summary: crate::coding::UnifiedDiffSummary {
                file_count: 1,
                added_lines: 1,
                removed_lines: 0,
                skipped_binary_files: 0,
                truncated: false,
            },
        };
        assert!(evaluate_semantic_score(&unsafe_diff) < evaluate_semantic_score(&safe));
    }

    #[test]
    fn intent_consistency_scored() {
        let add_feature = CodingIntent::AddFeature {
            target: PathBuf::from("src/lib.rs"),
            spec: "add feature".to_string(),
        };
        let diff = crate::coding::UnifiedDiffPreview {
            files: vec![crate::coding::CodeDiff {
                file: PathBuf::from("src/lib.rs"),
                before_label: "a/src/lib.rs".to_string(),
                after_label: "b/src/lib.rs".to_string(),
                hunks: vec![crate::coding::Hunk {
                    header: "@@ -1 +1 @@".to_string(),
                    lines: vec![
                        crate::coding::DiffLine::Added("fn new_feature() {}".to_string()),
                        crate::coding::DiffLine::Added("let value = 1;".to_string()),
                        crate::coding::DiffLine::Added("value".to_string()),
                    ],
                }],
                added_lines: 3,
                removed_lines: 0,
            }],
            summary: crate::coding::UnifiedDiffSummary {
                file_count: 1,
                added_lines: 3,
                removed_lines: 0,
                skipped_binary_files: 0,
                truncated: false,
            },
        };
        assert!(evaluate_intent_consistency(Some(&add_feature), &diff) > 0.9);
    }

    #[test]
    fn deterministic_validation() {
        let result = TestResult {
            passed: 2,
            failed: 1,
            skipped: 0,
        };
        let lhs = test_score(&result);
        let rhs = test_score(&result);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn wm_generates_candidates() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::Refactor {
            scope: crate::nl::types::IntentScope::Workspace,
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "refactor the architecture",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };

        let generated = WorldModelGenerator.generate(&input);
        assert!(!generated.is_empty());
        assert!(
            generated
                .iter()
                .all(|candidate| candidate.origin == CandidateOrigin::WorldModel)
        );
    }

    #[test]
    fn wm_candidates_deterministic() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::FixBug {
            target: PathBuf::from("src/renderer.rs"),
            description: "fix renderer".to_string(),
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "fix renderer bug",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };

        let lhs = WorldModelGenerator.generate(&input);
        let rhs = WorldModelGenerator.generate(&input);
        assert_eq!(
            lhs.iter()
                .map(|candidate| refactor_target_descriptor(&candidate.target))
                .collect::<Vec<_>>(),
            rhs.iter()
                .map(|candidate| refactor_target_descriptor(&candidate.target))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn wm_candidate_selected_when_better() {
        let wm = CandidatePatch {
            memory_id: "wm".to_string(),
            origin: CandidateOrigin::WorldModel,
            patch: empty_change_set(),
            symbolic_score: 0.4,
            embedding_score: 0.4,
            intent_score: 1.0,
            success_rate: 0.0,
            timestamp: 1,
            target: RefactorTarget::IntroduceService("renderer".to_string()),
            canonical_target: PathBuf::from("."),
            diff_preview: crate::coding::UnifiedDiffPreview::default(),
            snapshot: empty_session_diff(),
            pre_validation: DesignDeltaValidation::success(),
        };
        let rule = CandidatePatch {
            memory_id: "rule".to_string(),
            origin: CandidateOrigin::RuleBased,
            timestamp: 2,
            intent_score: 0.2,
            ..wm.clone()
        };

        let selected = select_best_candidate_patch(vec![
            (
                rule.clone(),
                SimulationResult {
                    compile_ok: true,
                    error_count: 0,
                    warnings: 0,
                    diff_size: 2,
                    test_result: TestResult {
                        passed: 1,
                        failed: 0,
                        skipped: 0,
                    },
                    semantic_score: 0.6,
                    intent_score: 0.2,
                },
                evaluate_candidate_patch(
                    &rule,
                    &SimulationResult {
                        compile_ok: true,
                        error_count: 0,
                        warnings: 0,
                        diff_size: 2,
                        test_result: TestResult {
                            passed: 1,
                            failed: 0,
                            skipped: 0,
                        },
                        semantic_score: 0.6,
                        intent_score: 0.2,
                    },
                ),
            ),
            (
                wm.clone(),
                SimulationResult {
                    compile_ok: true,
                    error_count: 0,
                    warnings: 0,
                    diff_size: 1,
                    test_result: TestResult {
                        passed: 3,
                        failed: 0,
                        skipped: 0,
                    },
                    semantic_score: 1.0,
                    intent_score: 1.0,
                },
                evaluate_candidate_patch(
                    &wm,
                    &SimulationResult {
                        compile_ok: true,
                        error_count: 0,
                        warnings: 0,
                        diff_size: 1,
                        test_result: TestResult {
                            passed: 3,
                            failed: 0,
                            skipped: 0,
                        },
                        semantic_score: 1.0,
                        intent_score: 1.0,
                    },
                ),
            ),
        ])
        .expect("selected");

        assert_eq!(selected.0.origin, CandidateOrigin::WorldModel);
    }

    #[test]
    fn wm_candidate_rejected_by_validation() {
        let simulation = SimulationResult {
            compile_ok: true,
            error_count: 0,
            warnings: 0,
            diff_size: 1,
            test_result: TestResult {
                passed: 0,
                failed: 2,
                skipped: 0,
            },
            semantic_score: 1.0,
            intent_score: 1.0,
        };
        assert!(!simulation_passes_validation(&simulation));
    }

    #[test]
    fn multistep_generates_candidates() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::Refactor {
            scope: crate::nl::types::IntentScope::Workspace,
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "refactor the architecture",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };

        let generated = generate_exploration_candidates(&input);
        assert!(
            generated
                .iter()
                .any(|candidate| candidate.actions.len() > 1)
        );
    }

    #[test]
    fn multistep_solves_unreachable_by_single() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::Refactor {
            scope: crate::nl::types::IntentScope::Workspace,
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "extract interface and remove dependency",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };

        let single = RuleBasedGenerator
            .generate(&input)
            .into_iter()
            .chain(WorldModelGenerator.generate(&input))
            .collect::<Vec<_>>();
        let multistep = generate_multistep_candidates(&input, &single);

        assert!(single.iter().all(|candidate| candidate.actions.len() == 1));
        assert!(multistep.iter().any(|candidate| {
            let descriptor = generated_candidate_descriptor(candidate);
            descriptor.contains("extract-interface") && descriptor.contains("remove-dependency")
        }));
    }

    #[test]
    fn deterministic_multistep() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::FixBug {
            target: PathBuf::from("src/renderer.rs"),
            description: "fix renderer".to_string(),
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "fix renderer bug",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };
        let seeds = generate_exploration_candidates(&input);

        let lhs = generate_multistep_candidates(&input, &seeds);
        let rhs = generate_multistep_candidates(&input, &seeds);

        assert_eq!(
            lhs.iter()
                .map(generated_candidate_descriptor)
                .collect::<Vec<_>>(),
            rhs.iter()
                .map(generated_candidate_descriptor)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn multistep_respects_constraints() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::AddFeature {
            target: PathBuf::from("src/renderer.rs"),
            spec: "add feature".to_string(),
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "add renderer feature",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };
        let seeds = generate_exploration_candidates(&input);
        let multistep = generate_multistep_candidates(&input, &seeds);

        assert!(multistep.len() <= MULTISTEP_TOP_K);
        assert!(
            multistep
                .iter()
                .all(|candidate| candidate.actions.len() <= MULTISTEP_MAX_DEPTH)
        );
    }

    fn naive_multistep_candidates(
        input: &ExplorationInput<'_>,
        seeds: &[GeneratedCandidate],
    ) -> Vec<GeneratedCandidate> {
        let mut queue = std::collections::VecDeque::new();
        for seed in seeds {
            queue.push_back(ExplorationState {
                applied_actions: seed.actions.clone(),
                depth: seed.actions.len(),
                origin: seed.origin.clone(),
                heuristic_score: heuristic_score_for_actions(input, &seed.actions),
            });
        }
        let mut generated = Vec::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut expanded_nodes = 0usize;
        while let Some(state) = queue.pop_front() {
            if state.depth >= MULTISTEP_MAX_DEPTH || expanded_nodes >= MULTISTEP_MAX_TOTAL_NODES {
                continue;
            }
            let Some(last_action) = state.applied_actions.last() else {
                continue;
            };
            for next in enumerate_follow_up_actions(input, last_action, &state)
                .into_iter()
                .take(MULTISTEP_MAX_BRANCH)
            {
                let mut actions = state.applied_actions.clone();
                actions.push(next.clone());
                let descriptor = action_sequence_descriptor(&actions);
                if !visited.insert(descriptor) {
                    continue;
                }
                let origin =
                    merge_candidate_origins(&state.origin, &candidate_origin_for_action(&next));
                generated.push(GeneratedCandidate {
                    target: next.target.clone(),
                    origin: origin.clone(),
                    actions: actions.clone(),
                });
                queue.push_back(ExplorationState {
                    applied_actions: actions,
                    depth: state.depth + 1,
                    origin,
                    heuristic_score: 0.0,
                });
                expanded_nodes += 1;
            }
        }
        generated
    }

    #[test]
    fn beam_reduces_nodes() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::Refactor {
            scope: crate::nl::types::IntentScope::Workspace,
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "refactor the architecture broadly",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };
        let seeds = RuleBasedGenerator
            .generate(&input)
            .into_iter()
            .chain(WorldModelGenerator.generate(&input))
            .collect::<Vec<_>>();
        let naive = naive_multistep_candidates(&input, &seeds);
        let beam = generate_multistep_candidates(&input, &seeds);
        assert!(beam.len() <= naive.len());
    }

    #[test]
    fn heuristic_prioritizes_better_paths() {
        let intent = CodingIntent::FixBug {
            target: PathBuf::from("src/lib.rs"),
            description: "fix".to_string(),
        };
        let better = vec![
            exploration_action_for_target(&RefactorTarget::ExtractInterface {
                from: "renderer".to_string(),
                to: "debug".to_string(),
            }),
            exploration_action_for_target(&RefactorTarget::RemoveDependency {
                from: "renderer".to_string(),
                to: "debug".to_string(),
            }),
        ];
        let worse = vec![
            exploration_action_for_target(&RefactorTarget::MergeModule(vec![
                "renderer".to_string(),
                "debug".to_string(),
            ])),
            exploration_action_for_target(&RefactorTarget::FileMove(PathBuf::from("src/debug.rs"))),
        ];
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "fix renderer bug",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };
        assert!(
            heuristic_score_for_actions(&input, &better)
                > heuristic_score_for_actions(&input, &worse)
        );
    }

    #[test]
    fn deterministic_beam_search() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::Refactor {
            scope: crate::nl::types::IntentScope::Workspace,
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "refactor the architecture broadly",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };
        let seeds = RuleBasedGenerator
            .generate(&input)
            .into_iter()
            .chain(WorldModelGenerator.generate(&input))
            .collect::<Vec<_>>();
        let lhs = generate_multistep_candidates(&input, &seeds);
        let rhs = generate_multistep_candidates(&input, &seeds);
        assert_eq!(
            lhs.iter()
                .map(generated_candidate_descriptor)
                .collect::<Vec<_>>(),
            rhs.iter()
                .map(generated_candidate_descriptor)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pruning_reduces_invalid_paths() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        write_design_delta_fixture(root);
        let analysis = analyze_path(root).expect("analysis");
        let delta = extract_interface_delta(root);
        let intent = CodingIntent::Refactor {
            scope: crate::nl::types::IntentScope::Workspace,
        };
        let input = ExplorationInput {
            intent: Some(&intent),
            context: &analysis,
            spec: "refactor the architecture broadly",
            delta: &delta,
            step_index: 1,
            memory_context: &MemoryContext::default(),
        };
        let state = ExplorationState {
            applied_actions: vec![exploration_action_for_target(&RefactorTarget::ModuleSplit(
                "renderer".to_string(),
            ))],
            depth: 1,
            origin: CandidateOrigin::RuleBased,
            heuristic_score: 0.0,
        };
        let invalid = exploration_action_for_target(&RefactorTarget::MergeModule(vec![
            "renderer".to_string(),
            "debug".to_string(),
        ]));
        assert!(increases_invalidity(&invalid, &state));
        assert!(!early_validate_action(
            &input,
            &exploration_action_for_target(&RefactorTarget::ExtractInterface {
                from: "missing".to_string(),
                to: "edge".to_string(),
            })
        ));
    }
}
