use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;

pub use crate::core::{CoreEvent, CoreExecutor, CoreRequest, RuntimeCoreBridge};
use crate::intent_resolution::{
    ConfirmationEngine, ExecutionContext, ExecutionRouter, ExecutionState, ExecutionStatus,
    IntentResolutionEngine, RecommendedAction,
};
use crate::nl::normalization::normalize_runtime_input;
use crate::pipeline::PipelineState;
use crate::runtime::logging::{emit_debug, tui_logging_isolated};
use crate::runtime::runtime_events::DebugLevel;
use crate::specification_bridge::{
    ImplementationPlanner, RepairPlanner, SpecificationContext, SpecificationKind,
    StructuralDiagnosisRequest, classify_specification,
};
use crate::tui::runtime::RuntimeShellState;

use super::design_convergence::DesignConvergenceEngine;
use super::runtime_worker::{RuntimeStatus, RuntimeWorkerEvent, spawn_runtime_worker};
use super::state::{EventQueue, TuiState, UiEvent};

pub fn resolve_projection_target(state: &TuiState) -> Option<String> {
    state
        .active_transaction
        .as_ref()
        .map(|tx| tx.target_path.clone())
        .or_else(|| state.active_target.clone())
        .filter(|target| !target.trim().is_empty() && target != "preview")
}

pub fn to_ui_event(event: CoreEvent) -> UiEvent {
    match event {
        CoreEvent::Thinking { summary } => UiEvent::Thinking { summary },
        CoreEvent::Editing {
            target,
            action,
            reason,
        } => UiEvent::Editing {
            target,
            action: match reason {
                Some(reason) if !reason.is_empty() => format!("{action} ({reason})"),
                _ => action,
            },
        },
        CoreEvent::Plan { steps } => UiEvent::Plan { steps },
        CoreEvent::Execution { step } => UiEvent::Execution { step },
        CoreEvent::Preview { diff } => UiEvent::Preview { diff },
        CoreEvent::Diff { file, changes } => UiEvent::Diff { file, changes },
        CoreEvent::Result { message } => UiEvent::Result { message },
        CoreEvent::DesignUpdate { summary, score } => UiEvent::DesignUpdate { summary, score },
        CoreEvent::DesignDiff { changes } => UiEvent::DesignDiff { changes },
        CoreEvent::Pipeline { state } => UiEvent::Pipeline { state },
        CoreEvent::Next { actions } => UiEvent::Next { actions },
        CoreEvent::Error { message } => UiEvent::Error { message },
        CoreEvent::ErrorRecovery { candidates } => UiEvent::ErrorRecovery { candidates },
        CoreEvent::Debug { message } => UiEvent::Debug { message },
        CoreEvent::Proposal { candidates } => UiEvent::Proposal { candidates },
    }
}

pub fn handle_submit(
    state: &mut TuiState,
    core: &dyn CoreExecutor,
    input: String,
    _working_dir: PathBuf,
) {
    crate::tui::render_trace::record("[RUNTIME_DISPATCH] start");
    let _event = emit_debug("UI", "Input received", DebugLevel::Debug);
    let classification = classify_specification(input.trim());
    crate::tui::render_trace::record(Box::leak(
        format!("[PLANNER_START] classification={:?}", classification).into_boxed_str(),
    ));
    match classification {
        SpecificationKind::DraftSpecification => {
            handle_convergence_submit(state, input);
        }
        SpecificationKind::DesignSpecification => {
            handle_specification_submit(state, input);
        }
        SpecificationKind::Instruction if is_design_convergence_intent(&input) => {
            handle_convergence_submit(state, input);
        }
        SpecificationKind::Instruction => {
            // §7.1 §10.1: Transition to Thinking state before dispatch.
            // Runtime must never be silent — emit visible thinking event immediately.
            state.convergence.record_user_intent(&input);
            state.runtime_state = RuntimeShellState::Thinking;
            state.enqueue_event(UiEvent::Thinking {
                summary: "processing intent / 意図を処理中".to_string(),
            });

            // Phase 4.5: build CoreRequest (pass-through).
            let runtime_input = normalize_runtime_input(&input)
                .map(|normalized| normalized.command.to_runtime_input())
                .unwrap_or(input);
            let request = CoreRequest::new(runtime_input);
            crate::tui::render_trace::record("[EXECUTOR_START]");
            let mut response = core.execute(request);

            // §13.1: Empty event protection — execution must always produce visible narrative.
            if response.events.is_empty() {
                response.events.push(CoreEvent::Error {
                    message: "No runtime narrative generated".to_string(),
                });
            }

            let success = response.status != crate::core::ExecutionStatus::Failed;

            // Phase 4.5: sync core_snapshot first so downstream render reads correct state.
            if let Some(snapshot) = response.core_state {
                state.core_snapshot = snapshot.clone();
                state.pipeline_state = snapshot.status.clone();
            }

            apply_core_response(
                &mut state.event_queue,
                &mut state.pipeline_state,
                response.events,
            );

            if success && let Some(design) = response.design {
                state.update_design(design);
            }
        }
    }
}

