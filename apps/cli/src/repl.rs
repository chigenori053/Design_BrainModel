/// Thin REPL UI for DBM_CLI.
///
/// Phase 1 boundary:
/// - REPL reads input and renders output only.
/// - Core is the only execution and reasoning entry point.
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::core::{
    CoreEvent, CoreExecutor, CoreRequest, CoreState, DesignDocument, RuntimeCoreBridge,
};
use crate::session::AgentSession;
use crate::state::State;
use crate::tui::composer::ComposerViewState;
use crate::tui::core::to_ui_event;

/// Thin UI cache for the REPL.  Phase 4.5: all pipeline/design/proposal state
/// lives in `core_snapshot`; this struct is just a read-only cache.
#[derive(Debug, Clone, Default)]
struct ReplUiState {
    core_snapshot: CoreState,
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
    let mut ui = ReplUiState::default();

    print_banner(writer)?;

    for line in reader.lines() {
        let input = line.map_err(|err| err.to_string())?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_exit(trimmed) {
            break;
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

        eprintln!("[UI] Input received");
        handle_submit(
            trimmed.to_string(),
            workspace_root.as_path(),
            &core,
            &mut ui,
            writer,
        )?;
        writer.flush().map_err(|err| err.to_string())?;
    }

    Ok(())
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
    let trimmed = input.trim();
    if is_exit(trimmed) {
        return Ok(true);
    }

    let workspace_root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let core = RuntimeCoreBridge::with_defaults();
    let mut ui = ReplUiState::default();
    if trimmed == "/save design" {
        save_design(
            workspace_root.as_path(),
            ui.core_snapshot.design.as_ref(),
            writer,
        )?;
        return Ok(false);
    }

    eprintln!("[UI] Input received");
    handle_submit(
        trimmed.to_string(),
        workspace_root.as_path(),
        &core,
        &mut ui,
        writer,
    )?;
    Ok(false)
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
    use crate::nl::session::ConversationState;
    use crate::planner::PlannerMode;

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
}
