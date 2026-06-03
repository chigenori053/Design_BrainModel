pub mod autonomous_execution;
pub mod cognitive_explanation;
pub mod cognitive_workspace;
pub mod composer;
pub mod confidence_rank;
pub mod core;
pub mod cross_domain_governance;
pub mod edit_block;
pub mod foundation;
pub mod git_governance;
pub mod governance_observability;
pub mod governed_execution;
pub mod input;
pub mod model;
pub mod multi_branch_orchestration;
pub mod panels;
pub mod proc_strip;
pub mod remote_governance;
pub mod render;
pub mod render_trace;
pub mod renderer;
pub mod rendering;
pub mod review_batch;
pub mod runtime;
pub mod runtime_worker;
pub mod state;
pub mod temporal_cognition;
pub mod workspace;
pub mod workspace_launcher;

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::event::{self, Event};

use crate::runtime::logging::isolate_tui_logging;

use self::core::RuntimeCoreBridge;
use self::model::UiPayload;
use self::renderer::{RenderScheduler, TerminalRenderer};
use self::rendering::RenderSnapshot;
use self::runtime_worker::{RuntimeStatus, RuntimeWorkerEvent};
use self::state::{TuiAction, TuiState, UiEvent};
use crate::specification_bridge::{SpecificationKind, classify_specification};

const FRAME_TIME: Duration = Duration::from_millis(16);

/// Launch the interactive TUI. Blocks until the user quits.
pub fn run_tui(payload: UiPayload, diagnostic: bool) -> Result<(), String> {
    let _logging_guard = isolate_tui_logging();
    let mut renderer = TerminalRenderer::enter()?;

    let mut state = TuiState::new(payload);
    state.diagnostic_mode = diagnostic;
    if let Ok(root) = std::env::current_dir() {
        state.enable_persistent_history(root.join(".dbm/cli_history"));
    }
    let core = Arc::new(RuntimeCoreBridge::with_defaults());
    let result = run_event_loop(&mut renderer, &mut state, core);
    renderer.shutdown();
    result
}

fn run_event_loop(
    renderer: &mut TerminalRenderer,
    state: &mut TuiState,
    core: Arc<RuntimeCoreBridge>,
) -> Result<(), String> {
    let mut scheduler = RenderScheduler::default();
    let (worker_tx, worker_rx) = std::sync::mpsc::channel();
    scheduler.request_full_repaint();
    if let Some(request_id) = scheduler.take_pending() {
        let snapshot = RenderSnapshot::from(&*state);
        renderer.full_repaint(request_id, &snapshot)?;
        scheduler.on_repaint_complete(renderer.generation_ids().repaint_generation_id);
    }

    loop {
        drain_runtime_worker_events(state, &worker_rx, &mut scheduler);
        flush_ui_events_before_render(state, &mut scheduler);

        if event::poll(FRAME_TIME).map_err(|e| e.to_string())? {
            let evt = event::read().map_err(|e| e.to_string())?;

            if state.diagnostic_mode {
                state.diagnostics.last_event = Some(format!("{:?}", evt));
                state.diagnostics.raw_mode_active = true; // Substrate is active if we are here
            }

            if let Event::Key(key) = evt {
                crate::tui::render_trace::record("key_event_received");
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }
                match state.handle_key_event(key) {
                    TuiAction::Quit => break,
                    TuiAction::Submit(input) => {
                        crate::tui::render_trace::record("[EVENT_LOOP_RECEIVED_SUBMIT]");
                        crate::tui::render_trace::record("submit_action_received");
                        let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
                        let routed =
                            dispatch_runtime_command_to_projection(state, &working_dir, &input);
                        if !routed {
                            crate::tui::render_trace::record("[SUBMIT_DISPATCH]");
                            self::core::handle_submit_async(
                                state,
                                Arc::clone(&core),
                                input,
                                working_dir,
                                worker_tx.clone(),
                            );
                        }
                    }
                    TuiAction::SaveDesign => {
                        let path = std::env::current_dir()
                            .unwrap_or_else(|_| ".".into())
                            .join("dbm_design.md");
                        match std::fs::write(&path, state.design_doc.rendered.join("\n")) {
                            Ok(_) => state.enqueue_event(self::state::UiEvent::Result {
                                message: format!("Design saved: {}", path.display()),
                            }),
                            Err(err) => state.enqueue_event(self::state::UiEvent::Error {
                                message: format!("save design failed: {err}"),
                            }),
                        }
                    }
                    TuiAction::None => {}
                }
            }
            flush_ui_events_before_render(state, &mut scheduler);
            scheduler.notify_state_change();
        }

        drain_runtime_worker_events(state, &worker_rx, &mut scheduler);
        flush_ui_events_before_render(state, &mut scheduler);

        if let Some(request_id) = scheduler.take_pending() {
            let snapshot = RenderSnapshot::from(&*state);
            renderer.full_repaint(request_id, &snapshot)?;
            scheduler.on_repaint_complete(renderer.generation_ids().repaint_generation_id);
        }
    }
    Ok(())
}

