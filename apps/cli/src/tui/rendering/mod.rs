use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::tui::cognitive_workspace::RuntimeIdentity;
use crate::tui::runtime::RuntimeShellState;
use crate::tui::state::{
    Focus, RuntimeNarrativeEvent, TuiState, contains_runtime_reference, sanitize_line,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSnapshot {
    pub projection: ProjectionSnapshot,
    pub runtime: RuntimeProjection,
    pub status: StatusModel,
    pub input: InputModel,
    pub focus: Focus,
    pub identity: RuntimeIdentity,
    pub is_expanded: bool,
    pub diagnostics: Option<DiagnosticModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    pub narrative: Vec<String>,
    pub workspace: WorkspaceProjectionModel,
    pub diagnostics: Option<DiagnosticModel>,
    pub runtime_state: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceProjectionModel {
    pub target: Option<String>,
    pub operation: String,
    pub status: String,
}

impl WorkspaceProjectionModel {
    pub fn from_state(state: &TuiState) -> Self {
        let target = resolved_target_label(state);
        let has_preview = state.active_transaction.is_some();
        Self {
            target,
            operation: if has_preview {
                "preview".to_string()
            } else {
                "none".to_string()
            },
            status: system_summary(&projection_state_label_from_runtime(state)),
        }
    }

    pub fn lines(&self) -> Vec<String> {
        vec![
            "Target:".to_string(),
            format!("  {}", self.target.as_deref().unwrap_or("(none)")),
            String::new(),
            "Operation:".to_string(),
            format!("  {}", self.operation),
            String::new(),
            "Status:".to_string(),
            format!("  {}", self.status),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticModel {
    pub last_event: String,
    pub last_focus: String,
    pub last_mutation: String,
    pub raw_mode: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeProjection {
    pub state_label: String,
    pub target_label: Option<String>,
    pub transaction_label: Option<String>,
    pub diff_projection: DiffProjection,
    pub rejection_label: Option<String>,
    pub narrative_lines: Vec<String>,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffProjection {
    pub target_label: Option<String>,
    pub workspace: WorkspaceProjectionModel,
    pub lines: Vec<String>,
    pub semantic_projection: Option<crate::tui::cognitive_workspace::WorkspaceSemanticProjection>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusModel {
    pub line: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputModel {
    pub pipeline_label: String,
    pub text: String,
    pub cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutMetadata {
    pub viewport: Rect,
    pub header: Rect,
    pub input: Rect,
    pub runtime: Rect,
    pub diff: Rect,
    pub diagnostics: Rect,
    pub status: Rect,
}

pub fn layout_for_area(area: Rect, show_diagnostics: bool) -> LayoutMetadata {
    let rows = layout_rows(area);

    let (middle_rect, diag_rect) = if show_diagnostics {
        let diag_width = if area.width > 100 { 40 } else { 20 };
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(diag_width)])
            .split(rows[2]);
        (cols[0], cols[1])
    } else {
        (rows[2], Rect::new(0, 0, 0, 0))
    };

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(middle_rect);

    LayoutMetadata {
        viewport: area,
        header: rows[0],
        input: rows[1],
        runtime: middle[0],
        diff: middle[1],
        diagnostics: diag_rect,
        status: rows[3],
    }
}

fn layout_rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorModel {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmutableFrame {
    pub snapshot: RenderSnapshot,
    pub layout: LayoutMetadata,
    pub cursor: Option<CursorModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullSurfaceProjection {
    pub frame: ImmutableFrame,
}

pub struct FrameComposer;

impl FrameComposer {
    pub fn compose(snapshot: RenderSnapshot, layout: LayoutMetadata) -> ImmutableFrame {
        let cursor = cursor_model(&snapshot, layout.input);
        ImmutableFrame {
            snapshot,
            layout,
            cursor,
        }
    }
}

impl From<&TuiState> for RenderSnapshot {
    fn from(state: &TuiState) -> Self {
        let runtime = RuntimeProjection::from_state(state);
        let diagnostics = if state.diagnostic_mode {
            Some(DiagnosticModel {
                last_event: state
                    .diagnostics
                    .last_event
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                last_focus: state
                    .diagnostics
                    .last_focus_transition
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", state.focus)),
                last_mutation: state
                    .diagnostics
                    .last_mutation
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                raw_mode: state.diagnostics.raw_mode_active,
            })
        } else {
            None
        };
        let projection = ProjectionSnapshot {
            narrative: runtime.narrative_lines.clone(),
            workspace: runtime.diff_projection.workspace.clone(),
            diagnostics: diagnostics.clone(),
            runtime_state: runtime.state_label.clone(),
        };
        Self {
            projection,
            status: StatusModel {
                line: runtime.status_line(),
            },
            runtime,
            input: InputModel {
                pipeline_label: sanitize_line(state.pipeline_state.label()).unwrap_or_default(),
                text: sanitize_line(&state.input.text).unwrap_or_default(),
                cursor: state.input.cursor.min(state.input.text.len()),
            },
            focus: state.focus,
            identity: RuntimeIdentity::default(),
            is_expanded: state.narrative_expanded,
            diagnostics,
        }
    }
}

impl RuntimeProjection {
    pub fn from_state(state: &TuiState) -> Self {
        let target_label = resolved_target_label(state);
        let diff_projection = DiffProjection::from_state(state, target_label.clone());
        let rejection_label = state.rejection.as_ref().map(|rej| {
            format!(
                "REJECTED: {} (via {})",
                rej.reason, rej.originating_mutation
            )
        });

        let mut projection = Self {
            state_label: projection_state_label_from_runtime(state),
            target_label,
            transaction_label: resolved_transaction_label(state),
            diff_projection,
            rejection_label,
            narrative_lines: Vec::new(),
            scroll_offset: state.chat_scroll.offset,
        };
        projection.narrative_lines =
            RuntimeNarrativeReducer::render(runtime_semantic_events_from_projection(&projection));
        projection
    }

    pub fn runtime_panel_lines(&self, _expanded: bool) -> Vec<String> {
        self.narrative_lines.clone()
    }

    pub fn status_line(&self) -> String {
        format!(
            "state={} tx={} target={}",
            system_summary(&self.state_label),
            self.transaction_label.as_deref().unwrap_or("(none)"),
            self.target_label.as_deref().unwrap_or("(none)")
        )
    }
}

impl DiffProjection {
    pub fn from_state(state: &TuiState, target_label: Option<String>) -> Self {
        let workspace = WorkspaceProjectionModel::from_state(state);
        let Some(transaction) = state.active_transaction.as_ref() else {
            return Self {
                target_label,
                workspace: workspace.clone(),
                lines: workspace.lines(),
                semantic_projection: None,
            };
        };

        // DBM-WORKSPACE-SEMANTIC-PROJECTION Integration
        let engine = crate::tui::cognitive_workspace::WorkspaceSemanticProjectionEngine {
            analyzer: crate::tui::cognitive_workspace::WorkspaceSemanticAnalyzer,
            classifier: crate::tui::cognitive_workspace::SemanticImpactClassifier,
            narrative_renderer: crate::tui::cognitive_workspace::WorkspaceNarrativeRenderer,
        };
        let semantic_projection = Some(engine.project_impact(&transaction.target_path));

        let target_label = target_label.or_else(|| semantic_label(&transaction.target_path));
        Self {
            target_label,
            workspace: workspace.clone(),
            lines: sanitize_lines(workspace.lines()),
            semantic_projection,
        }
    }
}

pub struct RuntimeNarrativeReducer;

impl RuntimeNarrativeReducer {
    pub fn render(events: Vec<RuntimeNarrativeEvent>) -> Vec<String> {
        let mut lines = Vec::new();
        for event in events {
            let line = render_narrative_event(normalize_narrative_event(event));
            if lines.last() != Some(&line) {
                lines.push(line);
            }
        }
        lines
    }
}

pub fn runtime_semantic_events(state: &TuiState) -> Vec<RuntimeNarrativeEvent> {
    let projection = RuntimeProjection::from_state(state);
    runtime_semantic_events_from_projection(&projection)
}

fn runtime_semantic_events_from_projection(
    projection: &RuntimeProjection,
) -> Vec<RuntimeNarrativeEvent> {
    let mut events = Vec::new();

    events.push(RuntimeNarrativeEvent::Intent {
        summary: intent_summary(projection),
    });
    events.push(RuntimeNarrativeEvent::Thinking {
        summary: "resolving target graph".to_string(),
    });
    events.push(RuntimeNarrativeEvent::Analysis {
        summary: analysis_summary(projection),
    });
    events.push(RuntimeNarrativeEvent::Validation {
        summary: validation_summary(projection),
    });
    events.push(RuntimeNarrativeEvent::Execution {
        summary: execution_summary(projection),
    });
    if projection.state_label == "APPLIED" {
        events.push(RuntimeNarrativeEvent::Apply {
            summary: "transaction committed successfully".to_string(),
        });
    }
    if let Some(rejection) = projection.rejection_label.clone() {
        events.push(RuntimeNarrativeEvent::GovernanceReject { reason: rejection });
    }
    events.push(RuntimeNarrativeEvent::System {
        summary: system_summary(&projection.state_label),
    });

    events
}

#[deprecated(note = "Use runtime_semantic_events instead")]
pub fn render_runtime_text(state: &TuiState) -> Vec<String> {
    runtime_semantic_events(state)
        .into_iter()
        .map(|e| e.render())
        .collect()
}

fn intent_summary(projection: &RuntimeProjection) -> String {
    match projection.state_label.as_str() {
        "APPLIED" => "applying active transaction".to_string(),
        "PREVIEW_READY" | "READY_TO_APPLY" | "AWAITING_APPLY" => {
            "preparing governed transaction".to_string()
        }
        _ => "checking runtime state".to_string(),
    }
}

fn analysis_summary(projection: &RuntimeProjection) -> String {
    if projection.target_label.is_some() {
        "diff structure computed".to_string()
    } else {
        "runtime state assessed".to_string()
    }
}

fn validation_summary(projection: &RuntimeProjection) -> String {
    if projection.rejection_label.is_some() {
        "governance boundary evaluated".to_string()
    } else if projection.transaction_label.is_some() {
        "transaction checksum verified".to_string()
    } else {
        "runtime invariants verified".to_string()
    }
}

fn execution_summary(projection: &RuntimeProjection) -> String {
    if projection.transaction_label.is_some() {
        "transaction active".to_string()
    } else {
        "no active transaction".to_string()
    }
}

fn system_summary(state_label: &str) -> String {
    match state_label {
        "IDLE" => "runtime idle".to_string(),
        "PREVIEW_READY" | "READY_TO_APPLY" | "AWAITING_APPLY" => "preview ready".to_string(),
        "APPLIED" => "runtime stabilized".to_string(),
        "APPLYING" => "mutation in progress".to_string(),
        "FAILED_RECOVERABLE" => "runtime recovery available".to_string(),
        "RUNAWAY_COGNITION_HALT" => "governance halt active".to_string(),
        _ => "runtime state projected".to_string(),
    }
}

fn projection_state_label(state: RuntimeShellState) -> &'static str {
    match state {
        RuntimeShellState::PreviewReady => "PREVIEW_READY",
        RuntimeShellState::AwaitingApply => "AWAITING_APPLY",
        RuntimeShellState::Ready | RuntimeShellState::AwaitConfirmation => "READY_TO_APPLY",
        RuntimeShellState::Apply => "APPLYING",
        RuntimeShellState::Git => "APPLIED",
        other => other.label(),
    }
}

fn projection_state_label_from_runtime(state: &TuiState) -> String {
    if state.runtime_state == RuntimeShellState::Failed {
        if state
            .active_transaction
            .as_ref()
            .is_some_and(|tx| tx.failed_recoverable && !tx.tx_id.is_empty())
        {
            return "FAILED_RECOVERABLE".to_string();
        }
        return "IDLE".to_string();
    }
    projection_state_label(state.runtime_state).to_string()
}

fn resolved_target_label(state: &TuiState) -> Option<String> {
    state
        .active_transaction
        .as_ref()
        .map(|tx| tx.target_path.as_str())
        .or(state.active_target.as_deref())
        .and_then(semantic_label)
}

fn resolved_transaction_label(state: &TuiState) -> Option<String> {
    if state.active_transaction.is_some() || state.active_transaction_id.is_some() {
        Some("transaction active".to_string())
    } else {
        None
    }
}

fn semantic_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "preview" || contains_runtime_reference(trimmed) {
        return None;
    }
    let path = std::path::Path::new(trimmed);
    let display = if path.is_absolute() {
        semantic_workspace_relative_path(path)
            .or_else(|| {
                std::env::current_dir().ok().and_then(|cwd| {
                    path.strip_prefix(cwd)
                        .ok()
                        .map(|relative| relative.to_path_buf())
                })
            })
            .unwrap_or_else(|| compressed_absolute_path(path))
    } else {
        path.to_path_buf()
    };
    sanitize_line(&display.display().to_string())
}

fn semantic_workspace_relative_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let components = path.components().collect::<Vec<_>>();
    for anchor in ["apps", "crates", "docs", "specs", "tests"] {
        if let Some(idx) = components
            .iter()
            .position(|component| component.as_os_str() == anchor)
        {
            return Some(components[idx..].iter().collect());
        }
    }
    None
}

fn compressed_absolute_path(path: &std::path::Path) -> std::path::PathBuf {
    let components = path.components().collect::<Vec<_>>();
    let start = components.len().saturating_sub(3);
    components[start..].iter().collect()
}

fn cursor_model(snapshot: &RenderSnapshot, input_area: Rect) -> Option<CursorModel> {
    if snapshot.focus != Focus::Input {
        return None;
    }
    let inner_x = input_area.x.saturating_add(1);
    let inner_y = input_area.y.saturating_add(1);
    let inner_width = input_area.width.saturating_sub(2);
    let inner_height = input_area.height.saturating_sub(2);
    let (row, col) = input_cursor_position(&snapshot.input.text, snapshot.input.cursor);
    let x = inner_x.saturating_add(2).saturating_add(col as u16);
    let y = inner_y.saturating_add(row as u16);
    if col as u16 >= inner_width || row as u16 >= inner_height {
        return None;
    }
    Some(CursorModel { x, y })
}

fn input_cursor_position(text: &str, cursor: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;
    for ch in text[..cursor.min(text.len())].chars() {
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (row, col)
}

fn sanitize_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter_map(|line| sanitize_line(&line))
        .collect()
}

fn normalize_narrative_event(event: RuntimeNarrativeEvent) -> RuntimeNarrativeEvent {
    match event {
        RuntimeNarrativeEvent::Intent { summary }
            if summary == "observing runtime cognition" || summary == "checking runtime state" =>
        {
            RuntimeNarrativeEvent::Intent {
                summary: "checking runtime state".to_string(),
            }
        }
        RuntimeNarrativeEvent::Planning { summary } => RuntimeNarrativeEvent::Thinking { summary },
        RuntimeNarrativeEvent::Preview { .. } => RuntimeNarrativeEvent::Execution {
            summary: "preparing transaction preview".to_string(),
        },
        RuntimeNarrativeEvent::Commit { summary } => RuntimeNarrativeEvent::Apply { summary },
        RuntimeNarrativeEvent::Error { message } => RuntimeNarrativeEvent::System {
            summary: format!("runtime error: {message}"),
        },
        other => other,
    }
}

fn render_narrative_event(event: RuntimeNarrativeEvent) -> String {
    match event {
        RuntimeNarrativeEvent::Intent { summary } => format!("[INTENT] {summary}"),
        RuntimeNarrativeEvent::Thinking { summary } => format!("[THINKING] {summary}"),
        RuntimeNarrativeEvent::Analysis { summary } => format!("[ANALYSIS] {summary}"),
        RuntimeNarrativeEvent::Planning { summary } => format!("[THINKING] {summary}"),
        RuntimeNarrativeEvent::Validation { summary } => format!("[VALIDATION] {summary}"),
        RuntimeNarrativeEvent::Execution { summary } => format!("[EXECUTION] {summary}"),
        RuntimeNarrativeEvent::Preview { target } => {
            format!("[EXECUTION] preparing transaction preview for {target}")
        }
        RuntimeNarrativeEvent::Apply { summary } | RuntimeNarrativeEvent::Commit { summary } => {
            format!("[APPLY] {summary}")
        }
        RuntimeNarrativeEvent::Rollback { summary } => format!("[ROLLBACK] {summary}"),
        RuntimeNarrativeEvent::System { summary } => format!("[SYSTEM] {summary}"),
        RuntimeNarrativeEvent::GovernanceReject { reason } => format!("[REJECT] {reason}"),
        RuntimeNarrativeEvent::Error { message } => format!("[SYSTEM] runtime error: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
    use crate::tui::state::{DiffChunk, UiEvent};

    fn empty_payload() -> UiPayload {
        UiPayload {
            trace: TraceViewModel {
                request_id: "render-test".to_string(),
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

    fn event_lines(state: &TuiState) -> Vec<String> {
        runtime_semantic_events(state)
            .into_iter()
            .map(|event| match event {
                RuntimeNarrativeEvent::Intent { summary } => format!("[INTENT] {summary}"),
                RuntimeNarrativeEvent::Thinking { summary } => format!("[THINKING] {summary}"),
                RuntimeNarrativeEvent::Analysis { summary } => format!("[ANALYSIS] {summary}"),
                RuntimeNarrativeEvent::Planning { summary } => format!("[PLANNING] {summary}"),
                RuntimeNarrativeEvent::Validation { summary } => {
                    format!("[VALIDATION] {summary}")
                }
                RuntimeNarrativeEvent::Execution { summary } => format!("[EXECUTION] {summary}"),
                RuntimeNarrativeEvent::Preview { target } => {
                    format!("[PREVIEW] changes prepared for {target}")
                }
                RuntimeNarrativeEvent::Apply { summary }
                | RuntimeNarrativeEvent::Commit { summary } => format!("[APPLY] {summary}"),
                RuntimeNarrativeEvent::Rollback { summary } => format!("[ROLLBACK] {summary}"),
                RuntimeNarrativeEvent::System { summary } => format!("[SYSTEM] {summary}"),
                RuntimeNarrativeEvent::GovernanceReject { reason } => format!("[REJECT] {reason}"),
                RuntimeNarrativeEvent::Error { message } => format!("[ERROR] {message}"),
            })
            .collect()
    }

    #[test]
    fn next_events_are_normalized_to_intent() {
        let apply = UiEvent::Next {
            actions: vec!["apply".to_string()],
        }
        .lines();

        assert_eq!(apply, vec!["[INTENT] applying active transaction"]);
        assert!(!apply.join("\n").contains("[NEXT]"));
    }

    #[test]
    fn governance_rejections_are_projected() {
        let mut state = TuiState::new(empty_payload());
        state.rejection = Some(crate::tui::state::RejectionInfo {
            reason: "target outside workspace boundary".to_string(),
            originating_mutation: "workspace_boundary".to_string(),
            governance_source: Some("workspace".to_string()),
            convergence_source: None,
        });

        let lines = event_lines(&state);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("[VALIDATION] governance boundary evaluated"))
        );
        assert!(lines.iter().any(|line| {
            line.contains("[REJECT]") && line.contains("target outside workspace boundary")
        }));
    }

    #[test]
    fn thinking_analysis_events_are_visible() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/main.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn main() {}".to_string()],
        });

        let lines = event_lines(&state);

        assert!(lines.contains(&"[THINKING] resolving target graph".to_string()));
        assert!(lines.contains(&"[ANALYSIS] diff structure computed".to_string()));
    }

    #[test]
    fn apply_projection_is_semantically_visible() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Git;

        let lines = event_lines(&state);

        assert!(lines.contains(&"[APPLY] transaction committed successfully".to_string()));
        assert!(lines.contains(&"[SYSTEM] runtime stabilized".to_string()));
    }

    #[test]
    fn semantic_narrative_order_is_stable() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/main.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn main() {}".to_string()],
        });
        state.runtime_state = RuntimeShellState::AwaitConfirmation;

        let first = event_lines(&state);
        let second = event_lines(&state);
        let categories = first
            .iter()
            .map(|line| line.split(']').next().unwrap_or_default().to_string() + "]")
            .collect::<Vec<_>>();

        assert_eq!(first, second);
        assert_eq!(
            categories,
            vec![
                "[INTENT]",
                "[THINKING]",
                "[ANALYSIS]",
                "[VALIDATION]",
                "[EXECUTION]",
                "[SYSTEM]",
            ]
        );
    }

    #[test]
    fn runtime_internal_state_is_not_exposed() {
        let mut state = TuiState::new(empty_payload());
        let absolute_target = std::env::current_dir()
            .expect("cwd")
            .join("apps/cli/src/main.rs");
        state.active_target = Some(absolute_target.display().to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn main() {}".to_string()],
        });

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("[NEXT]"));
        assert!(!surface.contains("[RUNTIME] status:"));
        assert!(!surface.contains("status: IDLE"));
        assert!(!surface.contains("PREVIEW_READY"));
        assert!(!surface.contains("tx-users-chigenori-development"));
        assert!(!surface.contains("/Users/chigenori/development"));
        assert!(surface.contains("[EXECUTION] transaction active"));
        assert!(surface.contains("apps/cli/src/main.rs"));
    }

    #[test]
    fn render_snapshot_excludes_debug_and_trace_residue() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Debug {
            message: "[IR-TRACE] leaked".to_string(),
        });
        state.append_chat(UiEvent::Diff {
            file: "target.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("[GRAPH] stale".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let snapshot = RenderSnapshot::from(&state);
        let surface = format!(
            "{}\n{}\n{}\n{}",
            snapshot.runtime.runtime_panel_lines(false).join("\n"),
            snapshot.runtime.diff_projection.lines.join("\n"),
            snapshot.status.line,
            snapshot.input.text
        );

        assert!(!surface.contains("[IR-TRACE]"));
        assert!(!surface.contains("[GRAPH]"));
        assert!(!surface.contains("[SCORE]"));
        assert!(!surface.contains("[CODING]"));
    }

    #[test]
    fn immutable_frame_composition_is_repeatable() {
        let state = TuiState::new(empty_payload());
        let layout = layout_for_area(Rect::new(0, 0, 80, 24), false);

        let first = FrameComposer::compose(RenderSnapshot::from(&state), layout);
        let second = FrameComposer::compose(RenderSnapshot::from(&state), layout);

        assert_eq!(first, second);
    }

    #[test]
    fn runtime_panel_shows_runtime_events() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::System {
            summary: "runtime idle".to_string(),
        });

        let snapshot = RenderSnapshot::from(&state);
        let lines = snapshot.runtime.runtime_panel_lines(false);
        assert!(lines.iter().any(|l| l.contains("[SYSTEM] runtime idle")));
    }

    #[test]
    fn projection_never_displays_runtime_reference() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("runtime.active_preview".to_string());
        state.append_chat(UiEvent::Diff {
            file: "runtime.active_preview".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("ActivePreview { target_path: \"apps/cli/src/core.rs\" }".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let snapshot = RenderSnapshot::from(&state);
        let surface = projection_surface(&snapshot);

        assert!(!surface.contains("runtime.active_preview"));
        assert!(!surface.contains("ActivePreview"));
        assert!(!surface.contains("target_path"));
    }

    #[test]
    fn target_projection_uses_resolved_path() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(
            projection.target_label.as_deref(),
            Some("apps/cli/src/core.rs")
        );
        assert!(
            projection
                .diff_projection
                .lines
                .windows(2)
                .any(|lines| lines == ["Target:", "  apps/cli/src/core.rs"])
        );
    }

    #[test]
    fn renderer_uses_projection_only() {
        let renderer_source = include_str!("../render.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        assert!(!renderer_source.contains("TuiState"));
        assert!(!renderer_source.contains("state.runtime_state"));
        assert!(!renderer_source.contains("active_target"));
        assert!(!renderer_source.contains("active_transaction_id"));
        assert!(!renderer_source.contains("active_preview"));
    }

    #[test]
    fn runtime_snapshot_is_semantic() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::AwaitConfirmation;
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.active_transaction_id = Some("tx-42".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });
        state.runtime_state = RuntimeShellState::AwaitConfirmation;

        let snapshot = RenderSnapshot::from(&state);

        assert_eq!(snapshot.runtime.state_label, "READY_TO_APPLY");
        assert_eq!(
            snapshot.runtime.target_label.as_deref(),
            Some("apps/cli/src/core.rs")
        );
        assert_eq!(
            snapshot.runtime.transaction_label.as_deref(),
            Some("transaction active")
        );
        assert_eq!(
            snapshot.status.line,
            "state=preview ready tx=transaction active target=apps/cli/src/core.rs"
        );
    }

    #[test]
    fn debug_string_not_rendered() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("RuntimeState { active_preview: Some(..) }".to_string());
        state.append_chat(UiEvent::Diff {
            file: "apps/cli/src/core.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("RuntimeState { active_preview: Some(..) }".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let snapshot = RenderSnapshot::from(&state);
        let surface = projection_surface(&snapshot);

        assert!(!surface.contains("RuntimeState"));
        assert!(!surface.contains("active_preview"));
        assert!(!surface.contains("ActivePreview"));
    }

    #[test]
    fn projection_normalization_deterministic() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.active_transaction_id = Some("tx-apply".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });
        state.runtime_state = RuntimeShellState::Apply;

        let first = RuntimeProjection::from_state(&state);
        let second = RuntimeProjection::from_state(&state);

        assert_eq!(first, second);
        assert_eq!(first.state_label, "APPLYING");
        assert_eq!(
            first.transaction_label.as_deref(),
            Some("transaction active")
        );
    }

    #[test]
    fn tx_none_projection_is_destroyed() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Diff {
            file: "apps/cli/src/core.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("fn semantic() {}".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });
        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "IDLE");
        assert_eq!(projection.transaction_label, None);
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"Target:".to_string())
        );
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"  (none)".to_string())
        );
    }

    #[test]
    fn failed_recoverable_retains_projection_only_with_tx() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });
        state.append_chat(UiEvent::Diff {
            file: "apps/cli/src/core.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("fn semantic() {}".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });
        state.runtime_state = RuntimeShellState::Apply;
        state.append_chat(UiEvent::Error {
            message: "recoverable".to_string(),
        });

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "FAILED_RECOVERABLE");
        assert!(
            projection
                .transaction_label
                .as_deref()
                .is_some_and(|tx| tx == "transaction active")
        );
        assert!(
            projection
                .diff_projection
                .lines
                .windows(2)
                .any(|lines| lines == ["Target:", "  apps/cli/src/core.rs"])
        );
    }

    #[test]
    fn failed_without_tx_cannot_retain_diff() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Failed;

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "IDLE");
        assert_eq!(projection.transaction_label, None);
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"Target:".to_string())
        );
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"  (none)".to_string())
        );
    }

    #[test]
    fn absolute_paths_are_not_projected() {
        let mut state = TuiState::new(empty_payload());
        let target = std::env::current_dir()
            .expect("cwd")
            .join("apps/cli/src/main.rs");
        state.active_target = Some(target.display().to_string());

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("/Users/"));
        assert!(surface.contains("apps/cli/src/main.rs"));
    }

    #[test]
    fn transaction_internal_ids_are_hidden() {
        let mut state = TuiState::new(empty_payload());
        state.active_transaction_id = Some("tx-users-secret-runtime-token".to_string());

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("tx-users-secret-runtime-token"));
        assert!(surface.contains("transaction active"));
    }

    #[test]
    fn workspace_projection_is_semantic() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/main.rs".to_string());

        let projection = RenderSnapshot::from(&state).projection.workspace;

        assert_eq!(projection.target.as_deref(), Some("apps/cli/src/main.rs"));
        assert_eq!(projection.operation, "none");
        assert_eq!(projection.status, "runtime idle");
    }

    #[test]
    fn workspace_projection_has_no_transport_leaks() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("runtime.active_preview".to_string());
        state.append_chat(UiEvent::Diff {
            file: "/Users/chigenori/development/Design_BrainModel/apps/cli/src/main.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some(
                    "+preview /Users/chigenori/development/Design_BrainModel/apps/cli/src/main.rs"
                        .to_string(),
                ),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("+preview /Users/"));
        assert!(!surface.contains("raw diff"));
        assert!(!surface.contains("runtime.active_preview"));
    }

    #[test]
    fn duplicate_runtime_idle_is_collapsed() {
        let events = vec![
            RuntimeNarrativeEvent::System {
                summary: "runtime idle".to_string(),
            },
            RuntimeNarrativeEvent::System {
                summary: "runtime idle".to_string(),
            },
        ];

        assert_eq!(
            RuntimeNarrativeReducer::render(events),
            vec!["[SYSTEM] runtime idle".to_string()]
        );
    }

    #[test]
    fn semantic_echoes_are_reduced() {
        let events = vec![RuntimeNarrativeEvent::Intent {
            summary: "observing runtime cognition".to_string(),
        }];

        assert_eq!(
            RuntimeNarrativeReducer::render(events),
            vec!["[INTENT] checking runtime state".to_string()]
        );
    }

    #[test]
    fn projection_snapshot_is_atomic() {
        let state = TuiState::new(empty_payload());
        let snapshot = RenderSnapshot::from(&state);

        assert_eq!(
            snapshot.projection.narrative,
            snapshot.runtime.narrative_lines
        );
        assert_eq!(
            snapshot.projection.workspace,
            snapshot.runtime.diff_projection.workspace
        );
        assert_eq!(
            snapshot.projection.runtime_state,
            snapshot.runtime.state_label
        );
    }

    #[test]
    fn projection_order_is_deterministic() {
        let state = TuiState::new(empty_payload());

        assert_eq!(
            RenderSnapshot::from(&state).projection,
            RenderSnapshot::from(&state).projection
        );
    }

    fn projection_surface(snapshot: &RenderSnapshot) -> String {
        format!(
            "{}\n{}\n{}",
            snapshot.runtime.runtime_panel_lines(false).join("\n"),
            snapshot.runtime.diff_projection.lines.join("\n"),
            snapshot.status.line
        )
    }
}
