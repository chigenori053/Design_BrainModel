pub mod composer;
pub mod confidence_rank;
pub mod core;
pub mod edit_block;
pub mod cognitive_workspace;
pub mod workspace_launcher;
pub mod multi_branch_orchestration;
pub mod temporal_cognition;
pub mod autonomous_execution;
pub mod governed_execution;
pub mod git_governance;
pub mod remote_governance;
pub mod cross_domain_governance;
pub mod governance_observability;
pub mod cognitive_explanation;
pub mod input;
pub mod model;
pub mod panels;
pub mod proc_strip;
pub mod render;
pub mod renderer;
pub mod rendering;
pub mod review_batch;
pub mod runtime;
pub mod state;

use std::time::Duration;

use crossterm::event::{self, Event};

use crate::runtime::logging::isolate_tui_logging;

use self::core::RuntimeCoreBridge;
use self::model::UiPayload;
use self::renderer::{RenderScheduler, TerminalRenderer};
use self::rendering::RenderSnapshot;
use self::state::{TuiAction, TuiState};

const FRAME_TIME: Duration = Duration::from_millis(16);

/// Launch the interactive TUI. Blocks until the user quits.
pub fn run_tui(payload: UiPayload) -> Result<(), String> {
    let _logging_guard = isolate_tui_logging();
    let mut renderer = TerminalRenderer::enter()?;

    let mut state = TuiState::new(payload);
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

        if event::poll(FRAME_TIME).map_err(|e| e.to_string())?
            && let Event::Key(key) = event::read().map_err(|e| e.to_string())?
        {
            match state.handle_key_event(key) {
                TuiAction::Quit => break,
                TuiAction::Submit(input) => {
                    let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
                    if let Some(_lines) = crate::runtime::shell::RuntimeCommandDispatcher::dispatch(
                        state,
                        &working_dir,
                        &input,
                    ) {
                        // Runtime-owned command handled.
                        // Trace instrumentation is already handled in dispatcher.
                    } else {
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
