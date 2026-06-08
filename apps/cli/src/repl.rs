/// Thin REPL UI for DBM_CLI.
///
/// Phase 1 boundary:
/// - REPL reads input and renders output only.
/// - Core is the only execution and reasoning entry point.
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::core::{
    CoreEvent, CoreExecutor, CoreRequest, CoreState, DesignDocument, RuntimeCoreBridge,
};
use crate::intent_resolution::{
    ConfirmationEngine, ExecutionContext, ExecutionRouter, IntentResolutionEngine,
    RecommendedAction, ResolvedIntent,
};
use crate::nl::normalization::{
    NormalizedRuntimeInput, RuntimeCommandCertainty, RuntimeInputSource,
    RuntimeNormalizationRejection, confirmation_like_target_failure, normalize_runtime_input,
};
use crate::nl::planner::InstructionPlan;
use crate::nl::runtime_intent::RuntimeIntent;
use crate::pipeline::PipelineState;
use crate::runtime::shell::{
    PreviewCandidate, ResolvedExecutionTarget, RuntimeAuthorityTarget, RuntimeCommandDispatcher,
    commit_preview_candidate, empty_runtime_payload, runtime_preview_from_intent,
};
use crate::session::AgentSession;
use crate::specification_bridge::{
    DesignSpecificationRecognizer, DiagnosisDomain, ImplementationPlan, ImplementationPlanner,
    RepairPlan, RepairPlanner, SpecificationContext, SpecificationKind, classify_specification,
};
use crate::state::State;
use crate::tui::composer::ComposerViewState;
use crate::tui::core::to_ui_event;
use crate::tui::rendering::{ProjectionSnapshot, RenderSnapshot};
use crate::tui::state::TuiState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecificationCaptureState {
    Idle,
    Capturing,
    Completed,
}

#[derive(Debug, Clone)]
pub struct SpecificationCaptureSession {
    pub session_id: String,
    pub state: SpecificationCaptureState,
    pub lines: Vec<String>,
    pub started_at: SystemTime,
}

impl SpecificationCaptureSession {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            state: SpecificationCaptureState::Idle,
            lines: Vec::new(),
            started_at: SystemTime::now(),
        }
    }

    pub fn reset(&mut self) {
        let previous = self.state;
        self.session_id = uuid::Uuid::new_v4().to_string();
        self.state = SpecificationCaptureState::Capturing;
        self.lines.clear();
        self.started_at = SystemTime::now();
        eprintln!("[SPEC_STATE]\nprevious={:?}\nnext=Capturing", previous);
    }
}

/// Thin UI cache for the REPL.  Phase 4.5: all pipeline/design/proposal state
/// lives in `core_snapshot`; this struct is just a read-only cache.
#[derive(Debug, Clone)]
struct ReplUiState {
    core_snapshot: CoreState,
    runtime: TuiState,
    semantic_state: ReplSemanticState,
}