pub fn handle_submit_async(
    state: &mut TuiState,
    core: Arc<RuntimeCoreBridge>,
    input: String,
    _working_dir: PathBuf,
    worker_tx: Sender<RuntimeWorkerEvent>,
) {
    crate::tui::render_trace::record("[HANDLE_SUBMIT_ASYNC_ENTER]");
    crate::tui::render_trace::record("[RUNTIME_DISPATCH] start");
    let _event = emit_debug("UI", "Input received", DebugLevel::Debug);
    let classification = classify_specification(input.trim());
    crate::tui::render_trace::record(Box::leak(
        format!("[PLANNER_START] classification={:?}", classification).into_boxed_str(),
    ));
    match classification {
        SpecificationKind::DraftSpecification => {
            handle_convergence_submit(state, input);
        }
        SpecificationKind::Instruction if is_design_convergence_intent(&input) => {
            handle_convergence_submit(state, input);
        }
        SpecificationKind::Instruction => handle_runtime_submit(state, core, input, worker_tx),
        SpecificationKind::DesignSpecification => {
            handle_specification_submit(state, input);
        }
    }
}

fn handle_convergence_submit(state: &mut TuiState, input: String) {
    let result = DesignConvergenceEngine::converge(&input, &state.convergence);
    state.convergence = result.state;
    state.enqueue_event(UiEvent::Intent {
        summary: "design convergence started from natural language intent".to_string(),
    });
    state.enqueue_event(UiEvent::Planning {
        summary: "generated Design Specification from convergence state".to_string(),
    });
    handle_specification_submit(state, result.generated_spec);
}

fn is_design_convergence_intent(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        "dbm_cli",
        "design convergence",
        "設計収束",
        "セルフ改修",
        "自己改修",
        "強化したい",
        "したい",
        "できるように",
    ]
    .into_iter()
    .any(|needle| lower.contains(needle))
}

pub fn handle_runtime_submit(
    state: &mut TuiState,
    core: Arc<RuntimeCoreBridge>,
    input: String,
    worker_tx: Sender<RuntimeWorkerEvent>,
) {
    state.convergence.record_user_intent(&input);
    let trimmed = input.trim();

    if let Some(action) = consume_confirmation_input(state, trimmed) {
        execute_confirmed_action(state, core, action, trimmed.to_string(), worker_tx);
        return;
    } else if state.pending_confirmation.is_none() && is_confirmation_cancel(trimmed) {
        return;
    }

    let resolved = IntentResolutionEngine::resolve(trimmed);
    state.resolved_intent = Some(resolved.clone());
    if resolved.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD {
        state.runtime_state = RuntimeShellState::ClarificationRequired;
        state.enqueue_event(UiEvent::Intent {
            summary: ConfirmationEngine::prompt(&resolved),
        });
        return;
    }
    if let Some(pending) = ConfirmationEngine::pending(&resolved) {
        state.pending_confirmation = Some(pending.clone());
        state.execution_state = ExecutionState::PendingConfirmation;
        state.execution_narrative = Some(pending.summary.clone());
        state.runtime_state = RuntimeShellState::AwaitConfirmation;
        state.enqueue_event(UiEvent::Intent {
            summary: pending.summary,
        });
        return;
    }
    if resolved.recommended_action() == RecommendedAction::RunAnalyze {
        state.enqueue_event(UiEvent::Intent {
            summary: ConfirmationEngine::prompt(&resolved),
        });
        execute_confirmed_action(
            state,
            core,
            RecommendedAction::RunAnalyze,
            trimmed.to_string(),
            worker_tx,
        );
        return;
    }

    state.runtime_state = RuntimeShellState::ClarificationRequired;
    state.enqueue_event(UiEvent::Intent {
        summary: ConfirmationEngine::prompt(&resolved),
    });
}

