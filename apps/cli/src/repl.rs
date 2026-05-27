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
use crate::nl::normalization::{
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
    let mut spec_capture: Option<Vec<String>> = None;
    let mut pending_plan: Option<InstructionPlan> = None;

    print_banner(writer)?;

    for line in reader.lines() {
        let input = line.map_err(|err| err.to_string())?;
        let trimmed = input.trim();
        if trimmed.is_empty() && ui.core_snapshot.status != PipelineState::Previewed {
            continue;
        }
        if is_exit(trimmed) {
            break;
        }
        if trimmed == "/begin spec" {
            spec_capture = Some(Vec::new());
            writeln!(writer, "[PLAN] capture: spec").map_err(|err| err.to_string())?;
            writer.flush().map_err(|err| err.to_string())?;
            continue;
        }
        if let Some(buffer) = spec_capture.as_mut() {
            if trimmed == "/end" {
                let plan = InstructionPlan::from_spec(&buffer.join("\n"));
                for line in plan.render_lines() {
                    writeln!(writer, "{line}").map_err(|err| err.to_string())?;
                }
                pending_plan = Some(plan);
                spec_capture = None;
                writer.flush().map_err(|err| err.to_string())?;
                continue;
            }
            buffer.push(input);
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
        _ => None,
    }
}

fn should_try_runtime_intent(input: &str) -> bool {
    let lower = input.to_lowercase();
    !crate::nl::context_aware_plan_target_resolver::is_plan_only_intent(&lower)
        && !crate::nl::context_aware_plan_target_resolver::has_context_reference(&lower)
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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        String::from_utf8(output).expect("utf8")
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

    #[test]
    fn repl_preview_n_does_not_route_to_language_core() {
        let output = run_preview_confirmation_script("n");

        assert!(!output.contains("intent=Unknown"), "{output}");
        assert!(!output.contains("ClarificationRequired"), "{output}");
    }

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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("## Candidates"), "{output}");
        assert!(output.contains("Target: File("), "{output}");
        assert!(output.contains("Validation required: yes"), "{output}");
    }

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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(
            output.contains("[IR-TRACE][CONTEXT_STORE] kind=selection candidate_id=1 target=File("),
            "{output}"
        );
    }

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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("# Plan Validation"), "{output}");
        assert!(output.contains("Apply allowed: true"), "{output}");
        assert!(
            output.contains("[IR-TRACE][CONTEXT_STORE] kind=validated_plan apply_allowed=true"),
            "{output}"
        );
    }

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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("# Apply Rejected"), "{output}");
        assert!(output.contains("MissingValidatedPlan"), "{output}");
    }

    #[test]
    fn repl_workspace_root_apply_is_rejected() {
        repl_apply_without_validation_is_rejected();
    }

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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
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

    #[test]
    fn repl_two_turn_analysis_then_plan_uses_previous_context() {
        repl_two_turn_analysis_then_plan_does_not_unresolved_target();
    }

    #[test]
    fn repl_two_turn_analysis_then_plan_is_plan_only() {
        repl_two_turn_analysis_then_plan_does_not_unresolved_target();
    }

    #[test]
    fn repl_two_turn_analysis_then_plan_does_not_apply() {
        repl_two_turn_analysis_then_plan_does_not_unresolved_target();
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

        run_repl_with_core(temp.path().to_path_buf(), &mut input, &mut output, &core)
            .expect("repl");
        let output = String::from_utf8(output).expect("utf8");

        assert!(
            output.contains("transaction committed successfully"),
            "{output}"
        );
        assert!(!output.contains("no active transaction"), "{output}");
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