fn drain_runtime_worker_events(
    state: &mut TuiState,
    worker_rx: &Receiver<RuntimeWorkerEvent>,
    scheduler: &mut RenderScheduler,
) {
    while let Ok(event) = worker_rx.try_recv() {
        match event {
            RuntimeWorkerEvent::Progress { task_id, status } => {
                project_runtime_status(state, task_id.0, status);
            }
            RuntimeWorkerEvent::Result(result) => {
                crate::tui::render_trace::record("[WORKER_RESULT]");
                if result.status == RuntimeStatus::Completed {
                    crate::tui::render_trace::record(Box::leak(
                        format!(
                            "[COMPLETED_EVENT]\nrequest_id={}\nstatus=Completed",
                            result.task_id.0
                        )
                        .into_boxed_str(),
                    ));
                }
                project_runtime_status(state, result.task_id.0, RuntimeStatus::Projecting);
                self::core::apply_runtime_response(state, result.response);
                match result.status {
                    RuntimeStatus::Completed => state.enqueue_event(UiEvent::System {
                        summary: format!("task {} completed", result.task_id.0),
                    }),
                    RuntimeStatus::Failed => state.enqueue_event(UiEvent::Error {
                        message: format!("task {} failed", result.task_id.0),
                    }),
                    _ => {}
                }
            }
        }
        scheduler.notify_state_change();
    }
}

fn project_runtime_status(state: &mut TuiState, task_id: u64, status: RuntimeStatus) {
    state.runtime_state = match status {
        RuntimeStatus::Queued => crate::tui::runtime::RuntimeShellState::Thinking,
        RuntimeStatus::Planning => crate::tui::runtime::RuntimeShellState::Plan,
        RuntimeStatus::Executing | RuntimeStatus::Projecting => {
            crate::tui::runtime::RuntimeShellState::Apply
        }
        RuntimeStatus::Completed | RuntimeStatus::Cancelled => {
            crate::tui::runtime::RuntimeShellState::Idle
        }
        RuntimeStatus::Failed => crate::tui::runtime::RuntimeShellState::Failed,
    };
    match status {
        RuntimeStatus::Queued => state.enqueue_event(UiEvent::Thinking {
            summary: format!("task {task_id} queued"),
        }),
        RuntimeStatus::Planning => state.enqueue_event(UiEvent::Planning {
            summary: format!("task {task_id} planning runtime execution"),
        }),
        RuntimeStatus::Executing => state.enqueue_event(UiEvent::Execution {
            step: format!("task {task_id} executing runtime core"),
        }),
        RuntimeStatus::Projecting => state.enqueue_event(UiEvent::Runtime {
            message: format!("task {task_id} projecting runtime result"),
        }),
        RuntimeStatus::Completed => state.enqueue_event(UiEvent::System {
            summary: format!("task {task_id} completed"),
        }),
        RuntimeStatus::Failed => state.enqueue_event(UiEvent::Error {
            message: format!("task {task_id} failed"),
        }),
        RuntimeStatus::Cancelled => state.enqueue_event(UiEvent::System {
            summary: format!("task {task_id} cancelled"),
        }),
    }
}

fn flush_ui_events_before_render(state: &mut TuiState, scheduler: &mut RenderScheduler) {
    if state.event_queue.is_empty() {
        return;
    }
    scheduler.notify_state_change();
    state.handle_ui_events();
}

