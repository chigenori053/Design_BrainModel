use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::core::RuntimeCoreBridge;
use crate::runtime::event_queue::RuntimeEventQueue;
use crate::runtime::logging::{emit_debug, tui_logging_isolated};
use crate::runtime::runtime_events::{DebugLevel, RenderEvent, RuntimeEvent};
use crate::runtime::runtime_state::initial_runtime_state;
use crate::tui::core::handle_submit;
use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
use crate::tui::rendering::render_runtime_text;
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

        handle_submit(
            &mut state,
            &core,
            trimmed.to_string(),
            workspace_root.clone(),
        );
        state.handle_ui_events();
        request_render(&mut events, "runtime event consumed");
        render_initial(writer, &state)?;
    }

    flush_runtime_events(writer, &events)?;
    writer.flush().map_err(|err| err.to_string())
}

fn render_initial<W: Write>(writer: &mut W, state: &TuiState) -> Result<(), String> {
    for line in render_runtime_text(state) {
        writeln!(writer, "{line}").map_err(|err| err.to_string())?;
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

        assert!(output.contains("state=IDLE"));
        assert!(output.contains("[RUNTIME][EVENT] BootstrapStarted"));
        assert!(!output.contains("[ROUTE]"));
    }

    #[test]
    fn deterministic_initial_rendering_is_stable() {
        let state = TuiState::new(empty_payload());

        assert_eq!(render_runtime_text(&state), render_runtime_text(&state));
    }
}