impl Default for ReplUiState {
    fn default() -> Self {
        Self {
            core_snapshot: CoreState::default(),
            runtime: TuiState::new(empty_runtime_payload()),
            semantic_state: ReplSemanticState::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplSemanticState {
    pub last_preview: Option<PreviewState>,
    pub last_validation: Option<ValidationState>,
    pub last_apply: Option<ApplyState>,
    pub rollback_checkpoint: Option<RollbackCheckpoint>,
    pub last_mutation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewState {
    pub projection: ProjectionSnapshot,
    pub rendered_output: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationState {
    pub projection_hash: String,
    pub valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyState {
    pub projection: ProjectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackCheckpoint {
    pub projection_before: ProjectionSnapshot,
    pub projection_after: ProjectionSnapshot,
}

/// REPLを起動して入力ループを実行する。
///
/// `/exit` または EOF (Ctrl+D) で終了する。
pub fn run_repl<R, W>(workspace_root: PathBuf, reader: &mut R, writer: &mut W) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let core = RuntimeCoreBridge::with_defaults();
    run_repl_with_core(workspace_root, reader, writer, &core)
}

fn run_repl_with_core<R, W>(
    workspace_root: PathBuf,
    reader: &mut R,
    writer: &mut W,
    core: &dyn CoreExecutor,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let mut ui = ReplUiState::default();
    let mut spec_capture = SpecificationCaptureSession::new();
    // DBM-SPECIFICATION-MULTILINE-CAPTURE-VALIDATION-SPEC v1.0 §2: mutable to receive parsed plan
    let mut pending_plan: Option<InstructionPlan> = None;
    let mut pending_specification: Option<SpecificationContext> = None;
    let mut pending_confirmation: Option<crate::intent_resolution::PendingConfirmation> = None;
    let mut pending_resolved_intent: Option<ResolvedIntent> = None;

    print_banner(writer)?;

    for line in reader.lines() {
        let input = line.map_err(|err| err.to_string())?;
        let trimmed = input.trim();

        // DBM-SPECIFICATION-MULTILINE-CAPTURE-VALIDATION-SPEC v1.0 §2
        // /begin spec is the explicit capture delimiter. Enter capture mode without
        // buffering the delimiter itself so body lines are pure spec content.
        if trimmed.to_lowercase() == "/begin spec" {
            eprintln!("[SPEC_CAPTURE_CONSUMED]\ncommand=\"/begin spec\"");
            if spec_capture.state == SpecificationCaptureState::Capturing {
                eprintln!(
                    "[SPEC_SESSION]\nsession={}\naction=reset",
                    spec_capture.session_id
                );
                eprintln!(
                    "[SPEC_BOUNDARY_TRACE]\nprevious_state={:?}\nreason=\"new_specification_detected\"",
                    spec_capture.state
                );
            }
            spec_capture.reset();
            eprintln!(
                "[SPEC_SESSION]\nsession={}\nstate={:?}\naction=create",
                spec_capture.session_id, spec_capture.state
            );
            continue;
        }

        // Global case-insensitive /end handling
        let normalized = trimmed.to_ascii_lowercase();
        eprintln!(
            "[INPUT_TRACE]\nraw='{}'\ntrimmed='{}'\nnormalized='{}'",
            input, trimmed, normalized
        );
        eprintln!("[SPEC_END_TRACE]\nchecking_end_command");
        if is_end_command(trimmed) {
            eprintln!("[SPEC_END_TRACE]\nend_command_matched");

            match spec_capture.state {
                SpecificationCaptureState::Capturing => {
                    eprintln!("[SPEC_CAPTURE_CONSUMED]\ncommand=\"/end\"\nreason=\"success\"");
                    let current_session_id = spec_capture.session_id.clone();
                    let captured_lines = std::mem::take(&mut spec_capture.lines);
                    let full_text = captured_lines.join("\n");

                    eprintln!(
                        "[SPEC_SESSION]\nsession={}\naction=dispatch\npayload_lines={}\npayload_chars={}",
                        current_session_id,
                        captured_lines.len(),
                        full_text.len()
                    );
                    eprintln!(
                        "[SPEC_DISPATCH]\nsession={}\npayload_lines={}\npayload_chars={}",
                        current_session_id,
                        captured_lines.len(),
                        full_text.len()
                    );

                    if full_text.trim().is_empty() {
                        writeln!(writer, "[SPEC] rejected: empty specification body")
                            .map_err(|err| err.to_string())?;
                        eprintln!(
                            "[SPEC_COHERENCE]\nsession={}\nstatus=invalid\nreason=\"empty_body\"",
                            current_session_id
                        );
                    } else {
                        dispatch_captured_specification(
                            &full_text,
                            &current_session_id,
                            writer,
                            &mut pending_plan,
                            &mut pending_specification,
                        )?;
                    }
                    eprintln!(
                        "[SPEC_STATE]\nprevious={:?}\nnext=Completed",
                        spec_capture.state
                    );
                    spec_capture.state = SpecificationCaptureState::Completed;
                }
                SpecificationCaptureState::Completed => {
                    eprintln!(
                        "[SPEC_CAPTURE_CONSUMED]\ncommand=\"/end\"\nreason=\"duplicate_end\""
                    );
                }
                SpecificationCaptureState::Idle => {
                    eprintln!(
                        "[SPEC_CAPTURE_CONSUMED]\ncommand=\"/end\"\nreason=\"no_active_capture\""
                    );
                }
            }
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }
        eprintln!("[SPEC_END_TRACE]\nend_command_not_matched");

        if spec_capture.state == SpecificationCaptureState::Capturing {
            eprintln!(
                "[SPEC_STATE_TRACE] before_end_check state={:?}",
                spec_capture.state
            );

            if normalized.starts_with("/end") {
                eprintln!(
                    "[SPEC_CAPTURE_CONSUMED]\ncommand=\"{}\"\nreason=\"invalid_end_command\"",
                    trimmed
                );
                writeln!(writer, "[SPEC] rejected: invalid end command")
                    .map_err(|err| err.to_string())?;
                spec_capture.lines.clear();
                eprintln!(
                    "[SPEC_STATE]\nprevious={:?}\nnext=Completed",
                    spec_capture.state
                );
                spec_capture.state = SpecificationCaptureState::Completed;
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }

            // TR-5: Contamination detection
            if trimmed.contains("DBM-") && !trimmed.starts_with("DBM-") {
                let token = extract_contamination_token(trimmed);
                eprintln!("[SPEC_BOUNDARY_WARNING]\ntoken=\"{}\"", token);
                eprintln!(
                    "[SPEC_BOUNDARY_VIOLATION]\nsession={}\nreason=\"contamination_detected\"",
                    spec_capture.session_id
                );
            }

            // TR-2: Buffer Append
            eprintln!("[SPEC_END_TRACE] append_path_entered");
            eprintln!("[SPEC_END_TRACE] append_payload='{}'", trimmed);
            spec_capture.lines.push(input);
            eprintln!(
                "[SPEC_SESSION]\nsession={}\naction=append\nbuffer_lines={}",
                spec_capture.session_id,
                spec_capture.lines.len()
            ); // TR-2
            continue;
        }
        // ───────────────────────────────────────────────────────────────────

        if trimmed.is_empty() && ui.core_snapshot.status != PipelineState::Previewed {
            continue;
        }
        if trimmed == "promote" {
            promote_pending_plan(
                &mut ui,
                workspace_root.as_path(),
                pending_plan.as_ref(),
                writer,
            )?;
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }
        if trimmed == "validate-plan" {
            validate_last_applied_plan(&ui, workspace_root.as_path(), writer)?;
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }
        if trimmed == "apply" {
            if pending_plan.is_some() && ui.runtime.promoted_plan.is_none() {
                writeln!(writer, "[APPLY] rejected: pending plan not promoted")
                    .map_err(|err| err.to_string())?;
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }
            if ui.runtime.active_transaction.is_none() {
                writeln!(writer, "[APPLY] rejected: no preview transaction")
                    .map_err(|err| err.to_string())?;
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }
        }
        if trimmed == "/save design" {
            save_design(
                workspace_root.as_path(),
                ui.core_snapshot.design.as_ref(),
                writer,
            )?;
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }

        if let Some(pending) = pending_confirmation.clone() {
            match confirmation_response(trimmed, pending.action) {
                ConfirmationResponse::Execute(action) => {
                    pending_confirmation = None;
                    let resolved = pending_resolved_intent
                        .take()
                        .unwrap_or_else(|| IntentResolutionEngine::resolve(trimmed));
                    dispatch_repl_action(
                        action,
                        trimmed,
                        resolved,
                        core,
                        workspace_root.as_path(),
                        &mut ui,
                        writer,
                    )?;
                    writer.flush().map_err(|err| err.to_string())?;
                    continue;
                }
                ConfirmationResponse::Cancel => {
                    pending_confirmation = None;
                    pending_resolved_intent = None;
                    writeln!(writer, "実行をキャンセルしました。")
                        .map_err(|err| err.to_string())?;
                    writer.flush().map_err(|err| err.to_string())?;
                    continue;
                }
                ConfirmationResponse::Unmatched => {}
            }
        }

        let resolved = IntentResolutionEngine::resolve(trimmed);
        if should_intercept_with_intent_layer(&resolved) {
            if resolved.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD {
                writeln!(writer, "{}", ConfirmationEngine::prompt(&resolved))
                    .map_err(|err| err.to_string())?;
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }
            if let Some(pending) = ConfirmationEngine::pending(&resolved) {
                writeln!(writer, "{}", pending.summary).map_err(|err| err.to_string())?;
                pending_confirmation = Some(pending);
                pending_resolved_intent = Some(resolved);
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }
            if resolved.recommended_action() == RecommendedAction::RunAnalyze {
                writeln!(writer, "{}", ConfirmationEngine::prompt(&resolved))
                    .map_err(|err| err.to_string())?;
                dispatch_repl_action(
                    RecommendedAction::RunAnalyze,
                    trimmed,
                    resolved,
                    core,
                    workspace_root.as_path(),
                    &mut ui,
                    writer,
                )?;
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }
        }

        if let Some(args) = parse_repl_memory_log_command(trimmed) {
            match crate::commands::memory::dispatch_memory_command(&args) {
                Ok(out) => writeln!(writer, "{}", out.message).map_err(|err| err.to_string())?,
                Err(err) => writeln!(writer, "[ERROR] {err}").map_err(|err| err.to_string())?,
            }
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }

        if let Some(args) = parse_repl_git_command(trimmed) {
            let (_code, output) =
                crate::runtime::shell::runtime_apply_git_command(workspace_root.as_path(), &args);
            let rendered = serde_json::to_string(&output).map_err(|err| err.to_string())?;
            writeln!(writer, "{rendered}").map_err(|err| err.to_string())?;
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }

        if let Some(events) =
            RuntimeCommandDispatcher::dispatch(&mut ui.runtime, workspace_root.as_path(), trimmed)
        {
            ui.semantic_state
                .capture_runtime_command(trimmed, &ui.runtime);
            for event in events {
                writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
            }
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }

        if should_try_runtime_intent(trimmed)
            && let Some(events) =
                dispatch_normalized_runtime_intent(&mut ui, workspace_root.as_path(), trimmed)
        {
            for event in events {
                writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
            }
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }

        eprintln!("[SPEC_RECOGNITION]\nchecking");
        if DesignSpecificationRecognizer::is_design_specification_start(trimmed) {
            let reason =
                DesignSpecificationRecognizer::recognition_reason(trimmed).unwrap_or("unknown");
            eprintln!("[SPEC_RECOGNITION]\nkind=DesignSpecification\nreason=\"{reason}\"");
            if spec_capture.state == SpecificationCaptureState::Completed {
                eprintln!(
                    "[SPEC_BOUNDARY_TRACE]\nprevious_state={:?}\nreason=\"new_specification_detected\"",
                    spec_capture.state
                );
            }
            spec_capture.reset();
            eprintln!(
                "[SPEC_SESSION]\nsession={}\nstate={:?}\naction=create",
                spec_capture.session_id, spec_capture.state
            );
            eprintln!("[SPEC_END_TRACE] append_path_entered");
            eprintln!("[SPEC_END_TRACE] append_payload='{}'", trimmed);
            spec_capture.lines.push(input);
            eprintln!(
                "[SPEC_SESSION]\nsession={}\naction=append\nbuffer_lines={}",
                spec_capture.session_id,
                spec_capture.lines.len()
            );
            continue;
        }
        eprintln!("[SPEC_RECOGNITION]\nkind=None");

        if is_specification_start(trimmed) {
            if spec_capture.state == SpecificationCaptureState::Completed {
                eprintln!(
                    "[SPEC_BOUNDARY_TRACE]\nprevious_state={:?}\nreason=\"new_specification_detected\"",
                    spec_capture.state
                );
            }
            spec_capture.reset();
            eprintln!(
                "[SPEC_SESSION]\nsession={}\nstate={:?}\naction=create",
                spec_capture.session_id, spec_capture.state
            ); // TR-1
            eprintln!("[SPEC_END_TRACE] append_path_entered");
            eprintln!("[SPEC_END_TRACE] append_payload='{}'", trimmed);
            spec_capture.lines.push(input);
            eprintln!(
                "[SPEC_SESSION]\nsession={}\naction=append\nbuffer_lines={}",
                spec_capture.session_id,
                spec_capture.lines.len()
            ); // TR-2
            continue;
        }

        eprintln!(
            "[SPEC_STATE_TRACE] before_submit state={:?}",
            spec_capture.state
        );
        eprintln!("[UI] Input received");
        handle_submit(
            trimmed.to_string(),
            workspace_root.as_path(),
            core,
            &mut ui,
            writer,
        )?;
        writer.flush().map_err(|err| err.to_string())?;
    }

    // EOF handling: Dispatch any remaining capture session
    if spec_capture.state == SpecificationCaptureState::Capturing && !spec_capture.lines.is_empty()
    {
        let current_session_id = spec_capture.session_id.clone();
        let full_text = spec_capture.lines.join("\n");
        eprintln!(
            "[SPEC_SESSION]\nsession={}\naction=dispatch\npayload_lines={}\npayload_chars={}",
            current_session_id,
            spec_capture.lines.len(),
            full_text.len()
        );
        eprintln!(
            "[SPEC_DISPATCH]\nsession={}\npayload_lines={}\npayload_chars={}",
            current_session_id,
            spec_capture.lines.len(),
            full_text.len()
        );
        dispatch_captured_specification(
            &full_text,
            &current_session_id,
            writer,
            &mut pending_plan,
            &mut pending_specification,
        )?;
    }

    Ok(())
}

fn dispatch_captured_specification<W: Write>(
    full_text: &str,
    session_id: &str,
    writer: &mut W,
    pending_plan: &mut Option<InstructionPlan>,
    pending_specification: &mut Option<SpecificationContext>,
) -> Result<(), String> {
    match classify_specification(full_text) {
        SpecificationKind::Instruction | SpecificationKind::DraftSpecification => {
            eprintln!(
                "[SPEC_CLASSIFIER]\nkind={:?}",
                classify_specification(full_text)
            );
            let plan = InstructionPlan::from_spec(full_text);
            for line in plan.render_lines() {
                writeln!(writer, "{line}").map_err(|err| err.to_string())?;
            }

            let valid =
                plan.title.is_some() && plan.goal.is_some() && !plan.deliverables.is_empty();
            if valid {
                eprintln!("[SPEC_COHERENCE]\nsession={session_id}\nstatus=valid");
            } else {
                eprintln!(
                    "[SPEC_COHERENCE]\nsession={session_id}\nstatus=invalid\nreason=\"missing_required_fields\""
                );
            }

            *pending_plan = Some(plan);
        }
        SpecificationKind::DesignSpecification => {
            eprintln!("[SPEC_CLASSIFIER]\nkind=DesignSpecification");
            let context =
                SpecificationContext::from_yaml(full_text).map_err(|err| err.to_string())?;
            eprintln!(
                "[SPEC_CONTEXT]\ngoals={}\nconstraints={}\ncomponents={}\nrules={}",
                context.goals.len(),
                context.constraints.len(),
                context.architecture.len(),
                context.rules.len()
            );

            let request =
                crate::specification_bridge::StructuralDiagnosisRequest::new(context.clone());
            let diagnosis_label = request.domain.diagnosis_log_label();
            let result = request.diagnose();
            let repair_label = if request.domain == DiagnosisDomain::UserInterface {
                "UI_REPAIR_PLAN"
            } else {
                "REPAIR_PLANNING"
            };
            eprintln!("[{repair_label}]\nstatus=started");
            let repair_plan = RepairPlanner::generate(&result);
            for suggestion in &repair_plan.suggestions {
                eprintln!(
                    "[REPAIR_SUGGESTION]\ntitle=\"{}\"\npriority={}",
                    suggestion.title, suggestion.priority
                );
            }
            eprintln!(
                "[{repair_label}]\nsuggestions={}\nsteps={}",
                repair_plan.suggestions.len(),
                repair_plan.execution_steps.len()
            );
            if request.domain != DiagnosisDomain::UserInterface {
                eprintln!(
                    "[REPAIR_PLAN]\nsuggestions={}\nsteps={}",
                    repair_plan.suggestions.len(),
                    repair_plan.execution_steps.len()
                );
            }
            eprintln!("[{repair_label}]\nstatus=completed");
            let implementation_plan = ImplementationPlanner::generate(&repair_plan);
            let implementation_label = if request.domain == DiagnosisDomain::UserInterface {
                "UI_IMPLEMENTATION_PLAN"
            } else {
                "IMPLEMENTATION_PLAN"
            };

            writeln!(writer, "[SPEC_CONTEXT] generated").map_err(|err| err.to_string())?;
            writeln!(
                writer,
                "[{}] violations={} warnings={}",
                diagnosis_label,
                result.violations.len(),
                result.warnings.len()
            )
            .map_err(|err| err.to_string())?;
            writeln!(writer, "Violations:").map_err(|err| err.to_string())?;
            if result.violations.is_empty() {
                writeln!(writer, "- none").map_err(|err| err.to_string())?;
            } else {
                for violation in &result.violations {
                    writeln!(writer, "- {}: {}", violation.rule, violation.message)
                        .map_err(|err| err.to_string())?;
                }
            }
            writeln!(writer, "Warnings:").map_err(|err| err.to_string())?;
            if result.warnings.is_empty() {
                writeln!(writer, "- none").map_err(|err| err.to_string())?;
            } else {
                for warning in &result.warnings {
                    writeln!(writer, "- {}: {}", warning.rule, warning.message)
                        .map_err(|err| err.to_string())?;
                }
            }
            render_repair_plan_with_label(writer, &repair_plan, repair_label)?;
            render_implementation_plan_with_label(
                writer,
                &implementation_plan,
                implementation_label,
            )?;

            *pending_specification = Some(context);
        }
    }

    Ok(())
}

fn render_repair_plan_with_label<W: Write>(
    writer: &mut W,
    repair_plan: &RepairPlan,
    label: &str,
) -> Result<(), String> {
    if repair_plan.suggestions.is_empty() && repair_plan.execution_steps.is_empty() {
        return Ok(());
    }

    writeln!(writer).map_err(|err| err.to_string())?;
    writeln!(writer, "[{label}] generated").map_err(|err| err.to_string())?;
    writeln!(writer, "Repair Suggestions:").map_err(|err| err.to_string())?;
    for suggestion in &repair_plan.suggestions {
        writeln!(writer).map_err(|err| err.to_string())?;
        writeln!(writer, "[{}]", suggestion.priority).map_err(|err| err.to_string())?;
        writeln!(writer, "{}", suggestion.title).map_err(|err| err.to_string())?;
        writeln!(writer).map_err(|err| err.to_string())?;
        writeln!(writer, "Reason:").map_err(|err| err.to_string())?;
        writeln!(writer, "{}", suggestion.rationale).map_err(|err| err.to_string())?;
    }

    writeln!(writer).map_err(|err| err.to_string())?;
    writeln!(writer, "Steps:").map_err(|err| err.to_string())?;
    for step in &repair_plan.execution_steps {
        writeln!(writer, "{}. {}", step.order, step.description).map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn render_implementation_plan_with_label<W: Write>(
    writer: &mut W,
    implementation_plan: &ImplementationPlan,
    label: &str,
) -> Result<(), String> {
    if implementation_plan.tasks.is_empty()
        && implementation_plan.file_modifications.is_empty()
        && implementation_plan.validations.is_empty()
    {
        return Ok(());
    }

    writeln!(writer).map_err(|err| err.to_string())?;
    writeln!(writer, "[{label}] generated").map_err(|err| err.to_string())?;
    writeln!(writer, "Implementation Plan").map_err(|err| err.to_string())?;
    writeln!(writer).map_err(|err| err.to_string())?;

    writeln!(writer, "Tasks:").map_err(|err| err.to_string())?;
    for task in &implementation_plan.tasks {
        writeln!(writer, "- {}", task.title).map_err(|err| err.to_string())?;
    }

    writeln!(writer).map_err(|err| err.to_string())?;
    writeln!(writer, "Files:").map_err(|err| err.to_string())?;
    if implementation_plan.file_modifications.is_empty() {
        writeln!(writer, "- none").map_err(|err| err.to_string())?;
    } else {
        for file_plan in &implementation_plan.file_modifications {
            writeln!(writer, "- {}: {}", file_plan.target_file, file_plan.action)
                .map_err(|err| err.to_string())?;
        }
    }

    writeln!(writer).map_err(|err| err.to_string())?;
    writeln!(writer, "Validation:").map_err(|err| err.to_string())?;
    for validation in &implementation_plan.validations {
        writeln!(
            writer,
            "- {}: {}",
            validation.validation_type, validation.description
        )
        .map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn parse_repl_git_command(input: &str) -> Option<Vec<String>> {
    let mut parts = input.split_whitespace();
    if parts.next()? != "git" {
        return None;
    }
    let args = parts.map(ToOwned::to_owned).collect::<Vec<_>>();
    if args.is_empty() { None } else { Some(args) }
}

fn parse_repl_memory_log_command(input: &str) -> Option<Vec<String>> {
    let rest = input.strip_prefix(":memory log")?.trim();
    let mut args = vec!["log".to_string()];
    if rest.is_empty() {
        return Some(args);
    }
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["recent", n] => {
            args.extend(["--recent".to_string(), (*n).to_string()]);
        }
        ["duplicates"] => args.push("--duplicates".to_string()),
        ["conflicts"] => args.push("--conflicts".to_string()),
        ["class", class] => {
            args.extend(["--class".to_string(), (*class).to_string()]);
        }
        ["json"] => args.push("--json".to_string()),
        [memory_id] => args.extend(["--memory-id".to_string(), (*memory_id).to_string()]),
        _ => {
            for part in parts {
                args.push(part.to_string());
            }
        }
    }
    Some(args)
}

fn promote_pending_plan<W: Write>(
    ui: &mut ReplUiState,
    workspace_root: &Path,
    pending_plan: Option<&InstructionPlan>,
    writer: &mut W,
) -> Result<(), String> {
    let Some(plan) = pending_plan else {
        writeln!(writer, "[PROMOTE] rejected: no pending plan").map_err(|err| err.to_string())?;
        return Ok(());
    };
    let Some(target) = plan.target.clone() else {
        writeln!(writer, "[PROMOTE] rejected: no target").map_err(|err| err.to_string())?;
        return Ok(());
    };
    let target_path = workspace_root.join(&target);
    if !target_path.exists() {
        writeln!(writer, "[PROMOTE] rejected: target missing").map_err(|err| err.to_string())?;
        return Ok(());
    }

    let comment = render_plan_comment(plan);
    if let Some(pattern) = unsafe_generated_marker_pattern(&comment) {
        writeln!(
            writer,
            "[PROMOTE] rejected: unsafe generated pattern: {pattern}"
        )
        .map_err(|err| err.to_string())?;
        return Ok(());
    }

    let target_label = target_path.display().to_string();
    let resolved_target = ResolvedExecutionTarget::from_canonical_path(&target_label);
    let diff = crate::tui::state::Diff {
        file: resolved_target.canonical_target.path.clone(),
        changes: vec![crate::tui::state::DiffChunk {
            old_line: None,
            new_line: Some(1),
            old: None,
            new: Some(comment.clone()),
        }],
    };
    if let Some(pattern) = unsafe_generated_marker_pattern(&diff_text(&diff)) {
        writeln!(
            writer,
            "[PROMOTE] rejected: unsafe generated pattern: {pattern}"
        )
        .map_err(|err| err.to_string())?;
        return Ok(());
    }

    commit_preview_candidate(
        &mut ui.runtime,
        PreviewCandidate {
            target_path: resolved_target.canonical_target.path.clone(),
            tx_id: format!(
                "tx-promoted-plan-{}",
                stable_plan_suffix(&target.display().to_string())
            ),
            resolved_target,
            diff,
        },
    );
    ui.runtime.promoted_plan = Some(plan.clone());
    ui.runtime.apply_guard = Some(crate::tui::state::ApplyGuardState {
        transaction_id: ui.runtime.active_transaction_id.clone(),
        target: Some(target.clone()),
        source: Some(crate::tui::state::ApplyGuardSource::PromotedPlan),
    });
    writeln!(writer, "[PROMOTE] preview: {}", target.display()).map_err(|err| err.to_string())
}

fn render_plan_comment(plan: &InstructionPlan) -> String {
    let text = plan
        .operations
        .iter()
        .find_map(|op| op.strip_prefix("InsertComment: "))
        .unwrap_or(plan.summary.as_str())
        .trim();
    format!("// {text}")
}

fn diff_text(diff: &crate::tui::state::Diff) -> String {
    diff.changes
        .iter()
        .filter_map(|chunk| chunk.new.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
}

fn unsafe_generated_marker_pattern(text: &str) -> Option<&'static str> {
    [
        "REPL_RUNTIME_TEST",
        "validate_runtime",
        "test_marker",
        "runtime marker",
        "dummy function",
        "#[allow(dead_code)]",
    ]
    .into_iter()
    .find(|pattern| text.contains(pattern))
}

fn stable_plan_suffix(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_lowercase()
}

fn validate_last_applied_plan<W: Write>(
    ui: &ReplUiState,
    workspace_root: &Path,
    writer: &mut W,
) -> Result<(), String> {
    let Some(plan) = ui.runtime.last_applied_plan.as_ref() else {
        writeln!(writer, "[VALIDATE] rejected: no applied plan").map_err(|err| err.to_string())?;
        return Ok(());
    };
    if plan.validation_plan.is_empty() {
        writeln!(writer, "[VALIDATE] skipped: no validation plan")
            .map_err(|err| err.to_string())?;
        return Ok(());
    }
    for line in &plan.validation_plan {
        let command = line
            .split_once(':')
            .map(|(_, rest)| rest.trim())
            .unwrap_or(line.trim());
        if !is_allowed_validation_command(command) {
            writeln!(writer, "[VALIDATE] rejected: unsafe validation command")
                .map_err(|err| err.to_string())?;
            return Ok(());
        }
        writeln!(writer, "[VALIDATE] running: {command}").map_err(|err| err.to_string())?;
        let mut parts = command.split_whitespace();
        let Some(program) = parts.next() else {
            continue;
        };
        let status = std::process::Command::new(program)
            .args(parts)
            .current_dir(workspace_root)
            .status()
            .map_err(|err| err.to_string())?;
        if !status.success() {
            writeln!(writer, "[VALIDATE] failed: {command}").map_err(|err| err.to_string())?;
            return Ok(());
        }
    }
    writeln!(writer, "[VALIDATE] ok").map_err(|err| err.to_string())
}

fn is_allowed_validation_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    let safe_prefix = lower.starts_with("cargo test")
        || lower.starts_with("cargo check")
        || lower.starts_with("cargo clippy")
        || lower.starts_with("cargo fmt");
    safe_prefix
        && ![
            "&&", "||", ";", "|", ">", "<", "`", "$(", " rm ", " rm\n", "rm -",
        ]
        .iter()
        .any(|token| lower.contains(token))
}

pub fn run_repl_stdio(workspace_root: PathBuf) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    run_repl(workspace_root, &mut reader, &mut writer)
}

pub fn dispatch_repl_input<W: Write>(
    input: &str,
    session: &mut AgentSession,
    _conversation: &mut crate::nl::session::ConversationState,
    _mode: &mut crate::planner::PlannerMode,
    writer: &mut W,
) -> Result<bool, String> {
    let core = RuntimeCoreBridge::with_defaults();
    dispatch_repl_input_with_core(input, session, &core, writer)
}

pub fn dispatch_repl_input_with_core<W: Write>(
    input: &str,
    session: &mut AgentSession,
    core: &dyn CoreExecutor,
    writer: &mut W,
) -> Result<bool, String> {
    let trimmed = input.trim();
    if is_exit(trimmed) {
        return Ok(true);
    }

    let workspace_root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let mut ui = ReplUiState::default();
    if trimmed == "/save design" {
        save_design(
            workspace_root.as_path(),
            ui.core_snapshot.design.as_ref(),
            writer,
        )?;
        return Ok(false);
    }

    if let Some(pending) = session.pending_confirmation.clone() {
        match confirmation_response(trimmed, pending.action) {
            ConfirmationResponse::Execute(action) => {
                session.pending_confirmation = None;
                let resolved = session
                    .pending_resolved_intent
                    .take()
                    .unwrap_or_else(|| IntentResolutionEngine::resolve(trimmed));
                dispatch_repl_action(
                    action,
                    trimmed,
                    resolved,
                    core,
                    workspace_root.as_path(),
                    &mut ui,
                    writer,
                )?;
                return Ok(false);
            }
            ConfirmationResponse::Cancel => {
                session.pending_confirmation = None;
                session.pending_resolved_intent = None;
                writeln!(writer, "実行をキャンセルしました。").map_err(|err| err.to_string())?;
                return Ok(false);
            }
            ConfirmationResponse::Unmatched => {}
        }
    }

    let resolved = IntentResolutionEngine::resolve(trimmed);
    if should_intercept_with_intent_layer(&resolved) {
        if resolved.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD {
            writeln!(writer, "{}", ConfirmationEngine::prompt(&resolved))
                .map_err(|err| err.to_string())?;
            return Ok(false);
        }
        if let Some(pending) = ConfirmationEngine::pending(&resolved) {
            writeln!(writer, "{}", pending.summary).map_err(|err| err.to_string())?;
            session.pending_confirmation = Some(pending);
            session.pending_resolved_intent = Some(resolved);
            return Ok(false);
        }
        if resolved.recommended_action() == RecommendedAction::RunAnalyze {
            writeln!(writer, "{}", ConfirmationEngine::prompt(&resolved))
                .map_err(|err| err.to_string())?;
            dispatch_repl_action(
                RecommendedAction::RunAnalyze,
                trimmed,
                resolved,
                core,
                workspace_root.as_path(),
                &mut ui,
                writer,
            )?;
            return Ok(false);
        }
    }

    if let Some(args) = parse_repl_memory_log_command(trimmed) {
        match crate::commands::memory::dispatch_memory_command(&args) {
            Ok(out) => writeln!(writer, "{}", out.message).map_err(|err| err.to_string())?,
            Err(err) => writeln!(writer, "[ERROR] {err}").map_err(|err| err.to_string())?,
        }
        return Ok(false);
    }

    if let Some(events) =
        RuntimeCommandDispatcher::dispatch(&mut ui.runtime, workspace_root.as_path(), trimmed)
    {
        ui.semantic_state
            .capture_runtime_command(trimmed, &ui.runtime);
        for event in events {
            writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
        }
        return Ok(false);
    }

    if should_try_runtime_intent(trimmed)
        && let Some(events) =
            dispatch_normalized_runtime_intent(&mut ui, workspace_root.as_path(), trimmed)
    {
        for event in events {
            writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
        }
        return Ok(false);
    }

    eprintln!("[UI] Input received");
    handle_submit(
        trimmed.to_string(),
        workspace_root.as_path(),
        core,
        &mut ui,
        writer,
    )?;
    Ok(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmationResponse {
    Execute(RecommendedAction),
    Cancel,
    Unmatched,
}

fn confirmation_response(input: &str, pending: RecommendedAction) -> ConfirmationResponse {
    match input.to_ascii_lowercase().as_str() {
        "y" | "yes" | "はい" | "実行" => ConfirmationResponse::Execute(pending),
        "n" | "no" | "いいえ" | "cancel" | "キャンセル" | "中止" => {
            ConfirmationResponse::Cancel
        }
        "p" | "preview" | "プレビュー" if pending == RecommendedAction::RunMutationApply => {
            ConfirmationResponse::Execute(RecommendedAction::RunMutationPreview)
        }
        _ => ConfirmationResponse::Unmatched,
    }
}

fn should_intercept_with_intent_layer(resolved: &crate::intent_resolution::ResolvedIntent) -> bool {
    resolved.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD
        || resolved.requires_confirmation
        || resolved.recommended_action() == RecommendedAction::RunAnalyze
}

fn dispatch_repl_action<W: Write>(
    action: RecommendedAction,
    user_input: &str,
    resolved_intent: ResolvedIntent,
    core: &dyn CoreExecutor,
    workspace_root: &Path,
    ui: &mut ReplUiState,
    writer: &mut W,
) -> Result<(), String> {
    let context = ExecutionContext {
        user_input: user_input.to_string(),
        resolved_intent,
        workspace_path: workspace_root.to_path_buf(),
    };
    let result = ExecutionRouter::execute(action, &context);
    writeln!(writer, "{}", result.narrative).map_err(|err| err.to_string())?;
    let Some(runtime_input) = ExecutionRouter::route_with_context(action, &context) else {
        return Ok(());
    };
    if let Some(events) =
        RuntimeCommandDispatcher::dispatch(&mut ui.runtime, workspace_root, &runtime_input)
    {
        ui.semantic_state.capture_mutation_events(&events);
        ui.semantic_state
            .capture_runtime_command(&runtime_input, &ui.runtime);
        for event in events {
            writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
        }
        return Ok(());
    }
    handle_submit(runtime_input.to_string(), workspace_root, core, ui, writer)?;
    Ok(())
}

fn dispatch_normalized_runtime_intent(
    ui: &mut ReplUiState,
    workspace_root: &Path,
    input: &str,
) -> Option<Vec<crate::tui::state::RuntimeNarrativeEvent>> {
    let normalized = normalize_runtime_input(input)?;
    eprintln!(
        "[REPL][PRECORE_CERTAINTY] source={:?} certainty={:?}",
        normalized.source, normalized.certainty
    );
    if !should_handle_in_precore(input, &normalized) {
        eprintln!("[REPL][PRECORE_FALLTHROUGH] reason=NotExplicitRuntimeCommand");
        return None;
    }
    if matches!(
        normalized.rejection,
        Some(RuntimeNormalizationRejection::UnresolvedTarget)
    ) && confirmation_like_target_failure(input).is_some()
    {
        return None;
    }
    match normalized.command.intent {
        RuntimeIntent::Preview => {
            let Some(target) = normalized.command.target else {
                eprintln!("[REPL][PRECORE_REJECTED] reason=ExplicitCommandMissingTarget");
                ui.runtime.rejection = Some(crate::tui::state::RejectionInfo {
                    reason: "unresolved target".to_string(),
                    originating_mutation: "runtime_intent_bridge".to_string(),
                    governance_source: None,
                    convergence_source: None,
                });
                return Some(vec![crate::tui::state::RuntimeNarrativeEvent::Error {
                    message: "unresolved target".to_string(),
                }]);
            };

            let target_label = target.display().to_string();
            let Ok(authority_target) = RuntimeAuthorityTarget::new(target, workspace_root) else {
                ui.runtime.rejection = Some(crate::tui::state::RejectionInfo {
                    reason: "unresolved target".to_string(),
                    originating_mutation: "runtime_intent_bridge".to_string(),
                    governance_source: None,
                    convergence_source: None,
                });
                return Some(vec![crate::tui::state::RuntimeNarrativeEvent::Error {
                    message: "unresolved target".to_string(),
                }]);
            };
            let mut events =
                runtime_preview_from_intent(&mut ui.runtime, workspace_root, authority_target);
            if ui.runtime.active_transaction.is_some() {
                events.insert(
                    0,
                    crate::tui::state::RuntimeNarrativeEvent::System {
                        summary: format!("Target: {target_label}"),
                        target: Some(target_label.clone()),
                    },
                );
            }
            ui.semantic_state
                .capture_runtime_command("preview", &ui.runtime);
            Some(events)
        }
        RuntimeIntent::MutationPlan
        | RuntimeIntent::MutationPreview
        | RuntimeIntent::MutationApply
        | RuntimeIntent::MutationReplay
        | RuntimeIntent::MutationRollback => {
            let runtime_input = match normalized.command.intent {
                RuntimeIntent::MutationPlan => normalized.command.to_runtime_input(),
                intent => {
                    let mutation_id = normalized
                        .command
                        .target
                        .as_ref()
                        .map(|target| target.display().to_string())
                        .or_else(|| ui.semantic_state.last_mutation_id.clone());
                    let Some(mutation_id) = mutation_id else {
                        return Some(vec![crate::tui::state::RuntimeNarrativeEvent::Error {
                            message: "unresolved mutation id".to_string(),
                        }]);
                    };
                    mutation_runtime_input(intent, &mutation_id)
                }
            };
            let Some(events) =
                RuntimeCommandDispatcher::dispatch(&mut ui.runtime, workspace_root, &runtime_input)
            else {
                let argument = match normalized.command.intent {
                    RuntimeIntent::MutationPlan => "mutation target",
                    _ => "mutation id",
                };
                return Some(vec![crate::tui::state::RuntimeNarrativeEvent::Error {
                    message: format!("unresolved {argument}"),
                }]);
            };
            ui.semantic_state.capture_mutation_events(&events);
            ui.semantic_state
                .capture_runtime_command(&runtime_input, &ui.runtime);
            Some(events)
        }
        _ => None,
    }
}

fn mutation_runtime_input(intent: RuntimeIntent, mutation_id: &str) -> String {
    match intent {
        RuntimeIntent::MutationPreview => format!("mutation preview {mutation_id}"),
        RuntimeIntent::MutationApply => format!("mutation apply {mutation_id}"),
        RuntimeIntent::MutationReplay => format!("mutation replay {mutation_id}"),
        RuntimeIntent::MutationRollback => format!("mutation rollback {mutation_id}"),
        _ => unreachable!("mutation id routing only supports post-plan intents"),
    }
}

fn should_handle_in_precore(_input: &str, normalized: &NormalizedRuntimeInput) -> bool {
    matches!(
        normalized.command.intent,
        RuntimeIntent::MutationPlan
            | RuntimeIntent::MutationPreview
            | RuntimeIntent::MutationApply
            | RuntimeIntent::MutationReplay
            | RuntimeIntent::MutationRollback
    ) || (normalized.source == RuntimeInputSource::ExplicitCommand
        && normalized.certainty == RuntimeCommandCertainty::Certain)
}

fn should_try_runtime_intent(input: &str) -> bool {
    if normalize_runtime_input(input).is_some_and(|normalized| {
        matches!(
            normalized.command.intent,
            RuntimeIntent::MutationPlan
                | RuntimeIntent::MutationPreview
                | RuntimeIntent::MutationApply
                | RuntimeIntent::MutationReplay
                | RuntimeIntent::MutationRollback
        )
    }) {
        return true;
    }
    let lower = input.to_lowercase();
    !crate::nl::context_aware_plan_target_resolver::is_plan_only_intent(&lower)
        && !crate::nl::context_aware_plan_target_resolver::has_context_reference(&lower)
}

impl ReplSemanticState {
    fn capture_mutation_events(&mut self, events: &[crate::tui::state::RuntimeNarrativeEvent]) {
        for event in events {
            let mutation_id = match event {
                crate::tui::state::RuntimeNarrativeEvent::MutationPlan { projection }
                | crate::tui::state::RuntimeNarrativeEvent::MutationApplied { projection } => {
                    Some(projection.mutation_id.as_str())
                }
                crate::tui::state::RuntimeNarrativeEvent::MutationPreview { projection } => {
                    Some(projection.mutation_id.as_str())
                }
                crate::tui::state::RuntimeNarrativeEvent::MutationReplay { projection } => {
                    Some(projection.mutation_id.as_str())
                }
                crate::tui::state::RuntimeNarrativeEvent::MutationRollback { projection } => {
                    Some(projection.mutation_id.as_str())
                }
                _ => None,
            };
            if let Some(mutation_id) = mutation_id {
                self.last_mutation_id = Some(mutation_id.to_string());
            }
        }
    }

    fn capture_runtime_command(&mut self, input: &str, runtime: &TuiState) {
        let snapshot = RenderSnapshot::from(runtime).projection;
        let rendered_output = runtime
            .active_transaction
            .as_ref()
            .map(|_| snapshot.narrative.lines.clone())
            .unwrap_or_default();
        match input.split_whitespace().next().unwrap_or_default() {
            "preview" => {
                self.last_preview = Some(PreviewState {
                    projection: snapshot.clone(),
                    rendered_output,
                });
                self.last_validation = Some(ValidationState {
                    projection_hash: snapshot.projection_hash.semantic_hash.clone(),
                    valid: runtime.rejection.is_none(),
                });
                self.rollback_checkpoint = Some(RollbackCheckpoint {
                    projection_before: snapshot.clone(),
                    projection_after: snapshot,
                });
            }
            "apply" | "commit" => {
                self.last_apply = Some(ApplyState {
                    projection: snapshot.clone(),
                });
                self.last_validation = Some(ValidationState {
                    projection_hash: snapshot.projection_hash.semantic_hash.clone(),
                    valid: runtime.rejection.is_none(),
                });
            }
            "rollback" => {
                let before = self
                    .rollback_checkpoint
                    .as_ref()
                    .map(|checkpoint| checkpoint.projection_before.clone())
                    .or_else(|| {
                        self.last_preview
                            .as_ref()
                            .map(|preview| preview.projection.clone())
                    })
                    .unwrap_or_else(|| snapshot.clone());
                self.rollback_checkpoint = Some(RollbackCheckpoint {
                    projection_before: before,
                    projection_after: snapshot,
                });
            }
            _ => {}
        }
    }
}

pub fn reset_review_session(view: &mut ComposerViewState, session: &mut AgentSession) {
    view.reset_review_session();
    view.state = State::Idle;
    session.current_plan = None;
    session.state = State::Idle;
}

fn handle_submit<W: Write>(
    input: String,
    _working_dir: &Path,
    core: &dyn CoreExecutor,
    ui: &mut ReplUiState,
    writer: &mut W,
) -> Result<(), String> {
    // Phase 4.5: build CoreRequest (pass-through).
    let request = CoreRequest::new(input);
    let response = core.execute(request);
    let success = response.status != crate::core::ExecutionStatus::Failed;

    // Phase 4.5: sync core_snapshot from response before rendering events.
    if let Some(snapshot) = response.core_state {
        ui.core_snapshot = snapshot;
    } else if success && let Some(design) = response.design.as_ref() {
        ui.core_snapshot.design = Some(design.clone());
    }

    for event in response.events {
        eprintln!("[UI] Rendering event");
        render_core_event(writer, event)?;
    }

    Ok(())
}

fn render_core_event<W: Write>(writer: &mut W, event: CoreEvent) -> Result<(), String> {
    let event = to_ui_event(event);
    for line in event.lines() {
        writeln!(writer, "{line}").map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn print_banner<W: Write>(writer: &mut W) -> Result<(), String> {
    writeln!(writer, "DBM_CLI REPL").map_err(|err| err.to_string())?;
    writeln!(
        writer,
        "Type /exit to quit. Use select <n>, y/n, cancel, /save design."
    )
    .map_err(|err| err.to_string())
}

fn is_exit(input: &str) -> bool {
    matches!(input, "/exit" | "/quit" | "exit" | "quit")
}

fn is_end_command(input: &str) -> bool {
    input.trim().eq_ignore_ascii_case("/end")
}

fn is_specification_start(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.starts_with("dbm-")
        || lower.starts_with("# ")
        || lower.starts_with("goal:")
        || lower.starts_with("deliverables:")
        || lower.starts_with("constraints:")
        || lower.starts_with("success criteria:")
        || lower.starts_with("assumptions:")
}

fn extract_contamination_token(line: &str) -> String {
    for word in line.split_whitespace() {
        if word.contains("DBM-") && !word.starts_with("DBM-") {
            return word.to_string();
        }
    }
    String::new()
}

fn save_design<W: Write>(
    workspace_root: &Path,
    design: Option<&DesignDocument>,
    writer: &mut W,
) -> Result<(), String> {
    let path = workspace_root.join("dbm_design.md");
    let content = design
        .map(|doc| doc.rendered.join("\n"))
        .unwrap_or_else(|| "[DESIGN]\nNo design snapshot available.".to_string());
    std::fs::write(&path, content).map_err(|err| err.to_string())?;
    writeln!(writer, "[RESULT] Design saved: {}", path.display()).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CoreResponse, ExecutionStatus};
    use crate::nl::session::ConversationState;
    use crate::planner::PlannerMode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingCore {
        calls: AtomicUsize,
    }

    impl CountingCore {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl CoreExecutor for CountingCore {
        fn execute(&self, _request: CoreRequest) -> CoreResponse {
            self.calls.fetch_add(1, Ordering::SeqCst);
            CoreResponse {
                events: vec![CoreEvent::Proposal { candidates: vec![] }],
                status: ExecutionStatus::Proposed,
                design: None,
                core_state: None,
            }
        }
    }

    use crate::test_support::CurrentDirGuard;

    fn run_repl_with_core_in_workspace<R, W>(
        workspace_root: PathBuf,
        reader: &mut R,
        writer: &mut W,
        core: &dyn CoreExecutor,
    ) -> Result<(), String>
    where
        R: BufRead,
        W: Write,
    {
        let _guard = CurrentDirGuard::enter(&workspace_root);
        run_repl_with_core(workspace_root, reader, writer, core)
    }

    fn run_preview_confirmation_script(confirm_input: &str) -> String {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let script = format!(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\nselect 1\n{confirm_input}\n"
        );
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        String::from_utf8(output).expect("utf8")
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_routes_ambiguous_input_to_core_proposal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        let mut mode = PlannerMode::default();
        let mut output = Vec::new();

        let should_exit = dispatch_repl_input(
            "fix parser bug",
            &mut session,
            &mut conversation,
            &mut mode,
            &mut output,
        )
        .expect("dispatch");

        let output = String::from_utf8(output).expect("utf8");
        assert!(!should_exit);
        assert!(output.contains("[PROPOSAL]"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_exit_returns_true() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        let mut mode = PlannerMode::default();
        let mut output = Vec::new();

        let should_exit = dispatch_repl_input(
            "/exit",
            &mut session,
            &mut conversation,
            &mut mode,
            &mut output,
        )
        .expect("dispatch");

        assert!(should_exit);
        assert!(output.is_empty());
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_n_cancels_without_clarification() {
        let output = run_preview_confirmation_script("n");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reject"),
            "{output}"
        );
        assert!(
            output.contains("[RESULT] Preview cancelled. No files modified."),
            "{output}"
        );
        assert!(!output.contains("ClarificationRequired"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_multiline_spec_dispatches_on_eof() {
        let temp = tempfile::tempdir().expect("tempdir");
        // No /end here
        let script = "/begin spec\nDBM-A\nGoal: X\nDeliverables: - Y\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 0, "Should capture locally on EOF");
        let output_str = String::from_utf8(output).expect("utf8");
        // Note: [SPEC_SESSION] and [SPEC_COHERENCE] go to stderr, but [SPEC_EXTRACT] goes to stdout (writer)
        assert!(
            output_str.contains("[SPEC_EXTRACT] title="),
            "Should have extraction logs in stdout"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_multiline_spec_with_end_renders_plan_but_no_immediate_dispatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/begin spec\nDBM-A\nGoal: X\nDeliverables: - Y\n/end\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(
            core.calls(),
            0,
            "Should not dispatch to core immediately on /end"
        );
        let output_str = String::from_utf8(output).expect("utf8");
        assert!(
            output_str.contains("[PLAN] summary:"),
            "Should render the plan"
        );
        assert!(
            output_str.contains("[SPEC_EXTRACT] title=Some(\"DBM-A\")"),
            "Should have title extraction"
        );
        assert!(
            output_str.contains("[SPEC_EXTRACT] deliverables=1"),
            "Should have deliverables count"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_reject_then_undo_does_not_return_previewed() {
        let output = run_preview_confirmation_script("n\nundo");

        assert!(
            output.contains("[IR-TRACE][ROLLBACK_STATE_CHECK]"),
            "{output}"
        );
        assert!(
            !output
                .lines()
                .skip_while(|line| !line.contains("[RESULT] Undo to v"))
                .any(|line| line.contains("[PIPELINE] Previewed")),
            "{output}"
        );
        assert!(
            !output
                .lines()
                .skip_while(|line| !line.contains("[RESULT] Undo to v"))
                .any(|line| line.trim() == "y" || line.trim() == "n"),
            "{output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_y_without_validated_plan_rejects_apply() {
        let output = run_preview_confirmation_script("y");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Confirm"),
            "{output}"
        );
        assert!(
            output.contains("[IR-TRACE][APPLY_GUARD] rejected=true reason=MissingValidatedPlan"),
            "{output}"
        );
        assert!(output.contains("[RESULT] # Apply Rejected"), "{output}");
        assert!(output.contains("No files modified."), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_cancel_clears_selection() {
        let output = run_preview_confirmation_script("cancel");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Cancel"),
            "{output}"
        );
        assert!(
            output.contains("[RESULT] Preview cancelled. No files modified."),
            "{output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_confirmation_does_not_emit_unknown_intent() {
        let output = run_preview_confirmation_script("abc");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reconfirm"),
            "{output}"
        );
        assert!(!output.contains("intent=Unknown"), "{output}");
        assert!(!output.contains("ClarificationRequired"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_empty_input_reconfirms() {
        let output = run_preview_confirmation_script("");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reconfirm"),
            "{output}"
        );
        assert!(
            output.contains("[RESULT] Please confirm: y / n / cancel"),
            "{output}"
        );
        assert!(!output.contains("ClarificationRequired"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_preview_unknown_input_reconfirms() {
        let output = run_preview_confirmation_script("maybe");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reconfirm"),
            "{output}"
        );
        assert!(
            output.contains("[RESULT] Please confirm: y / n / cancel"),
            "{output}"
        );
        assert!(output.contains("[PIPELINE] Previewed"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn rollback_bypasses_executor_and_clears_runtime_projection() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\nrollback\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("runtime idle"), "{output}");
        assert!(output.contains("no active transaction"), "{output}");
        assert!(output.contains("transaction reverted"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn preview_short_circuits_executor() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("preview ready"), "{output}");
        assert!(output.contains("transaction active"), "{output}");
        assert!(!output.contains("[PROPOSAL]"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn preview_dispatch_terminates_pipeline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("preview ready"), "{output}");
        assert!(output.contains("transaction active"), "{output}");
        assert!(!output.contains("[PROPOSAL]"), "{output}");
        assert!(!output.contains("[RESULT]"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_explicit_runtime_preview_with_target_works() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/test_runtime.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn generated() {}\n").expect("write");
        let mut input = io::Cursor::new("runtime preview apps/cli/src/test_runtime.rs\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("preview ready"), "{output}");
        assert!(output.contains("transaction active"), "{output}");
        assert!(
            output.contains("Target: apps/cli/src/test_runtime.rs"),
            "{output}"
        );
        assert!(!output.contains("Target: (none)"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_natural_language_preview_falls_through() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("修正してください\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 1);
        assert!(!output.contains("[ERROR] unresolved target"), "{output}");
        assert!(!output.contains("preview ready"), "{output}");
        assert!(!output.contains("transaction active"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_long_analyze_project_falls_through() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new(
            "このプロジェクト全体の構造を解析してください。まだ修正、apply、git操作、外部コマンド実行は行わないでください。\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(!output.contains("[ERROR] unresolved target"), "{output}");
        assert!(output.contains("# Project Structure Analysis"), "{output}");
        assert!(
            output.contains("[IR-TRACE][CONTEXT_STORE] kind=analysis action=AnalyzeProject target=WorkspaceRoot mode=ReadOnly"),
            "{output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_review_safety_falls_through() {
        assert_precore_falls_through(
            "この変更案の安全性レビューをしてください。まだapplyしないでください。",
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_validate_plan_falls_through() {
        assert_precore_falls_through(
            "この修正プランを検証してください。外部コマンド実行は行わないでください。",
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_generate_change_plan_falls_through() {
        assert_precore_falls_through(
            "このプロジェクト向けの修正プランを生成してください。git操作しないでください。",
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_yes_no_reference_falls_through() {
        assert_precore_falls_through(
            "yes/no や y/n は確認トークンの参照文字列として扱い、構造を解析してください。",
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_explicit_preview_without_target_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("preview\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("[ERROR] unresolved target"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_explicit_preview_with_target_works() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("preview ready"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_routes_natural_language_mutation_plan_to_dispatcher() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/lib.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "pub fn before() {}\n").expect("write");
        let mut input =
            io::Cursor::new("src/lib.rs を分割してください。Mutation Plan を作成してください。\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0, "{output}");
        assert!(output.contains("Mutation Plan"), "{output}");
        assert!(output.contains("Target: src/lib.rs"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_handles_natural_language_mutation_preview_without_core_fallthrough() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/lib.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "pub fn before() {}\n").expect("write");
        let mut input = io::Cursor::new(
            "src/lib.rs を分割してください。Mutation Plan を作成してください。\n変更内容を確認してください。\n",
        );
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0, "{output}");
        assert!(output.contains("Mutation Preview"), "{output}");
        assert!(!output.contains("unresolved mutation id"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_handles_natural_language_mutation_apply_without_core_fallthrough() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/lib.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "pub fn before() {}\n").expect("write");
        let mut input = io::Cursor::new(
            "src/lib.rs を分割してください。Mutation Plan を作成してください。\n変更を適用してください。\n",
        );
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0, "{output}");
        assert!(output.contains("Mutation Plan"), "{output}");
        assert!(
            output.contains("explicit confirmation is required"),
            "{output}"
        );
        assert!(!output.contains("unresolved mutation id"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_precore_confirmation_exact_token_still_works() {
        let output = run_preview_confirmation_script("y");

        assert!(
            output.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Confirm"),
            "{output}"
        );
        assert!(!output.contains("ClarificationRequired"), "{output}");
    }

    fn assert_precore_falls_through(input: &str) {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new(format!("{input}\n"));
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 1);
        assert!(!output.contains("[ERROR] unresolved target"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_two_turn_analysis_then_plan_does_not_unresolved_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let mut input = io::Cursor::new(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(!output.contains("[ERROR] unresolved target"), "{output}");
        assert!(
            output.contains("[IR-TRACE][CONTEXT_STORE] kind=analysis"),
            "{output}"
        );
        assert!(
            output.contains("[IR-TRACE][CONTEXT_LOAD]")
                && output.contains("previous_analysis_context=Some"),
            "{output}"
        );
        assert!(
            output.contains("[IR-TRACE][CONTEXT_RESOLUTION]")
                && output.contains("previous_context_used=true"),
            "{output}"
        );
        assert!(output.contains("target=WorkspaceRoot"), "{output}");
        assert!(output.contains("mode=PlanOnly"), "{output}");
        assert!(output.contains("# Change Plan"), "{output}");
        assert!(!output.contains("[APPLYING]"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_plan_outputs_narrow_candidates() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let mut input = io::Cursor::new(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("## Candidates"), "{output}");
        assert!(output.contains("Target: File("), "{output}");
        assert!(output.contains("Validation required: yes"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_select_candidate_stores_selection_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let mut input = io::Cursor::new(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\nselect 1\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(
            output.contains("[IR-TRACE][CONTEXT_STORE] kind=selection candidate_id=1 target=File("),
            "{output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_validate_selected_candidate_stores_validated_plan() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let mut input = io::Cursor::new(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\nselect 1\nこの候補を検証して\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("# Plan Validation"), "{output}");
        assert!(output.contains("Apply allowed: true"), "{output}");
        assert!(
            output.contains("[IR-TRACE][CONTEXT_STORE] kind=validated_plan apply_allowed=true"),
            "{output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_apply_without_validation_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let mut input = io::Cursor::new(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\nselect 1\n問題なければ適用して\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("# Apply Rejected"), "{output}");
        assert!(output.contains("MissingValidatedPlan"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_plan_validate_apply_happy_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("write Cargo.toml");
        let mut input = io::Cursor::new(
            "このプロジェクトの構造を解析して\nこのプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで\nselect 1\nこの候補を検証して\n問題なければ適用して\n",
        );
        let mut output = Vec::new();
        let core = RuntimeCoreBridge::with_defaults();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("Target: File("), "{output}");
        assert!(output.contains("# Plan Validation"), "{output}");
        assert!(
            output.contains("[IR-TRACE][APPLY_GUARD] rejected=false"),
            "{output}"
        );
        assert!(
            !output.contains("[IR-TRACE][APPLY_GUARD] rejected=true"),
            "{output}"
        );
        assert!(!output.contains("git add"), "{output}");
        assert!(!output.contains("git commit"), "{output}");
        assert!(!output.contains("git push"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn invalid_preview_preserves_previous_repl_projection() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input =
            io::Cursor::new("preview apps/cli/src/core.rs\npreview does/not/exist.rs\nstatus\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");
        let status_lines = output
            .lines()
            .filter(|line| line.contains("preview ready"))
            .collect::<Vec<_>>();
        let unique_status_lines = status_lines
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(core.calls(), 0);
        assert!(!output.contains("does/not/exist.rs"), "{output}");
        assert!(status_lines.len() >= 3, "{output}");
        assert_eq!(unique_status_lines.len(), 1, "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn runtime_commands_bypass_reasoning_pipeline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("status\nrollback\napply\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 0);
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn unsafe_generated_marker_pattern_is_rejected() {
        assert_eq!(
            unsafe_generated_marker_pattern(
                "#[allow(dead_code)]\nconst REPL_RUNTIME_TEST: &str = \"x\";"
            ),
            Some("REPL_RUNTIME_TEST")
        );
        assert_eq!(
            unsafe_generated_marker_pattern("fn validate_runtime() -> bool { true }"),
            Some("validate_runtime")
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn apply_success_does_not_emit_no_active_transaction() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/coding.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "pub fn code() -> i32 { 0 }\n").expect("write target");
        let mut input = io::Cursor::new(
            "/begin spec\nTarget: src/coding.rs\nコメントを追加する。\n/end\npromote\napply\n",
        );
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(
            output.contains("transaction committed successfully"),
            "{output}"
        );
        assert!(!output.contains("no active transaction"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn non_runtime_input_still_routes_to_core() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("fix parser bug\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 1);
    }

    // ── DBM-SPECIFICATION-MULTILINE-CAPTURE-VALIDATION-SPEC v1.0 tests ────────

    // CATEGORY: REPL_ROUTING
    #[test]
    fn spec_begin_spec_does_not_route_to_core() {
        // §2: /begin spec enters capture mode; body lines must NOT be sent to the core.
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/lib.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "pub fn lib() {}\n").expect("write");
        let mut input = io::Cursor::new("/begin spec\nTarget: src/lib.rs\nModify lib\n/end\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        // No core calls: spec lines are captured locally, not forwarded to the core.
        assert_eq!(core.calls(), 0, "spec lines must not call core: {output}");
        // Plan summary is rendered to output after /end.
        assert!(
            output.contains("[PLAN]"),
            "plan must be rendered after /end: {output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn spec_empty_body_is_rejected() {
        // §3: A spec with no body lines must be rejected before creating a plan.
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("/begin spec\n/end\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(
            output.contains("[SPEC] rejected: empty specification body"),
            "{output}"
        );
        // No pending_plan → apply must be rejected for the right reason.
        assert!(
            !output.contains("[PLAN]"),
            "empty spec must not render a plan: {output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn spec_second_begin_spec_resets_capture() {
        // §2: A second /begin spec discards the in-progress session and starts fresh.
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/lib.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "pub fn lib() {}\n").expect("write");
        // First /begin spec is interrupted by a second one before /end.
        let mut input = io::Cursor::new(
            "/begin spec\nTarget: src/lib.rs\nFirst body\n/begin spec\nTarget: src/lib.rs\nSecond body\n/end\n",
        );
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core_in_workspace(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        // The second session should be captured; first is silently discarded.
        assert_eq!(core.calls(), 0, "no core calls expected: {output}");
        assert!(
            output.contains("[PLAN]"),
            "second spec must produce a plan: {output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_multiline_spec_with_end_is_consumed_and_not_routed_to_core() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/begin spec\ngoal: test\n/end\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(
            core.calls(),
            0,
            "Core should not be called for /end or capture"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_design_spec_generates_specification_context_and_diagnosis() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/begin spec\nsystem_name: DBM\ngoals:\nconstraints:\narchitecture:\nrules:\n  - ApplyGate required\n/end\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0, "design spec is consumed locally: {output}");
        assert!(
            output.contains("[SPEC_CONTEXT] generated"),
            "context must be generated: {output}"
        );
        assert!(
            output.contains("[RUNTIME_DIAGNOSIS] violations=0 warnings=0"),
            "diagnosis result must be rendered: {output}"
        );
        assert!(output.contains("Violations:"), "{output}");
        assert!(output.contains("Warnings:"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_design_spec_renders_repair_plan_after_diagnosis() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "\
/begin spec
system_name: DBM_REPL_UI
goals:
  - Separate input and output
constraints:
  - Preserve REPL compatibility
architecture:
  Runtime:
    responsibilities:
      - runtime bypass AuditCore
rules:
  - Runtime must pass through AuditCore
/end
";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("Repair Suggestions:"), "{output}");
        assert!(output.contains("[Critical]"), "{output}");
        assert!(output.contains("Introduce AuditGateway"), "{output}");
        assert!(output.contains("Enforce ApplyGate"), "{output}");
        assert!(output.contains("Steps:"), "{output}");
        assert!(output.contains("Implementation Plan"), "{output}");
        assert!(
            output.contains("Create AuditGateway abstraction"),
            "{output}"
        );
        assert!(
            output.contains("Route mutation through ApplyGate"),
            "{output}"
        );
        assert!(output.contains("Validation:"), "{output}");
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_design_specification_keys_auto_enter_capture() {
        for script in [
            "system_name: DBM\n/end\n",
            "goals:\n/end\n",
            "constraints:\n/end\n",
            "architecture:\n/end\n",
            "rules:\n/end\n",
        ] {
            let temp = tempfile::tempdir().expect("tempdir");
            let mut input = io::Cursor::new(script);
            let mut output = Vec::new();
            let core = CountingCore::new();

            run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
                .expect("repl");
            let output = String::from_utf8(output).expect("utf8");

            assert_eq!(core.calls(), 0, "{script} routed to core: {output}");
            assert!(
                output.contains("[SPEC_CONTEXT] generated"),
                "{script} must generate spec context: {output}"
            );
        }
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_non_yaml_design_specification_like_code_does_not_capture() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("let system_name = \"dbm\";\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 1);
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_end_without_capture_is_safely_ignored() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/end\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(
            core.calls(),
            0,
            "Core should not be called for /end without capture"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_end_case_insensitivity_is_consumed_locally() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/begin spec\ngoal: test\n/END\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(
            core.calls(),
            0,
            "Core should not be called since /END is consumed case-insensitively"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_end_command_boundary_validation_matches_only_exact_trimmed_token() {
        assert!(is_end_command("/end"));
        assert!(is_end_command("/END"));
        assert!(is_end_command("/end\r\n"));
        assert!(is_end_command("/end   "));
        assert!(!is_end_command("/end/end"));
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_invalid_end_command_rejects_without_dispatching_capture() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/begin spec\ngoal: test\n/end/end\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0, "invalid /end must stay local: {output}");
        assert!(
            output.contains("[SPEC] rejected: invalid end command"),
            "{output}"
        );
        assert!(
            !output.contains("[PLAN]") && !output.contains("[SPEC_CONTEXT] generated"),
            "invalid /end must not dispatch captured payload: {output}"
        );
    }

    // CATEGORY: REPL_ROUTING
    #[test]
    fn repl_double_end_is_consumed_locally() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "/begin spec\ngoal: test\n/end\n/end\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(
            core.calls(),
            0,
            "Core should not be called for the second /end"
        );
    }

    #[test]
    fn repl_confirmation_y_executes_pending_action() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "この変更を適用して\nY\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 1);
    }

    #[test]
    fn repl_confirmation_y_executes_mutation_plan_route() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "apps::cli::core を整理したい\nY\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("変更計画を生成しています。"), "{output}");
        assert_eq!(core.calls(), 0, "{output}");
    }

    #[test]
    fn repl_confirmation_y_executes_security_route() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "セキュリティ監査を実施して\nY\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(
            output.contains("セキュリティ監査を開始しました。"),
            "{output}"
        );
        assert_eq!(core.calls(), 1, "{output}");
    }

    #[test]
    fn repl_confirmation_n_discards_pending_action() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = "この変更を適用して\nN\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0, "{output}");
        assert!(output.contains("実行をキャンセルしました。"), "{output}");
    }
}
// DBM clarification execution guarantee