fn dispatch_runtime_command_to_projection(
    state: &mut TuiState,
    working_dir: &std::path::Path,
    input: &str,
) -> bool {
    crate::tui::render_trace::record("runtime_route_entered");
    if classify_specification(input) == SpecificationKind::DesignSpecification {
        crate::tui::render_trace::record("runtime_route_design_specification_guard");
        return false;
    }

    let Some(events) =
        crate::runtime::shell::RuntimeCommandDispatcher::dispatch(state, working_dir, input)
    else {
        crate::tui::render_trace::record("runtime_route_dispatcher_no_match");
        return false;
    };
    crate::tui::render_trace::record("runtime_route_dispatcher_matched");

    let rejection_message = state.rejection.as_ref().map(|rejection| {
        format!(
            "runtime rejected: {} (via {})",
            rejection.reason, rejection.originating_mutation
        )
    });
    if let Some(message) = rejection_message {
        state.append_chat(UiEvent::Error { message });
    }
    project_runtime_lines(state, events);
    true
}

fn project_runtime_lines(state: &mut TuiState, events: Vec<self::state::RuntimeNarrativeEvent>) {
    let mut projected = false;
    for event in events {
        let ui_event = match event {
            self::state::RuntimeNarrativeEvent::Intent { summary } => {
                self::state::UiEvent::Intent { summary }
            }
            self::state::RuntimeNarrativeEvent::Thinking { summary } => {
                self::state::UiEvent::Thinking { summary }
            }
            self::state::RuntimeNarrativeEvent::Analysis { summary } => {
                self::state::UiEvent::Analysis { summary }
            }
            self::state::RuntimeNarrativeEvent::Planning { summary } => {
                self::state::UiEvent::Planning { summary }
            }
            self::state::RuntimeNarrativeEvent::Validation { summary, target } => {
                if let Some(target) = target {
                    state.active_target = Some(target);
                }
                self::state::UiEvent::Validation { summary }
            }
            self::state::RuntimeNarrativeEvent::Execution { summary, target } => {
                if let Some(target) = target {
                    state.active_target = Some(target);
                }
                self::state::UiEvent::Execution { step: summary }
            }
            self::state::RuntimeNarrativeEvent::Apply { summary, target } => {
                if let Some(target) = target {
                    state.active_target = Some(target);
                }
                self::state::UiEvent::Apply { summary }
            }
            self::state::RuntimeNarrativeEvent::Commit { summary } => {
                self::state::UiEvent::Apply { summary }
            }
            self::state::RuntimeNarrativeEvent::Rollback { summary } => {
                self::state::UiEvent::Rollback { summary }
            }
            self::state::RuntimeNarrativeEvent::System { summary, target } => {
                if let Some(target) = target {
                    state.active_target = Some(target);
                }
                self::state::UiEvent::System { summary }
            }
            self::state::RuntimeNarrativeEvent::GovernanceReject { reason } => {
                self::state::UiEvent::Reject { reason }
            }
            self::state::RuntimeNarrativeEvent::Error { message } => {
                self::state::UiEvent::Error { message }
            }
            _ => self::state::UiEvent::Runtime {
                message: event.render(),
            },
        };
        state.append_chat(ui_event);
        projected = true;
    }

    if !projected {
        state.append_chat(self::state::UiEvent::Runtime {
            message: "[Runtime] command completed with no output".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::shell::empty_runtime_payload;
    use crate::specification_bridge::{StructuralDiagnosisResult, Violation};
    use crate::tui::runtime::RuntimeShellState;

    fn runtime_messages(state: &TuiState) -> Vec<String> {
        state
            .chat
            .events
            .iter()
            .filter_map(|event| match event {
                UiEvent::Runtime { message } => Some(message.clone()),
                UiEvent::Error { message } => Some(message.clone()),
                UiEvent::Intent { summary } => Some(summary.clone()),
                UiEvent::Thinking { summary } => Some(summary.clone()),
                UiEvent::Analysis { summary } => Some(summary.clone()),
                UiEvent::Planning { summary } => Some(summary.clone()),
                UiEvent::Validation { summary } => Some(summary.clone()),
                UiEvent::Execution { step } => Some(step.clone()),
                UiEvent::Apply { summary } => Some(summary.clone()),
                UiEvent::Rollback { summary } => Some(summary.clone()),
                UiEvent::System { summary } => Some(summary.clone()),
                UiEvent::Reject { reason } => Some(reason.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_runtime_status_projects_to_chat() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());

        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "status"
        ));

        let projection = runtime_messages(&state).join("\n");
        assert!(projection.contains("runtime idle"), "{projection}");
        assert!(!projection.contains("status: IDLE"), "{projection}");
        assert!(!state.chat.events.is_empty());
    }

    #[test]
    fn test_runtime_route_guard_preserves_design_specification_submit_path() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());
        let spec = "system_name: DBM_TUI\ngoals:\n";
        let initial_event_count = state.chat.events.len();

        assert!(!dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            spec
        ));

        let projection = runtime_messages(&state).join("\n");
        assert!(
            !projection.contains("[RUNTIME_ROUTE_TRACE]"),
            "{projection}"
        );
        assert_eq!(state.chat.events.len(), initial_event_count);
    }

    #[test]
    fn flush_ui_events_projects_workspace_before_render() {
        let mut state = TuiState::new(empty_runtime_payload());
        let mut scheduler = RenderScheduler::default();
        state.enqueue_event(UiEvent::StructuralDiagnosis {
            result: StructuralDiagnosisResult {
                violations: vec![Violation {
                    rule: "ApplyGate boundary unspecified".to_string(),
                    message: "architecture must expose ApplyGate boundary".to_string(),
                }],
                warnings: Vec::new(),
            },
        });

        flush_ui_events_before_render(&mut state, &mut scheduler);
        let snapshot = RenderSnapshot::from(&state);

        assert!(state.event_queue.is_empty());
        assert_eq!(
            snapshot.workspace.analysis_result.diagnosis,
            vec!["ApplyGate boundary unspecified".to_string()]
        );
        assert_eq!(snapshot.workspace.evaluation.status, "Diagnosis");
        assert!(scheduler.take_pending().is_some());
    }

    #[test]
    fn worker_events_project_progress_and_result_in_fifo_order() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut state = TuiState::new(empty_runtime_payload());
        let mut scheduler = RenderScheduler::default();
        let task_id = crate::tui::runtime_worker::RuntimeTaskId(7);

        tx.send(RuntimeWorkerEvent::Progress {
            task_id,
            status: RuntimeStatus::Planning,
        })
        .expect("planning");
        tx.send(RuntimeWorkerEvent::Progress {
            task_id,
            status: RuntimeStatus::Executing,
        })
        .expect("executing");
        tx.send(RuntimeWorkerEvent::Result(
            crate::tui::runtime_worker::RuntimeResult {
                task_id,
                status: RuntimeStatus::Completed,
                output: "done".to_string(),
                response: crate::core::CoreResponse {
                    events: vec![crate::core::CoreEvent::Result {
                        message: "done".to_string(),
                    }],
                    status: crate::core::ExecutionStatus::Executed,
                    design: None,
                    core_state: None,
                },
            },
        ))
        .expect("result");

        drain_runtime_worker_events(&mut state, &rx, &mut scheduler);
        flush_ui_events_before_render(&mut state, &mut scheduler);

        let projection = runtime_messages(&state).join("\n");
        let planning = projection.find("task 7 planning").expect(&projection);
        let executing = projection.find("task 7 executing").expect(&projection);
        let projecting = projection.find("task 7 projecting").expect(&projection);
        let completed = projection.find("task 7 completed").expect(&projection);

        assert!(planning < executing, "{projection}");
        assert!(executing < projecting, "{projection}");
        assert!(projecting < completed, "{projection}");
        assert!(scheduler.take_pending().is_some());
    }

    #[test]
    fn completed_worker_result_clears_active_task_before_snapshot() {
        crate::tui::render_trace::reset();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut state = TuiState::new(empty_runtime_payload());
        let mut scheduler = RenderScheduler::default();
        let task_id = crate::tui::runtime_worker::RuntimeTaskId(9);
        flush_ui_events_before_render(&mut state, &mut scheduler);
        crate::tui::render_trace::reset();
        state.workspace.evaluation.status = "Completed".to_string();
        state.workspace.evaluation.active_task =
            Some("Route mutation through ApplyGate".to_string());

        tx.send(RuntimeWorkerEvent::Result(
            crate::tui::runtime_worker::RuntimeResult {
                task_id,
                status: RuntimeStatus::Completed,
                output: "done".to_string(),
                response: crate::core::CoreResponse {
                    events: vec![crate::core::CoreEvent::Result {
                        message: "done".to_string(),
                    }],
                    status: crate::core::ExecutionStatus::Executed,
                    design: None,
                    core_state: None,
                },
            },
        ))
        .expect("result");

        drain_runtime_worker_events(&mut state, &rx, &mut scheduler);
        flush_ui_events_before_render(&mut state, &mut scheduler);
        let snapshot = RenderSnapshot::from(&state);
        let activity = snapshot.runtime.runtime_panel_lines(false).join("\n");
        let trace = crate::tui::render_trace::snapshot().join("\n");

        assert_eq!(snapshot.workspace.evaluation.status, "Completed");
        assert_eq!(snapshot.workspace.evaluation.active_task, None);
        assert!(activity.contains("[Completed] done"), "{activity}");
        assert!(
            activity.contains("[Completed] task 9 completed"),
            "{activity}"
        );
        assert!(
            trace.contains("[COMPLETED_EVENT]\nrequest_id=9\nstatus=Completed"),
            "{trace}"
        );
        assert!(
            trace.contains("[QUEUE_PUSH]\nevent=System\nsummary=task 9 completed"),
            "{trace}"
        );
        assert!(
            trace.contains("[QUEUE_PROCESS]\nevent=System\nsummary=task 9 completed"),
            "{trace}"
        );
        assert!(
            trace.contains("[ACTIVE_TASK_CLEAR]\nbefore=task-9\nafter=None"),
            "{trace}"
        );
        assert!(
            trace.contains("[SNAPSHOT]\nstatus=Completed\nactive_task=None"),
            "{trace}"
        );
    }

    #[test]
    fn test_runtime_preview_projects_to_chat() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("core.rs"), "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "preview core.rs"
        ));

        let projection = runtime_messages(&state).join("\n");
        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(projection.contains("preview ready"), "{projection}");
        assert!(projection.contains("transaction active"), "{projection}");
        assert!(
            state
                .active_target
                .as_deref()
                .is_some_and(|target| target.ends_with("core.rs"))
        );
    }

    #[test]
    fn test_runtime_apply_projects_to_chat() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("core.rs"), "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "preview core.rs"
        ));
        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "apply"
        ));

        let projection = runtime_messages(&state).join("\n");
        assert_eq!(state.runtime_state, RuntimeShellState::Git);
        assert!(
            projection.contains("transaction committed successfully"),
            "{projection}"
        );
        assert!(projection.contains("transaction committed"), "{projection}");
    }

    #[test]
    fn test_runtime_projection_persists() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());

        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "status"
        ));

        assert!(state.chat.events.iter().any(|event| matches!(
            event,
            UiEvent::System { summary } if summary == "runtime idle"
        )));
    }

    #[test]
    fn test_runtime_error_projects() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());

        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "preview missing.rs"
        ));

        let projection = runtime_messages(&state).join("\n");
        assert!(projection.contains("target missing"), "{projection}");
        assert!(state.chat.events.iter().any(|event| matches!(
            event,
            UiEvent::Error { message } if message.contains("target missing")
        )));
    }

    #[test]
    fn test_status_command_visible() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());

        assert!(dispatch_runtime_command_to_projection(
            &mut state,
            root.path(),
            "status"
        ));

        let rendered = state
            .chat
            .events
            .iter()
            .flat_map(UiEvent::lines)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("[SYSTEM] runtime idle"), "{rendered}");
        assert!(!rendered.contains("[RUNTIME] status:"), "{rendered}");
        assert!(!rendered.contains("status: IDLE"), "{rendered}");
    }
}
// DBM clarification execution guarantee
