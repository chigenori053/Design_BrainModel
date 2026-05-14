use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::core::RuntimeCoreBridge;
use crate::runtime::event_queue::RuntimeEventQueue;
use crate::runtime::logging::{emit_debug, tui_logging_isolated};
use crate::runtime::runtime_events::{DebugLevel, RenderEvent, RuntimeEvent};
use crate::runtime::runtime_state::initial_runtime_state;
use crate::runtime::shell::RuntimeCommandDispatcher;
use crate::tui::core::handle_submit;
use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
use crate::tui::rendering::runtime_semantic_events;
use crate::tui::state::TuiState;

pub fn run_runtime_loop_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    run_runtime_loop(
        &mut reader,
        &mut writer,
        std::env::current_dir().unwrap_or_else(|_| ".".into()),
    )
}

pub fn run_runtime_loop<R, W>(
    reader: &mut R,
    writer: &mut W,
    workspace_root: PathBuf,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let mut state = TuiState::new(empty_payload());
    state.runtime_state = initial_runtime_state();
    state.enable_persistent_history(workspace_root.join(".dbm/cli_history"));
    let core = RuntimeCoreBridge::with_defaults();
    let mut events = RuntimeEventQueue::default();
    events.emit(RuntimeEvent::BootstrapStarted);
    events.emit(RuntimeEvent::LoopStarted);
    events.emit(emit_debug("RUNTIME][LOOP", "start", DebugLevel::Info));

    request_render(&mut events, "initial");
    render_initial(writer, &state)?;

    for line in reader.lines() {
        let input = line.map_err(|err| err.to_string())?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        events.emit(RuntimeEvent::InputAccepted(trimmed.to_string()));
        if matches!(trimmed, "/exit" | "/quit" | "exit" | "quit") {
            events.emit(RuntimeEvent::ShutdownRequested);
            break;
        }

        if let Some(events) =
            RuntimeCommandDispatcher::dispatch(&mut state, &workspace_root, trimmed)
        {
            for event in events {
                writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
            }
            continue;
        } else {
            handle_submit(
                &mut state,
                &core,
                trimmed.to_string(),
                workspace_root.clone(),
            );
            state.handle_ui_events();
        }
        request_render(&mut events, "runtime event consumed");
        render_initial(writer, &state)?;
    }

    flush_runtime_events(writer, &events)?;
    writer.flush().map_err(|err| err.to_string())
}

fn render_initial<W: Write>(writer: &mut W, state: &TuiState) -> Result<(), String> {
    for event in runtime_semantic_events(state) {
        writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn request_render(events: &mut RuntimeEventQueue, reason: &str) {
    events.emit(RuntimeEvent::Render(RenderEvent {
        reason: reason.to_string(),
    }));
}

fn flush_runtime_events<W: Write>(
    writer: &mut W,
    events: &RuntimeEventQueue,
) -> Result<(), String> {
    if tui_logging_isolated() {
        return Ok(());
    }
    for event in events.iter() {
        writeln!(writer, "[RUNTIME][EVENT] {event:?}").map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn empty_payload() -> UiPayload {
    UiPayload {
        trace: TraceViewModel {
            request_id: "runtime-loop".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_loop_starts_renders_and_exits_without_core_input() {
        let mut input = io::Cursor::new("/exit\n");
        let mut output = Vec::new();
        let root = tempfile::tempdir().expect("tempdir");

        run_runtime_loop(&mut input, &mut output, root.path().to_path_buf()).expect("loop");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("runtime idle"));
        assert!(output.contains("[RUNTIME][EVENT] BootstrapStarted"));
        assert!(!output.contains("[ROUTE]"));
    }

    #[test]
    fn deterministic_initial_rendering_is_stable() {
        let state = TuiState::new(empty_payload());

        assert_eq!(
            runtime_semantic_events(&state),
            runtime_semantic_events(&state)
        );
    }

    #[test]
    fn rollback_runtime_command_never_enters_failed_or_apply_state() {
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\nrollback\n/exit\n");
        let mut output = Vec::new();
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");

        run_runtime_loop(&mut input, &mut output, root.path().to_path_buf()).expect("loop");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("runtime idle"), "{output}");
        assert!(output.contains("no active transaction"), "{output}");
        assert!(output.contains("transaction reverted"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
        assert!(!output.contains("[ROUTE]"), "{output}");
    }

    #[test]
    fn preview_runtime_command_never_converts_to_apply_event() {
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n/exit\n");
        let mut output = Vec::new();
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");

        run_runtime_loop(&mut input, &mut output, root.path().to_path_buf()).expect("loop");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("preview ready"), "{output}");
        assert!(output.contains("transaction active"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
        assert!(!output.contains("APPLIED"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
        assert!(!output.contains("[ROUTE]"), "{output}");
    }

    #[test]
    fn preview_no_post_transition() {
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n/exit\n");
        let mut output = Vec::new();
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");

        run_runtime_loop(&mut input, &mut output, root.path().to_path_buf()).expect("loop");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("preview ready"), "{output}");
        assert!(!output.contains("runtime event consumed"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
    }

    #[test]
    fn preview_no_reducer_after_dispatch() {
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n/exit\n");
        let mut output = Vec::new();
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");

        run_runtime_loop(&mut input, &mut output, root.path().to_path_buf()).expect("loop");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("preview ready"), "{output}");
        assert!(!output.contains("[ROUTE]"), "{output}");
        assert!(!output.contains("[PROPOSAL]"), "{output}");
        assert!(!output.contains("runtime event consumed"), "{output}");
    }
}
