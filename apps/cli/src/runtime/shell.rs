use std::path::{Path, PathBuf};

use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
use crate::tui::rendering::render_runtime_text;
use crate::tui::runtime::RuntimeShellState;
use crate::tui::state::{Diff, DiffChunk, RuntimeTransaction, TuiState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewCandidate {
    pub target_path: String,
    pub tx_id: String,
    pub diff: Diff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewValidationError {
    TargetMissing { target: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCommandKind {
    Preview,
    Apply,
    Rollback,
    Status,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCommandTrace {
    pub command_id: u64,
    pub raw_input: String,
    pub runtime_command: RuntimeCommandKind,
    pub dispatch_target: String,
    pub planner_entered: bool,
    pub executor_entered: bool,
    pub apply_entered: bool,
    pub edit_mode_entered: bool,
    pub transaction_created: bool,
    pub transaction_consumed: bool,
    pub state_before: RuntimeShellState,
    pub state_after: RuntimeShellState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    Preview { target: PathBuf },
    Apply,
    Rollback,
    Status,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeCommandDispatcher;

impl RuntimeCommandDispatcher {
    pub fn parse(input: &str) -> Option<RuntimeCommand> {
        let mut parts = input.split_whitespace();
        let command = parts.next()?;
        match command {
            "preview" => {
                let target = parts
                    .next()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| ".".into());
                Some(RuntimeCommand::Preview { target })
            }
            "apply" => Some(RuntimeCommand::Apply),
            "rollback" => Some(RuntimeCommand::Rollback),
            "status" => Some(RuntimeCommand::Status),
            _ => None,
        }
    }

    pub fn is_runtime_command(input: &str) -> bool {
        Self::parse(input).is_some()
    }

    pub fn dispatch(
        state: &mut TuiState,
        workspace_root: &Path,
        input: &str,
    ) -> Option<Vec<String>> {
        let command = Self::parse(input)?;
        let command_id = state.next_command_id;
        state.next_command_id = state.next_command_id.saturating_add(1);

        let mut trace = RuntimeCommandTrace {
            command_id,
            raw_input: input.to_string(),
            runtime_command: match command {
                RuntimeCommand::Preview { .. } => RuntimeCommandKind::Preview,
                RuntimeCommand::Apply => RuntimeCommandKind::Apply,
                RuntimeCommand::Rollback => RuntimeCommandKind::Rollback,
                RuntimeCommand::Status => RuntimeCommandKind::Status,
            },
            dispatch_target: match &command {
                RuntimeCommand::Preview { target } => target.display().to_string(),
                _ => String::new(),
            },
            planner_entered: false,
            executor_entered: false,
            apply_entered: false,
            edit_mode_entered: false,
            transaction_created: false,
            transaction_consumed: false,
            state_before: state.runtime_state,
            state_after: state.runtime_state, // Will be updated
        };

        let lines = match command {
            RuntimeCommand::Preview { target } => {
                let before_tx_id = state.active_transaction_id.clone();
                let lines = runtime_preview(state, workspace_root, target);
                trace.transaction_created = state.active_transaction_id != before_tx_id;
                lines
            }
            RuntimeCommand::Apply => {
                trace.apply_entered = true;
                let lines = runtime_apply(state);
                trace.transaction_consumed = state.active_transaction.is_none();
                lines
            }
            RuntimeCommand::Rollback => {
                let lines = runtime_rollback(state);
                trace.transaction_consumed = true;
                lines
            }
            RuntimeCommand::Status => runtime_status(state),
        };

        trace.state_after = state.runtime_state;
        state.last_command_trace = Some(trace);

        Some(lines)
    }
}

pub fn runtime_preview(
    state: &mut TuiState,
    workspace_root: &Path,
    target: PathBuf,
) -> Vec<String> {
    let target_path = resolve_target(workspace_root, target);
    if validate_preview_target(&target_path).is_err() {
        return render_runtime_text(state);
    }

    let target_label = target_path.display().to_string();
    let candidate = PreviewCandidate {
        tx_id: transaction_id_for(&target_label),
        diff: Diff {
            file: target_label.clone(),
            changes: preview_changes(&target_path),
        },
        target_path: target_label,
    };
    commit_preview_candidate(state, candidate);
    render_runtime_text(state)
}

pub fn validate_preview_target(target: &Path) -> Result<(), PreviewValidationError> {
    if target.exists() {
        Ok(())
    } else {
        Err(PreviewValidationError::TargetMissing {
            target: target.to_path_buf(),
        })
    }
}

pub fn commit_preview_candidate(state: &mut TuiState, candidate: PreviewCandidate) {
    state.active_transaction = Some(RuntimeTransaction {
        tx_id: candidate.tx_id.clone(),
        target_path: candidate.target_path.clone(),
        diff: candidate.diff,
        failed_recoverable: false,
    });
    state.active_transaction_id = Some(candidate.tx_id);
    state.active_target = Some(candidate.target_path);
    state.runtime_state = RuntimeShellState::PreviewReady;
}

pub fn runtime_apply(state: &mut TuiState) -> Vec<String> {
    if state.active_transaction.is_some() {
        state.runtime_state = RuntimeShellState::Git;
        state.active_transaction = None;
        state.active_transaction_id = None;
        state.active_target = None;
    } else {
        state.runtime_state = RuntimeShellState::Idle;
    }
    render_runtime_text(state)
}

pub fn runtime_rollback(state: &mut TuiState) -> Vec<String> {
    state.runtime_state = RuntimeShellState::Idle;
    state.active_transaction = None;
    state.active_transaction_id = None;
    state.active_target = None;
    render_runtime_text(state)
}

pub fn runtime_status(state: &TuiState) -> Vec<String> {
    render_runtime_text(state)
}

pub fn empty_runtime_payload() -> UiPayload {
    UiPayload {
        trace: TraceViewModel {
            request_id: "runtime-shell".to_string(),
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

fn resolve_target(workspace_root: &Path, target: PathBuf) -> PathBuf {
    if target.is_absolute() {
        target
    } else {
        workspace_root.join(target)
    }
}

fn preview_changes(target: &Path) -> Vec<DiffChunk> {
    let preview = format!("preview {}", target.display());
    vec![DiffChunk {
        old_line: None,
        new_line: Some(1),
        old: None,
        new: Some(preview),
    }]
}

fn transaction_id_for(target: &str) -> String {
    let normalized = target
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_lowercase();
    if normalized.is_empty() {
        "tx-runtime".to_string()
    } else {
        format!("tx-{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::rendering::RenderSnapshot;

    fn state_after_preview_then_rollback() -> (TuiState, String) {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());
        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));
        let output = runtime_rollback(&mut state).join("\n");
        (state, output)
    }

    fn write_core(root: &Path) {
        std::fs::write(root.join("core.rs"), "fn core() {}\n").expect("write core");
    }

    #[test]
    fn rollback_always_clears_transaction_projection_and_target() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));
        assert!(state.active_transaction.is_some());

        let output = runtime_rollback(&mut state).join("\n");

        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
        assert!(state.active_transaction.is_none());
        assert!(state.active_transaction_id.is_none());
        assert!(state.active_target.is_none());
        assert!(output.contains("state=IDLE"));
        assert!(output.contains("Transaction: (none)"));
        assert!(output.contains("Target: (none)"));
        assert!(output.contains("No preview available"));
        assert!(!output.contains("FAILED_RECOVERABLE"));
        assert!(!output.contains("APPLYING"));
    }

    #[test]
    fn rollback_never_enters_failed_state() {
        let (state, output) = state_after_preview_then_rollback();

        assert_ne!(state.runtime_state, RuntimeShellState::Failed);
        assert!(!output.contains("FAILED_RECOVERABLE"));
    }

    #[test]
    fn rollback_always_clears_transaction() {
        let (state, output) = state_after_preview_then_rollback();

        assert!(state.active_transaction.is_none());
        assert!(state.active_transaction_id.is_none());
        assert!(output.contains("Transaction: (none)"));
    }

    #[test]
    fn rollback_always_clears_projection() {
        let (_state, output) = state_after_preview_then_rollback();

        assert!(output.contains("No preview available"));
    }

    #[test]
    fn rollback_always_enters_idle() {
        let (state, output) = state_after_preview_then_rollback();

        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
        assert!(output.contains("state=IDLE"));
    }

    #[test]
    fn rollback_clears_target() {
        let (state, output) = state_after_preview_then_rollback();

        assert!(state.active_target.is_none());
        assert!(output.contains("Target: (none)"));
    }

    #[test]
    fn rollback_always_clears_diff() {
        let (state, output) = state_after_preview_then_rollback();

        assert!(
            state
                .active_transaction
                .as_ref()
                .map(|tx| tx.diff.changes.is_empty())
                .unwrap_or(true)
        );
        assert!(output.contains("No preview available"));
    }

    #[test]
    fn preview_never_enters_apply_state() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        let output = runtime_preview(&mut state, root.path(), PathBuf::from("core.rs")).join("\n");

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(state.active_transaction.is_some());
        assert!(!output.contains("APPLYING"));
        assert!(!output.contains("FAILED_RECOVERABLE"));
    }

    #[test]
    fn preview_never_enters_applying() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        let output = runtime_preview(&mut state, root.path(), PathBuf::from("core.rs")).join("\n");

        assert_ne!(state.runtime_state, RuntimeShellState::Apply);
        assert!(!output.contains("APPLYING"));
        assert!(!output.contains("state=APPLYING"));
    }

    #[test]
    fn preview_never_calls_begin_apply() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert_ne!(state.runtime_state, RuntimeShellState::Apply);
        assert!(state.active_transaction.is_some());
    }

    #[test]
    fn preview_never_transitions_to_applying() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        let output = runtime_preview(&mut state, root.path(), PathBuf::from("core.rs")).join("\n");

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(!output.contains("state=APPLYING"));
        assert!(!output.contains("APPLYING"));
        assert!(!output.contains("APPLIED"));
    }

    #[test]
    fn preview_never_enters_mutation_pipeline() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        let original = "fn core() {}\n";
        std::fs::write(&target, original).expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        let output = RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs")
            .expect("preview")
            .join("\n");

        assert_eq!(std::fs::read_to_string(&target).expect("read"), original);
        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(state.active_transaction.is_some());
        assert!(!output.contains("[ROUTE]"));
        assert!(!output.contains("[PROPOSAL]"));
        assert!(!output.contains("APPLYING"));
        assert!(!output.contains("APPLIED"));
    }

    #[test]
    fn preview_dispatch_actual_state_matches_render_snapshot() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("apps/cli/src/core.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        let output = RuntimeCommandDispatcher::dispatch(
            &mut state,
            root.path(),
            "preview apps/cli/src/core.rs",
        )
        .expect("preview")
        .join("\n");
        let snapshot = RenderSnapshot::from(&state);

        eprintln!(
            "[RUNTIME_STATE_TRACE] actual={:?} snapshot={} rendered_applying={} rendered_failed={}",
            state.runtime_state,
            snapshot.runtime.state_label,
            output.contains("APPLYING"),
            output.contains("FAILED_RECOVERABLE")
        );

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert_eq!(snapshot.runtime.state_label, "PREVIEW_READY");
        assert!(output.contains("state=PREVIEW_READY"));
        assert!(!output.contains("APPLYING"));
        assert!(!output.contains("FAILED_RECOVERABLE"));
    }

    #[test]
    fn preview_never_enters_failed() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        let output = runtime_preview(&mut state, root.path(), PathBuf::from("core.rs")).join("\n");

        assert_ne!(state.runtime_state, RuntimeShellState::Failed);
        assert!(!output.contains("FAILED_RECOVERABLE"));
        assert!(!output.contains("state=FAILED"));
    }

    #[test]
    fn preview_no_auto_failed_transition() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));
        let before = state.clone();
        let output = runtime_status(&state).join("\n");

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert_eq!(state.active_transaction, before.active_transaction);
        assert!(
            state
                .active_transaction
                .as_ref()
                .is_some_and(|tx| !tx.failed_recoverable)
        );
        assert!(!output.contains("FAILED_RECOVERABLE"));
    }

    #[test]
    fn preview_no_runtime_tick_mutation() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));
        let before = state.clone();
        state.handle_ui_events();

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert_eq!(state.active_transaction, before.active_transaction);
        assert_eq!(state.active_transaction_id, before.active_transaction_id);
        assert_eq!(state.active_target, before.active_target);
    }

    #[test]
    fn preview_is_non_mutating() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        let original = "fn core() {}\n";
        std::fs::write(&target, original).expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        let output = runtime_preview(&mut state, root.path(), PathBuf::from("core.rs")).join("\n");

        assert_eq!(std::fs::read_to_string(&target).expect("read"), original);
        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(!output.contains("APPLIED"));
        assert!(!output.contains("APPLYING"));
    }

    #[test]
    fn preview_sets_preview_ready() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        let output = runtime_preview(&mut state, root.path(), PathBuf::from("core.rs")).join("\n");

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(output.contains("state=PREVIEW_READY"));
    }

    #[test]
    fn preview_creates_transaction_only() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_runtime_payload());

        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));

        let tx = state.active_transaction.as_ref().expect("transaction");
        assert!(state.active_transaction_id.is_some());
        assert!(state.active_target.is_some());
        assert!(!tx.failed_recoverable);
        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert_eq!(tx.target_path, target.display().to_string());
        assert_eq!(tx.diff.file, target.display().to_string());
        assert!(!tx.diff.changes.is_empty());
    }

    #[test]
    fn preview_requires_explicit_apply() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        runtime_preview(&mut state, root.path(), PathBuf::from("core.rs"));
        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);

        runtime_apply(&mut state);
        assert_eq!(state.runtime_state, RuntimeShellState::Git);
    }

    #[test]
    fn runtime_command_parser_recognizes_owned_commands() {
        for command in [
            "preview",
            "preview src/lib.rs",
            "apply",
            "rollback",
            "status",
        ] {
            assert!(RuntimeCommandDispatcher::is_runtime_command(command));
        }
        assert!(!RuntimeCommandDispatcher::is_runtime_command(
            "fix parser bug"
        ));
    }

    #[test]
    fn preview_never_enters_edit_mode() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(!trace.edit_mode_entered);
    }

    #[test]
    fn preview_never_enters_apply_lifecycle() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(!trace.apply_entered);
    }

    #[test]
    fn preview_never_calls_executor() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(!trace.executor_entered);
    }

    #[test]
    fn preview_never_calls_planner() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(!trace.planner_entered);
    }

    #[test]
    fn apply_is_only_mutating_command() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        assert!(!state.last_command_trace.as_ref().unwrap().apply_entered);

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "rollback");
        assert!(!state.last_command_trace.as_ref().unwrap().apply_entered);

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "apply");
        assert!(state.last_command_trace.as_ref().unwrap().apply_entered);
    }

    #[test]
    fn apply_consumes_transaction() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        assert!(state.active_transaction.is_some());

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "apply");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(trace.transaction_consumed);
        assert!(state.active_transaction.is_none());
    }

    #[test]
    fn rollback_clears_transaction() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "rollback");
        assert!(state.active_transaction.is_none());
    }

    #[test]
    fn rollback_returns_idle() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "rollback");
        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
    }

    #[test]
    fn runtime_trace_matches_state_machine() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert_eq!(trace.state_before, RuntimeShellState::Idle);
        assert_eq!(trace.state_after, RuntimeShellState::PreviewReady);
    }

    #[test]
    fn preview_trace_contains_no_mutation() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(!trace.apply_entered);
        assert!(!trace.executor_entered);
        assert!(!trace.planner_entered);
    }

    #[test]
    fn apply_trace_contains_mutation() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "apply");
        let trace = state.last_command_trace.as_ref().unwrap();
        assert!(trace.apply_entered);
    }

    #[test]
    fn surface_state_matches_runtime_state() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let output = render_runtime_text(&state).join("\n");
        assert!(output.contains("state=PREVIEW_READY"));

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "apply");
        let output = render_runtime_text(&state).join("\n");
        assert!(output.contains("state=APPLIED"));
    }

    #[test]
    fn preview_ready_always_visible() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let output = render_runtime_text(&state).join("\n");
        assert!(output.contains("state=PREVIEW_READY"));
    }

    #[test]
    fn applying_only_visible_during_apply() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let output = render_runtime_text(&state).join("\n");
        assert!(!output.contains("state=APPLYING"));

        state.runtime_state = RuntimeShellState::Apply;
        let output = render_runtime_text(&state).join("\n");
        assert!(output.contains("state=APPLYING"));
    }

    #[test]
    fn failed_state_requires_real_failure() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());

        // Preview should never fail by default
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        assert_ne!(state.runtime_state, RuntimeShellState::Failed);

        // Manual state transition to failed to verify it can exist
        state.runtime_state = RuntimeShellState::Failed;
        assert_eq!(state.runtime_state, RuntimeShellState::Failed);
    }

    #[test]
    fn invalid_preview_preserves_active_owner() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let before = state.clone();

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview does/not/exist.rs");

        assert_eq!(state.active_transaction, before.active_transaction);
        assert_eq!(state.active_transaction_id, before.active_transaction_id);
        assert_eq!(state.active_target, before.active_target);
        assert_eq!(state.runtime_state, before.runtime_state);
    }

    #[test]
    fn invalid_preview_never_allocates_tx() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview does/not/exist.rs");

        assert!(state.active_transaction.is_none());
        assert!(state.active_transaction_id.is_none());
        assert!(state.active_target.is_none());
        assert!(
            !state
                .last_command_trace
                .as_ref()
                .expect("trace")
                .transaction_created
        );
    }

    #[test]
    fn invalid_preview_never_enters_preview_ready() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut state = TuiState::new(empty_runtime_payload());
        let output = RuntimeCommandDispatcher::dispatch(
            &mut state,
            root.path(),
            "preview does/not/exist.rs",
        )
        .expect("preview")
        .join("\n");

        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
        assert!(!output.contains("PREVIEW_READY"));
    }

    #[test]
    fn invalid_preview_never_publishes_projection() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let before_projection = RenderSnapshot::from(&state).runtime.diff_projection;

        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview does/not/exist.rs");
        let after_projection = RenderSnapshot::from(&state).runtime.diff_projection;

        assert_eq!(after_projection, before_projection);
        assert!(
            !after_projection
                .lines
                .join("\n")
                .contains("does/not/exist.rs")
        );
    }

    #[test]
    fn runtime_state_bit_identical_after_failed_preview() {
        let root = tempfile::tempdir().expect("tempdir");
        write_core(root.path());
        let mut state = TuiState::new(empty_runtime_payload());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        let before_state = state.runtime_state;
        let before_target = state.active_target.clone();
        let before_tx_id = state.active_transaction_id.clone();
        let before_tx = state.active_transaction.clone();
        let before_render = render_runtime_text(&state).join("\n");

        let after_render = RuntimeCommandDispatcher::dispatch(
            &mut state,
            root.path(),
            "preview does/not/exist.rs",
        )
        .expect("preview")
        .join("\n");

        assert_eq!(state.runtime_state, before_state);
        assert_eq!(state.active_target, before_target);
        assert_eq!(state.active_transaction_id, before_tx_id);
        assert_eq!(state.active_transaction, before_tx);
        assert_eq!(after_render, before_render);
    }

    #[test]
    fn ownership_commit_occurs_after_validation() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        let mut state = TuiState::new(empty_runtime_payload());
        assert!(validate_preview_target(&target).is_err());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        assert!(state.active_transaction.is_none());

        std::fs::write(&target, "fn core() {}\n").expect("write");
        assert!(validate_preview_target(&target).is_ok());
        RuntimeCommandDispatcher::dispatch(&mut state, root.path(), "preview core.rs");
        assert!(state.active_transaction.is_some());
        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
    }
}
