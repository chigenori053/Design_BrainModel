/// Thin REPL UI for DBM_CLI.
///
/// Phase 1 boundary:
/// - REPL reads input and renders output only.
/// - Core is the only execution and reasoning entry point.
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::core::{CoreEvent, CoreExecutor, CoreRequest, RuntimeCoreBridge, to_ui_event};
use crate::pipeline::PipelineState;
use crate::session::AgentSession;
use crate::state::State;
use crate::tui::composer::ComposerViewState;
use crate::tui::state::DesignDocument;

#[derive(Debug, Clone, Default)]
struct ReplUiState {
    pipeline_state: PipelineState,
    design_snapshot: Option<DesignDocument>,
    current_proposals: Option<Vec<strategy_engine::ExecutionPlanCandidate>>,
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
    working_dir: &Path,
    core: &dyn CoreExecutor,
    ui: &mut ReplUiState,
    writer: &mut W,
) -> Result<(), String> {
    let is_select = input.trim().to_ascii_lowercase().starts_with("select ");
    let request = CoreRequest::new(
        input,
        working_dir.to_path_buf(),
        ui.pipeline_state.clone(),
        ui.design_snapshot.clone(),
        ui.current_proposals.clone(),
    );
    let response = core.execute(request);
    let success = !response
        .events
        .iter()
        .any(|event| matches!(event, CoreEvent::Error { .. }));

    for event in response.events {
        apply_core_event(ui, &event);
        eprintln!("[UI] Rendering event");
        render_core_event(writer, event)?;
    }

    if success && is_select {
        ui.current_proposals = None;
    }
    if success {
        ui.design_snapshot = response.design;
    }

    Ok(())
}

fn apply_core_event(ui: &mut ReplUiState, event: &CoreEvent) {
    match event {
        CoreEvent::Pipeline { state } => {
            if let Some(next) = pipeline_state_from_label(state) {
                ui.pipeline_state = next;
            }
        }
        CoreEvent::Proposal { candidates } => {
            ui.current_proposals = Some(candidates.clone());
        }
        _ => {}
    }
}

fn render_core_event<W: Write>(writer: &mut W, event: CoreEvent) -> Result<(), String> {
    let event = to_ui_event(event);
    for line in event.lines() {
        writeln!(writer, "{line}").map_err(|err| err.to_string())?;
    }
    Ok(())
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

fn print_banner<W: Write>(writer: &mut W) -> Result<(), String> {
    writeln!(writer, "DBM_CLI REPL").map_err(|err| err.to_string())?;
    writeln!(writer, "Type /exit to quit.").map_err(|err| err.to_string())
}

fn is_exit(input: &str) -> bool {
    matches!(input, "/exit" | "/quit" | "exit" | "quit")
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
