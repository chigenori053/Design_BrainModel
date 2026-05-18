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
use crate::nl::normalization::normalize_runtime_input;
use crate::nl::runtime_intent::RuntimeIntent;
use crate::runtime::shell::{
    RuntimeCommandDispatcher, empty_runtime_payload, runtime_preview_from_intent,
};
use crate::session::AgentSession;
use crate::state::State;
use crate::tui::composer::ComposerViewState;
use crate::tui::core::to_ui_event;
use crate::tui::rendering::{ProjectionSnapshot, RenderSnapshot};
use crate::tui::state::TuiState;

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

        if let Some(events) =
            dispatch_normalized_runtime_intent(&mut ui, workspace_root.as_path(), trimmed)
        {
            for event in events {
                writeln!(writer, "{}", event.render()).map_err(|err| err.to_string())?;
            }
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }

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

    if let Some(events) =
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
        &core,
        &mut ui,
        writer,
    )?;
    Ok(false)
}

fn dispatch_normalized_runtime_intent(
    ui: &mut ReplUiState,
    workspace_root: &Path,
    input: &str,
) -> Option<Vec<crate::tui::state::RuntimeNarrativeEvent>> {
    let normalized = normalize_runtime_input(input)?;
    match normalized.command.intent {
        RuntimeIntent::Preview => {
            let Some(target) = normalized.command.target else {
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
            let mut events = runtime_preview_from_intent(&mut ui.runtime, workspace_root, target);
            if ui.runtime.active_transaction.is_some() {
                events.insert(
                    0,
                    crate::tui::state::RuntimeNarrativeEvent::System {
                        summary: format!("Target: {target_label}"),
                    },
                );
            }
            ui.semantic_state
                .capture_runtime_command("preview", &ui.runtime);
            Some(events)
        }
        _ => None,
    }
}

impl ReplSemanticState {
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

    #[test]
    fn rollback_bypasses_executor_and_clears_runtime_projection() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\nrollback\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("runtime idle"), "{output}");
        assert!(output.contains("no active transaction"), "{output}");
        assert!(output.contains("transaction reverted"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
    }

    #[test]
    fn preview_short_circuits_executor() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("preview ready"), "{output}");
        assert!(output.contains("transaction active"), "{output}");
        assert!(!output.contains("[PROPOSAL]"), "{output}");
        assert!(!output.contains("APPLYING"), "{output}");
        assert!(!output.contains("FAILED_RECOVERABLE"), "{output}");
    }

    #[test]
    fn preview_dispatch_terminates_pipeline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut input = io::Cursor::new("preview apps/cli/src/core.rs\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
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

    #[test]
    fn nl_runtime_intent_binds_target_to_preview_transaction() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("apps/cli/src/test_runtime.rs を生成。\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
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

    #[test]
    fn nl_runtime_intent_rejects_empty_target_preview() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("修正してください\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(core.calls(), 0);
        assert!(output.contains("[ERROR] unresolved target"), "{output}");
        assert!(!output.contains("preview ready"), "{output}");
        assert!(!output.contains("transaction active"), "{output}");
    }

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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
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

    #[test]
    fn runtime_commands_bypass_reasoning_pipeline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("status\nrollback\napply\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 0);
    }

    #[test]
    fn non_runtime_input_still_routes_to_core() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut input = io::Cursor::new("fix parser bug\n");
        let mut output = Vec::new();
        let core = CountingCore::new();

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");

        assert_eq!(core.calls(), 1);
    }
}
// DBM clarification execution guarantee