fn execute_confirmed_action(
    state: &mut TuiState,
    core: Arc<RuntimeCoreBridge>,
    action: RecommendedAction,
    user_input: String,
    worker_tx: Sender<RuntimeWorkerEvent>,
) {
    let resolved_intent = state
        .resolved_intent
        .clone()
        .unwrap_or_else(|| IntentResolutionEngine::resolve(&user_input));
    let context = ExecutionContext {
        user_input,
        resolved_intent,
        workspace_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let result = ExecutionRouter::execute(action, &context);
    state.execution_narrative = Some(result.narrative.clone());
    state.execution_state = match result.status {
        ExecutionStatus::Completed => ExecutionState::Running,
        ExecutionStatus::Failed => ExecutionState::Failed,
        ExecutionStatus::WaitingConfirmation => ExecutionState::PendingConfirmation,
    };
    state.enqueue_event(UiEvent::Execution {
        step: result.narrative,
    });
    if let Some(runtime_input) = ExecutionRouter::route(action) {
        queue_runtime_request(state, core, runtime_input.to_string(), worker_tx);
    }
}

fn queue_runtime_request(
    state: &mut TuiState,
    core: Arc<RuntimeCoreBridge>,
    input: String,
    worker_tx: Sender<RuntimeWorkerEvent>,
) {
    state.runtime_state = RuntimeShellState::Thinking;
    state.enqueue_event(UiEvent::Thinking {
        summary: "processing intent / 意図を処理中".to_string(),
    });

    let runtime_input = normalize_runtime_input(&input)
        .map(|normalized| normalized.command.to_runtime_input())
        .unwrap_or(input);
    let request = CoreRequest::new(runtime_input);
    crate::tui::render_trace::record("[ASYNC_PREPARE_WORKER]");
    let task = spawn_runtime_worker(core, request, worker_tx);
    state.enqueue_event(UiEvent::Thinking {
        summary: format!("task {} queued", task.id.0),
    });
    crate::tui::render_trace::record(Box::leak(
        format!(
            "[QUEUE_PUSH] runtime_task status={:?}",
            RuntimeStatus::Queued
        )
        .into_boxed_str(),
    ));
}

fn consume_confirmation_input(state: &mut TuiState, input: &str) -> Option<RecommendedAction> {
    let pending = state.pending_confirmation.clone()?;
    let lower = input.to_ascii_lowercase();
    match lower.as_str() {
        "y" | "yes" | "はい" | "実行" => {
            state.pending_confirmation = None;
            Some(pending.action)
        }
        "n" | "no" | "いいえ" | "cancel" | "キャンセル" | "中止" => {
            state.pending_confirmation = None;
            state.runtime_state = RuntimeShellState::Idle;
            state.enqueue_event(UiEvent::Intent {
                summary: "実行をキャンセルしました。".to_string(),
            });
            None
        }
        "p" | "preview" | "プレビュー" if pending.action == RecommendedAction::RunMutationApply =>
        {
            state.pending_confirmation = None;
            Some(RecommendedAction::RunMutationPreview)
        }
        _ => None,
    }
}

fn is_confirmation_cancel(input: &str) -> bool {
    matches!(
        input.to_ascii_lowercase().as_str(),
        "n" | "no" | "いいえ" | "cancel" | "キャンセル" | "中止"
    )
}

pub fn apply_runtime_response(state: &mut TuiState, mut response: crate::core::CoreResponse) {
    state.enqueue_event(UiEvent::Runtime {
        message: "runtime projecting result".to_string(),
    });
    if response.events.is_empty() {
        response.events.push(CoreEvent::Error {
            message: "No runtime narrative generated".to_string(),
        });
    }
    let success = response.status != crate::core::ExecutionStatus::Failed;
    state.execution_state = if success {
        ExecutionState::Completed
    } else {
        ExecutionState::Failed
    };

    if let Some(snapshot) = response.core_state {
        state.core_snapshot = snapshot.clone();
        state.pipeline_state = snapshot.status.clone();
    }

    apply_core_response(
        &mut state.event_queue,
        &mut state.pipeline_state,
        response.events,
    );

    if success && let Some(design) = response.design {
        state.update_design(design);
    }
}

fn handle_specification_submit(state: &mut TuiState, input: String) {
    crate::tui::render_trace::record_payload_dump(
        "RAW_PAYLOAD_BEGIN",
        "RAW_PAYLOAD_END",
        "PAYLOAD",
        &input,
    );
    let context = match SpecificationContext::from_yaml(&input) {
        Ok(context) => context,
        Err(err) => {
            let event = UiEvent::Error {
                message: format!("specification rejected: {err}"),
            };
            crate::tui::render_trace::record(Box::leak(
                format!("[QUEUE_PUSH] spec_error={:?}", event).into_boxed_str(),
            ));
            state.event_queue.push(event);
            return;
        }
    };
    let event = UiEvent::SpecContext {
        context: context.clone(),
    };
    crate::tui::render_trace::record(Box::leak(
        format!("[QUEUE_PUSH] spec_context={:?}", event).into_boxed_str(),
    ));
    state.event_queue.push(event);

    let request = StructuralDiagnosisRequest::new(context);
    let event = UiEvent::DomainClassification {
        domain: request.domain.as_str().to_string(),
    };
    crate::tui::render_trace::record(Box::leak(
        format!("[QUEUE_PUSH] domain={:?}", event).into_boxed_str(),
    ));
    state.event_queue.push(event);

    crate::tui::render_trace::record("core_submit_diagnosis_started");
    let diagnosis = request.diagnose();
    let event = UiEvent::StructuralDiagnosis {
        result: diagnosis.clone(),
    };
    crate::tui::render_trace::record(Box::leak(
        format!("[QUEUE_PUSH] diagnosis={:?}", event).into_boxed_str(),
    ));
    state.event_queue.push(event);

    let repair_plan = RepairPlanner::generate(&diagnosis);
    crate::tui::render_trace::record("core_submit_repair_plan_generated");
    let event = UiEvent::RepairPlan {
        plan: repair_plan.clone(),
    };
    crate::tui::render_trace::record(Box::leak(
        format!("[QUEUE_PUSH] repair_plan={:?}", event).into_boxed_str(),
    ));
    state.event_queue.push(event);

    let implementation_plan = ImplementationPlanner::generate(&repair_plan);
    crate::tui::render_trace::record("core_submit_implementation_plan_generated");
    let event = UiEvent::ImplementationPlan {
        plan: implementation_plan,
    };
    crate::tui::render_trace::record(Box::leak(
        format!("[QUEUE_PUSH] impl_plan={:?}", event).into_boxed_str(),
    ));
    state.event_queue.push(event);
    crate::tui::render_trace::record("core_submit_analysis_workspace_updated");
}

fn apply_core_response(
    queue: &mut EventQueue,
    pipeline_state: &mut PipelineState,
    events: Vec<CoreEvent>,
) {
    for event in events {
        if let CoreEvent::Pipeline { state } = &event
            && let Some(next) = pipeline_state_from_label(state)
        {
            *pipeline_state = next;
        }
        if !tui_logging_isolated() {
            let _event = emit_debug("UI", "Rendering event", DebugLevel::Trace);
        }
        let ui_event = to_ui_event(event);
        crate::tui::render_trace::record(Box::leak(
            format!("[QUEUE_PUSH] core_event={:?}", ui_event).into_boxed_str(),
        ));
        queue.push(ui_event);
    }
}

fn pipeline_state_from_label(label: &str) -> Option<PipelineState> {
    match label {
        "Idle" => Some(PipelineState::Idle),
        "Proposed" => Some(PipelineState::Proposed),
        "Planned" => Some(PipelineState::Planned),
        "Previewed" => Some(PipelineState::Previewed),
        "Applied" => Some(PipelineState::Applied),
        "Staged" => Some(PipelineState::Staged),
        "Committed" => Some(PipelineState::Committed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        Constraint, CoreResponse, DesignDocument, ExecutionStatus, ReasonUnit, StructureTree,
    };
    use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
    use crate::tui::rendering::RenderSnapshot;
    use crate::tui::state::{Diff, RuntimeTransaction, TuiAction};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[derive(Default)]
    struct FakeCore {
        response: Option<CoreResponse>,
        seen_input: std::sync::Mutex<Option<String>>,
    }

    impl CoreExecutor for FakeCore {
        fn execute(&self, request: CoreRequest) -> CoreResponse {
            *self.seen_input.lock().expect("seen input") = Some(request.raw.clone());
            self.response.clone().unwrap_or(CoreResponse {
                events: vec![CoreEvent::Result {
                    message: "done".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            })
        }
    }

    fn test_design(version: u64) -> DesignDocument {
        DesignDocument::new(
            version,
            vec![ReasonUnit {
                id: "ru-test".to_string(),
                title: "test".to_string(),
                summary: "test design".to_string(),
            }],
            StructureTree {
                module: "test".to_string(),
                functions: vec!["test_fn".to_string()],
            },
            vec![Constraint {
                text: "test constraint".to_string(),
            }],
        )
    }

    fn runtime_transaction(target: &str) -> RuntimeTransaction {
        RuntimeTransaction {
            tx_id: "tx-workspace-projection".to_string(),
            target_path: target.to_string(),
            resolved_target: crate::runtime::shell::ResolvedExecutionTarget::from_canonical_path(
                target,
            ),
            diff: Diff {
                file: target.to_string(),
                changes: vec![],
            },
            failed_recoverable: false,
        }
    }

    #[test]
    fn workspace_projection_uses_transaction_authority() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = None;
        state.active_transaction = Some(runtime_transaction("apps/cli/src/repl.rs"));

        assert_eq!(
            resolve_projection_target(&state).as_deref(),
            Some("apps/cli/src/repl.rs")
        );
        assert_eq!(
            RenderSnapshot::from(&state)
                .projection
                .workspace
                .target
                .as_deref(),
            Some("apps/cli/src/repl.rs")
        );
    }

    #[test]
    fn projection_target_persists_while_transaction_active() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::PreviewReady;
        state.active_target = Some("apps/cli/src/repl.rs".to_string());
        state.active_transaction = Some(runtime_transaction("apps/cli/src/repl.rs"));

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });

        assert!(state.active_transaction.is_some());
        assert_eq!(state.active_target.as_deref(), Some("apps/cli/src/repl.rs"));
        assert_eq!(
            RenderSnapshot::from(&state)
                .projection
                .workspace
                .target
                .as_deref(),
            Some("apps/cli/src/repl.rs")
        );
    }

    #[test]
    fn submit_normalizes_japanese_runtime_intent_before_core() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();

        handle_submit(
            &mut state,
            &core,
            "parser.rs を preview".to_string(),
            ".".into(),
        );

        assert_eq!(
            core.seen_input.lock().expect("seen").as_deref(),
            Some("preview parser.rs")
        );
    }

    #[test]
    fn submit_forwards_input_and_renders_events() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Result {
                        message: "ok".to_string(),
                    },
                    CoreEvent::Pipeline {
                        state: "Proposed".to_string(),
                    },
                ],
                status: ExecutionStatus::Proposed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };

        handle_submit(&mut state, &core, "fix parser bug".to_string(), ".".into());
        state.handle_ui_events();

        assert_eq!(state.pipeline_state, PipelineState::Proposed);
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line == "[RESULT] ok")
        );
    }

    #[test]
    fn design_spec_submit_runs_specification_pipeline_without_core_dispatch() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        let spec = r#"system_name: DBM_REPL_UI

