pub mod autonomous_execution;
pub mod cognitive_explanation;
pub mod cognitive_workspace;
pub mod composer;
pub mod confidence_rank;
pub mod core;
pub mod cross_domain_governance;
pub mod edit_block;
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
pub mod renderer;
pub mod rendering;
pub mod review_batch;
pub mod runtime;
pub mod state;
pub mod temporal_cognition;
pub mod workspace_launcher;

use std::time::Duration;

use crossterm::event::{self, Event};

use crate::runtime::logging::isolate_tui_logging;

use self::core::RuntimeCoreBridge;
use self::model::UiPayload;
use self::renderer::{RenderScheduler, TerminalRenderer};
use self::rendering::RenderSnapshot;
use self::state::{TuiAction, TuiState, UiEvent};

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
    let core = RuntimeCoreBridge::with_defaults();
    let result = run_event_loop(&mut renderer, &mut state, &core);
    renderer.shutdown();
    result
}

fn run_event_loop(
    renderer: &mut TerminalRenderer,
    state: &mut TuiState,
    core: &RuntimeCoreBridge,
) -> Result<(), String> {
    let mut scheduler = RenderScheduler::default();
    scheduler.request_full_repaint();
    if let Some(request_id) = scheduler.take_pending() {
        let snapshot = RenderSnapshot::from(&*state);
        renderer.full_repaint(request_id, &snapshot)?;
        scheduler.on_repaint_complete(renderer.generation_ids().repaint_generation_id);
    }

    loop {
        if !state.event_queue.is_empty() {
            scheduler.notify_state_change();
        }
        state.handle_ui_events();

        if event::poll(FRAME_TIME).map_err(|e| e.to_string())? {
            let evt = event::read().map_err(|e| e.to_string())?;

            if state.diagnostic_mode {
                state.diagnostics.last_event = Some(format!("{:?}", evt));
                state.diagnostics.raw_mode_active = true; // Substrate is active if we are here
            }

            if let Event::Key(key) = evt {
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }
                match state.handle_key_event(key) {
                    TuiAction::Quit => break,
                    TuiAction::Submit(input) => {
                        let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
                        if !dispatch_runtime_command_to_projection(state, &working_dir, &input) {
                            self::core::handle_submit(state, core, input, working_dir);
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
            scheduler.notify_state_change();
        }

        if let Some(request_id) = scheduler.take_pending() {
            let snapshot = RenderSnapshot::from(&*state);
            renderer.full_repaint(request_id, &snapshot)?;
            scheduler.on_repaint_complete(renderer.generation_ids().repaint_generation_id);
        }
    }
    Ok(())
}

fn dispatch_runtime_command_to_projection(
    state: &mut TuiState,
    working_dir: &std::path::Path,
    input: &str,
) -> bool {
    let Some(lines) =
        crate::runtime::shell::RuntimeCommandDispatcher::dispatch(state, working_dir, input)
    else {
        return false;
    };

    let rejection_message = state.rejection.as_ref().map(|rejection| {
        format!(
            "runtime rejected: {} (via {})",
            rejection.reason, rejection.originating_mutation
        )
    });
    if let Some(message) = rejection_message {
        state.append_chat(UiEvent::Error { message });
    }
    project_runtime_lines(state, lines);
    true
}

fn project_runtime_lines(state: &mut TuiState, lines: Vec<String>) {
    let mut projected = false;
    for line in lines {
        state.append_chat(UiEvent::Runtime { message: line });
        projected = true;
    }

    if !projected {
        state.append_chat(UiEvent::Runtime {
            message: "[Runtime] command completed with no output".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::shell::empty_runtime_payload;
    use crate::tui::runtime::RuntimeShellState;

    fn runtime_messages(state: &TuiState) -> Vec<String> {
        state
            .chat
            .events
            .iter()
            .filter_map(|event| match event {
                UiEvent::Runtime { message } => Some(message.clone()),
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
        assert!(projection.contains("state=IDLE"), "{projection}");
        assert!(!state.chat.events.is_empty());
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
        assert!(projection.contains("state=PREVIEW_READY"), "{projection}");
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
        assert!(projection.contains("state=APPLIED"), "{projection}");
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
            UiEvent::Runtime { message } if message.contains("state=IDLE")
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
        assert!(
            projection.contains("REJECTED: target missing"),
            "{projection}"
        );
        assert!(state.chat.events.iter().any(|event| matches!(
            event,
            UiEvent::Error { message } if message.contains("runtime rejected: target missing")
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
        assert!(rendered.contains("[RUNTIME]"), "{rendered}");
        assert!(rendered.contains("state=IDLE"), "{rendered}");
    }
}