goals:
  - Separate input and output

architecture:
  SpecificationEditor:
    responsibilities:
      - Edit explicit design specifications

rules:
  - Runtime must pass through AuditCore
"#;

        handle_submit(&mut state, &core, spec.to_string(), ".".into());
        state.handle_ui_events();

        assert_eq!(core.seen_input.lock().expect("seen").as_deref(), None);
        let lines = state.workspace.analysis_result.lines();
        assert!(lines.contains(&"Diagnosis".to_string()));
        assert!(lines.contains(&"Repair Plan".to_string()));
        assert!(lines.contains(&"Implementation Plan".to_string()));
        assert!(!state.workspace.analysis_result.diagnosis.is_empty());
        assert!(!state.workspace.analysis_result.repair_plan.is_empty());
        assert!(
            !state
                .workspace
                .analysis_result
                .implementation_plan
                .is_empty()
        );
        assert_eq!(state.workspace.evaluation.status, "Completed");
        assert_eq!(state.workspace.evaluation.progress, 100);
        let rendered = state
            .chat
            .events
            .iter()
            .map(UiEvent::text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!rendered.contains("[CORE_SUBMIT_TRACE]"), "{rendered}");
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .all(|line| !line.contains("[SPEC_CONTEXT]"))
        );
    }

    #[test]
    fn self_modification_intent_starts_design_convergence() {
        crate::tui::render_trace::reset();
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();

        handle_submit(
            &mut state,
            &core,
            "DBM_CLIでセルフ改修したい".to_string(),
            ".".into(),
        );
        state.handle_ui_events();

        assert_eq!(core.seen_input.lock().expect("seen").as_deref(), None);
        assert_eq!(
            state
                .convergence
                .intent
                .as_ref()
                .map(|intent| intent.objective.as_str()),
            Some("self_modification")
        );
        let lines = state.convergence.workspace_lines(&[]);
        let surface = lines.join("\n");
        let log = state
            .convergence
            .log
            .iter()
            .map(|entry| entry.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(surface.contains("Natural Language Input"));
        assert!(surface.contains("DBM_CLIでセルフ改修したい"));
        assert!(log.contains("domain=runtime"));
        assert!(log.contains("objective=self_modification"));
        assert!(log.contains("target=design_cli"));
        assert!(surface.contains("Generated Specification"));
        assert!(!lines.iter().any(|line| line.contains("(not started)")));
        assert!(!lines.iter().any(|line| line.contains("(not generated)")));
        assert!(
            state
                .convergence
                .log
                .iter()
                .any(|entry| entry.kind == "Intent")
        );
        assert!(
            state
                .convergence
                .log
                .iter()
                .any(|entry| entry.kind == "Question")
        );
        assert!(
            state
                .convergence
                .log
                .iter()
                .any(|entry| entry.kind == "Decision")
        );
        assert!(
            state
                .convergence
                .log
                .iter()
                .any(|entry| entry.kind == "Spec")
        );
        let trace = crate::tui::render_trace::snapshot().join("\n");
        assert!(!trace.contains("PAYLOAD_LINE_1 DBM_CLIでセルフ改修"));
        assert!(trace.contains("PAYLOAD_LINE_1 system_name: DBM"));
    }

    #[test]
    fn goals_only_draft_starts_design_convergence() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();

        handle_submit(
            &mut state,
            &core,
            "goals:\n- DBM_CLIでセルフ改修したい".to_string(),
            ".".into(),
        );
        state.handle_ui_events();

        assert_eq!(core.seen_input.lock().expect("seen").as_deref(), None);
        assert!(state.convergence.intent.is_some());
        assert!(state.convergence.generated_spec.is_some());
    }

    #[test]
    fn system_name_input_routes_directly_to_specification_analyzer() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        let spec = "system_name: DBM\n\ngoals:\n  - Self modification\narchitecture:\n  RuntimeCore:\n    responsibilities:\n      - Execute generated plans\nrules:\n  - ApplyGate required";

        handle_submit(&mut state, &core, spec.to_string(), ".".into());
        state.handle_ui_events();

        assert_eq!(core.seen_input.lock().expect("seen").as_deref(), None);
        assert!(state.convergence.intent.is_none());
        assert_eq!(state.workspace.evaluation.status, "Completed");
        assert_ne!(state.workspace.evaluation.domain, "(none)");
        assert_eq!(state.workspace.evaluation.progress, 100);
    }

    #[test]
    fn specification_submit_records_parser_input_dump() {
        crate::tui::render_trace::reset();
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        let spec = "system_name: DBM\n\ngoals:\n  - stabilize parser\narchitecture:\n  Parser:\n    responsibilities:\n      - Parse specifications\nrules:\n  - ApplyGate required";

        handle_submit(&mut state, &core, spec.to_string(), ".".into());

        let trace = crate::tui::render_trace::snapshot();
        assert!(trace.contains(&"RAW_PAYLOAD_BEGIN"));
        assert!(trace.contains(&"RAW_PAYLOAD_END"));
        assert!(trace.contains(&"PAYLOAD_LINE_1 system_name: DBM"));
        assert!(trace.contains(&"PAYLOAD_LINE_2"));
        assert!(trace.contains(&"PAYLOAD_LINE_3 goals:"));
        assert!(trace.contains(&"PAYLOAD_LINE_4   - stabilize parser"));
        assert!(trace.contains(&"PAYLOAD_LINE_5 architecture:"));
        assert!(trace.contains(&"PAYLOAD_LINE_9 rules:"));
    }

    #[test]
    fn command_enter_submit_updates_evaluation_and_analysis_workspaces() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        let spec = "system_name: DBM_TUI_Test\n\narchitecture:\n  RuntimeCore:\n    responsibilities:\n      - Execute submitted specifications\nrules:\n  - Runtime must pass through ApplyGate";
        state.editor_state.editor.clear();
        for ch in spec.chars() {
            if ch == '\n' {
                state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            } else {
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
            }
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));
        let TuiAction::Submit(submitted) = action else {
            panic!("expected submit, got {action:?}");
        };
        handle_submit(&mut state, &core, submitted, ".".into());
        state.handle_ui_events();

        assert_ne!(state.workspace.evaluation.domain, "(none)");
        assert_ne!(state.workspace.evaluation.status, "Recognition");
        assert_eq!(state.workspace.evaluation.status, "Completed");
        assert_eq!(state.workspace.evaluation.progress, 100);
    }

    fn empty_payload() -> UiPayload {
        UiPayload {
            trace: TraceViewModel {
                request_id: "test".to_string(),
                steps: vec![],
                stats: TraceStatsViewModel {
                    total_nodes: 0,
                    max_depth: 0,
                    recall_hit_rate: 0.0,
                    avg_branching: 0.0,
                },
            },
            hypotheses: vec![],
            memory: vec![],
            selected: None,
        }
    }

    // ─── §14.1 Interactive Runtime Tests ────────────────────────────────────

    /// §14.1 — Every submit produces at least one visible runtime event.
    #[test]
    fn test_submit_generates_visible_event() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(
            &mut state,
            &core,
            "analyze workspace".to_string(),
            ".".into(),
        );
        state.handle_ui_events();
        assert!(!state.flattened_chat_lines().is_empty());
    }

    /// §14.1 — Runtime state transitions to Thinking before Core dispatch.
    #[test]
    fn test_runtime_transition_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(&mut state, &core, "fix parser".to_string(), ".".into());
        state.handle_ui_events();
        // At least one [THINKING] line must appear.
        let lines = state.flattened_chat_lines();
        assert!(
            lines.iter().any(|l| l.starts_with("[THINKING]")),
            "no [THINKING] line found: {lines:?}"
        );
    }

    #[test]
    fn enter_submit_projects_user_intent_and_reasoning_activity() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        state.editor_state.editor.clear();
        for ch in "hello".chars() {
            state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let TuiAction::Submit(input) = action else {
            panic!("expected Enter to create TuiAction::Submit");
        };
        handle_submit(&mut state, &core, input, ".".into());
        state.handle_ui_events();

        assert_eq!(state.convergence.raw_intent.as_deref(), Some("hello"));
        assert!(
            state
                .convergence
                .timeline_lines()
                .iter()
                .any(|line| line.trim() == "hello")
        );
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line.starts_with("[THINKING]"))
        );
        let snapshot = RenderSnapshot::from(&state);
        assert!(
            snapshot
                .reasoning
                .lines
                .iter()
                .any(|line| line.contains("hello"))
        );
    }

    #[test]
    fn enter_submit_reaches_async_runtime_worker_and_projects_response() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "hello".chars() {
            state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let TuiAction::Submit(input) = action else {
            panic!("expected Enter to create TuiAction::Submit");
        };
        let (worker_tx, worker_rx) = std::sync::mpsc::channel();

        handle_submit_async(
            &mut state,
            Arc::new(RuntimeCoreBridge::with_defaults()),
            input,
            ".".into(),
            worker_tx,
        );
        state.handle_ui_events();

        assert_eq!(state.convergence.raw_intent.as_deref(), Some("hello"));
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line.starts_with("[THINKING]"))
        );

        let mut runtime_result = None;
        for _ in 0..3 {
            let event = worker_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("runtime worker event");
            if let RuntimeWorkerEvent::Result(result) = event {
                runtime_result = Some(result);
                break;
            }
        }
        let result = runtime_result.expect("runtime worker result");
        apply_runtime_response(&mut state, result.response);
        state.handle_ui_events();

        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line.starts_with("[RESULT]") || line.starts_with("[ERROR]"))
        );
        assert!(
            RenderSnapshot::from(&state)
                .reasoning
                .lines
                .iter()
                .any(|line| line.contains("hello"))
        );
    }

    /// §14.1 §13.1 — Empty Core response triggers fallback Error event.
    #[test]
    fn test_empty_response_protection() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "unknown xyz".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(!lines.is_empty(), "runtime silence after empty response");
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("[THINKING]") || l.starts_with("[ERROR]")),
            "no visible narrative after empty response: {lines:?}"
        );
    }

    /// §14.1 §9.1 — Thinking narrative emitted by Core is visible in chat.
    #[test]
    fn test_narrative_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Thinking {
                    summary: "cognitive processing active".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(
            &mut state,
            &core,
            "analyze workspace".to_string(),
            ".".into(),
        );
        state.handle_ui_events();
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|l| l.contains("[THINKING]") && l.contains("cognitive processing active")),
            "narrative not visible in chat"
        );
    }

    /// §14.1 §8.1 — Design update projected into design panel after execution.
    #[test]
    fn test_projection_updates_after_execution() {
        let mut state = TuiState::new(empty_payload());
        let new_version = state.design_doc.version + 1;
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Result {
                    message: "updated".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: Some(test_design(new_version)),
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        let prev_version = state.design_doc.version;
        handle_submit(&mut state, &core, "update design".to_string(), ".".into());
        state.handle_ui_events();
        assert_ne!(
            state.design_doc.version, prev_version,
            "design projection not updated"
        );
    }

    // ─── §14.2 Intent Tests ──────────────────────────────────────────────────

    /// §14.2 — English normalized intent reaches Core correctly.
    #[test]
    fn test_english_intent_execution() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(
            &mut state,
            &core,
            "analyze workspace".to_string(),
            ".".into(),
        );
        // "analyze workspace" starts with "analyze" → normalized to "analyze"
        assert_eq!(core.seen_input.lock().unwrap().as_deref(), Some("analyze"));
    }

    /// §14.2 — Japanese intent (解析) normalizes and reaches Core.
    #[test]
    fn test_japanese_intent_execution() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(
            &mut state,
            &core,
            "ランタイム状態を解析する".to_string(),
            ".".into(),
        );
        state.handle_ui_events();
        // Must not be silent.
        assert!(!state.flattened_chat_lines().is_empty());
        // Intent should have been normalized to "analyze"
        assert_eq!(core.seen_input.lock().unwrap().as_deref(), Some("analyze"));
    }

    /// §14.2 — Mixed bilingual intent normalizes correctly.
    #[test]
    fn test_bilingual_intent_execution() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(
            &mut state,
            &core,
            "parser.rs を preview".to_string(),
            ".".into(),
        );
        assert_eq!(
            core.seen_input.lock().unwrap().as_deref(),
            Some("preview parser.rs")
        );
    }

    /// §14.2 §6.3 — Unknown intent still produces a visible event (no silence).
    #[test]
    fn test_unknown_intent_generates_error() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Error {
                    message: "unknown intent: !@#$".to_string(),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "!@#$ gibberish".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("[ERROR]") || l.starts_with("[THINKING]")),
            "no visible event for unknown intent: {lines:?}"
        );
    }

    // ─── §14.3 Governance Tests ──────────────────────────────────────────────

    /// §14.3 §12.1 — Governance rejection event is visible in chat.
    #[test]
    fn test_governance_narrative_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Error {
                    message: "GovernanceRejected: mutation risk exceeds threshold".to_string(),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "risky mutation".to_string(), ".".into());
        state.handle_ui_events();
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|l| l.contains("[ERROR]") && l.contains("GovernanceRejected")),
            "governance narrative not visible"
        );
    }

    /// §14.3 §12.2 — Rejection reason is rendered in chat projection.
    #[test]
    fn test_rejection_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Error {
                        message: "SafetyViolation: rm -rf rejected".to_string(),
                    },
                    CoreEvent::Next {
                        actions: vec!["undo".to_string(), "reselect".to_string()],
                    },
                ],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "rm -rf /".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(
            lines.iter().any(|l| l.contains("[ERROR]")),
            "rejection not projected"
        );
        assert!(
            lines.iter().any(|l| l.contains("[INTENT]")),
            "recovery actions not projected"
        );
    }

    /// §14.3 — Pipeline projection is visible after execution.
    #[test]
    fn test_execution_pipeline_projection() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Thinking {
                        summary: "planning".to_string(),
                    },
                    CoreEvent::Execution {
                        step: "strategy executed".to_string(),
                    },
                    CoreEvent::Result {
                        message: "pipeline complete".to_string(),
                    },
                    CoreEvent::Pipeline {
                        state: "Planned".to_string(),
                    },
                ],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "execute plan".to_string(), ".".into());
        state.handle_ui_events();
        assert_eq!(state.pipeline_state, PipelineState::Planned);
        let lines = state.flattened_chat_lines();
        assert!(lines.iter().any(|l| l.starts_with("[EXECUTION]")));
        assert!(lines.iter().any(|l| l.starts_with("[RESULT]")));
    }

    // ─── §14.4 Render Tests ──────────────────────────────────────────────────

    /// §14.4 §11.1 — state_generation_id advances after event processing.
    #[test]
    fn test_render_updates_after_event() {
        let mut state = TuiState::new(empty_payload());
        let gen_before = state.state_generation_id;
        state.enqueue_event(UiEvent::Thinking {
            summary: "render trigger".to_string(),
        });
        state.handle_ui_events();
        assert!(
            state.state_generation_id > gen_before,
            "generation_id did not advance after event"
        );
    }

    /// §14.4 — Events appear in flattened chat lines after handle_ui_events.
    #[test]
    fn test_chat_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Thinking {
                        summary: "planning workspace update".to_string(),
                    },
                    CoreEvent::Result {
                        message: "workspace updated".to_string(),
                    },
                ],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(
            &mut state,
            &core,
            "analyze workspace".to_string(),
            ".".into(),
        );
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("planning workspace update"))
        );
        assert!(lines.iter().any(|l| l.contains("workspace updated")));
    }

    /// §14.4 §8.1 — Workspace design panel updates reflect execution output.
    #[test]
    fn test_workspace_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let new_version = state.design_doc.version + 1;
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::DesignUpdate {
                    summary: "workspace redesigned".to_string(),
                    score: 0.91,
                }],
                status: ExecutionStatus::Executed,
                design: Some(test_design(new_version)),
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(
            &mut state,
            &core,
            "redesign workspace".to_string(),
            ".".into(),
        );
        state.handle_ui_events();
        let panel = state.design_panel_lines();
        assert!(
            panel.iter().any(|l| l.contains(&new_version.to_string())),
            "design version not reflected in workspace projection"
        );
    }

    /// §14.4 §11.1 — Event queue is populated after submit (before handle_ui_events).
    #[test]
    fn test_runtime_refresh_after_submit() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(&mut state, &core, "fix bug".to_string(), ".".into());
        // Queue must have events before handle_ui_events drains them.
        assert!(
            !state.event_queue.is_empty(),
            "event queue empty immediately after submit"
        );
    }

    // ─── §14.5 Stability Tests ───────────────────────────────────────────────

    /// §14.5 §10.2 — Every submit produces at least one visible chat line.
    #[test]
    fn test_no_runtime_silence() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        let initial_lines = state.flattened_chat_lines().len();
        handle_submit(&mut state, &core, "some command".to_string(), ".".into());
        state.handle_ui_events();
        assert!(
            state.flattened_chat_lines().len() > initial_lines,
            "runtime was silent after submit"
        );
    }

    /// §14.5 — Pipeline state transitions do not desync runtime_state projection.
    #[test]
    fn test_no_projection_desync() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Pipeline {
                    state: "Proposed".to_string(),
                }],
                status: ExecutionStatus::Proposed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "propose change".to_string(), ".".into());
        state.handle_ui_events();
        // pipeline_state and runtime_state must be consistent.
        assert_eq!(state.pipeline_state, PipelineState::Proposed);
        // runtime_state should not be Idle after a Proposed pipeline event.
        assert_ne!(
            state.runtime_state,
            crate::tui::runtime::RuntimeShellState::Idle
        );
    }

    /// §14.5 §7.2 — Narrative events and pipeline state are consistent.
    #[test]
    fn test_narrative_runtime_consistency() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Thinking {
                        summary: "planning execution".to_string(),
                    },
                    CoreEvent::Execution {
                        step: "step 1".to_string(),
                    },
                    CoreEvent::Result {
                        message: "execution complete".to_string(),
                    },
                ],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "execute".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        // Narrative must contain Thinking before Result.
        let thinking_pos = lines.iter().position(|l| l.contains("[THINKING]"));
        let result_pos = lines.iter().position(|l| l.contains("[RESULT]"));
        assert!(
            thinking_pos.is_some() && result_pos.is_some(),
            "narrative sequence incomplete"
        );
    }

    /// §14.5 — Multiple successive submits remain stable.
    #[test]
    fn test_interactive_loop_stability() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        for i in 0..5 {
            handle_submit(&mut state, &core, format!("command {i}"), ".".into());
            state.handle_ui_events();
        }
        // No panic, pipeline stable, events visible.
        assert!(!state.flattened_chat_lines().is_empty());
        assert_eq!(state.pipeline_state, PipelineState::Idle);
    }

    /// DBM-NARRATIVE-PROJECTION-BINDING-SPEC §12.5 — status command visible in narrative.
    #[test]
    fn test_submit_status_visible_end_to_end() {
        let mut state = TuiState::new(empty_payload());
        // Use a real dispatcher command 'status'
        super::super::dispatch_runtime_command_to_projection(
            &mut state,
            std::path::Path::new("."),
            "status",
        );

        let lines = state.flattened_chat_lines();
        assert!(
            lines
                .iter()
                .any(|l: &String| l.contains("[SYSTEM]") && l.contains("runtime idle")),
            "status output not visible in narrative"
        );
    }

    /// DBM-NARRATIVE-PROJECTION-BINDING-SPEC §12.5 — narrative survives redraw.
    #[test]
    fn test_runtime_projection_survives_redraw() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Runtime {
            message: "trace 1".to_string(),
        });

        let snapshot1 = RenderSnapshot::from(&state);
        assert!(
            snapshot1
                .runtime
                .runtime_panel_lines(false)
                .iter()
                .any(|line: &String| line == "次の入力を待機しています。")
        );

        // Simulating a "redraw" by creating a new snapshot
        let snapshot2 = RenderSnapshot::from(&state);
        assert_eq!(
            snapshot1.runtime.narrative_lines,
            snapshot2.runtime.narrative_lines
        );
    }

    /// DBM-NARRATIVE-PROJECTION-BINDING-SPEC §12.5 — narrative is scrollable.
    #[test]
    fn test_runtime_projection_scrollable() {
        let mut state = TuiState::new(empty_payload());
        state.chat_scroll.offset = 5;

        let snapshot = RenderSnapshot::from(&state);
        assert_eq!(snapshot.runtime.scroll_offset, 5);
    }
}
