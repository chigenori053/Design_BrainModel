use ratatui::layout::Rect;

use crate::tui::cognitive_explanation::{
    CognitiveExplanation, CognitiveNarrativeRenderer, CognitiveSeverity,
};
use crate::tui::cognitive_workspace::RuntimeIdentity;
use crate::tui::runtime::RuntimeShellState;
use crate::tui::state::{Focus, TuiState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSnapshot {
    pub runtime: RuntimeProjection,
    pub status: StatusModel,
    pub input: InputModel,
    pub focus: Focus,
    pub identity: RuntimeIdentity,
    pub is_expanded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeProjection {
    pub state_label: String,
    pub target_label: Option<String>,
    pub transaction_label: Option<String>,
    pub diff_projection: DiffProjection,
    pub rejection_label: Option<String>,
    pub explanations: Vec<CognitiveExplanation>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffProjection {
    pub target_label: Option<String>,
    pub lines: Vec<String>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutMetadata {
    pub viewport: Rect,
    pub header: Rect,
    pub input: Rect,
    pub runtime: Rect,
    pub diff: Rect,
    pub status: Rect,
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
        Self {
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

        // DBM-COGNITIVE-NARRATIVE-RENDERING Integration
        let renderer = CognitiveNarrativeRenderer::new(16);
        let state_explanation = renderer.explain_state(state.runtime_state);

        Self {
            state_label: projection_state_label_from_runtime(state),
            target_label,
            transaction_label: resolved_transaction_label(state),
            diff_projection,
            rejection_label,
            explanations: vec![state_explanation],
        }
    }

    pub fn runtime_panel_lines(&self, expanded: bool) -> Vec<String> {
        let mut lines = Vec::new();

        // Narrative-First Rendering
        for (i, explanation) in self.explanations.iter().enumerate() {
            // Severity Labeling (Spec 7.2)
            let prefix = match explanation.severity {
                CognitiveSeverity::Critical => "[CRITICAL] ",
                CognitiveSeverity::Warning => "[WARNING] ",
                CognitiveSeverity::Notice => "[NOTICE] ",
                CognitiveSeverity::Info => "",
            };

            lines.push(format!("{} [JA] {}", prefix, explanation.summary_ja));
            lines.push(format!("{} [EN] {}", prefix, explanation.summary_en));

            if expanded {
                if let Some(detail_ja) = &explanation.detail_ja {
                    lines.push(format!("  [Detail-JA] {}", detail_ja));
                }
                if let Some(detail_en) = &explanation.detail_en {
                    lines.push(format!("  [Detail-EN] {}", detail_en));
                }
                if let Some(rec_ja) = &explanation.recommendation_ja {
                    lines.push(format!("  [Action-JA] {}", rec_ja));
                }
                if let Some(rec_en) = &explanation.recommendation_en {
                    lines.push(format!("  [Action-EN] {}", rec_en));
                }
            }

            if i < self.explanations.len() - 1 || self.rejection_label.is_some() {
                lines.push(String::new());
            }
        }

        if let Some(rejection) = &self.rejection_label {
            lines.push(rejection.clone());
        }
        lines
    }

    pub fn status_line(&self) -> String {
        format!(
            "state={} tx={} target={}",
            self.state_label,
            self.transaction_label.as_deref().unwrap_or("(none)"),
            self.target_label.as_deref().unwrap_or("(none)")
        )
    }
}

impl DiffProjection {
    pub fn from_state(state: &TuiState, target_label: Option<String>) -> Self {
        let Some(transaction) = state.active_transaction.as_ref() else {
            return Self {
                target_label,
                lines: vec!["No preview available.".to_string()],
            };
        };
        let diff = &transaction.diff;
        let target_label = target_label.or_else(|| semantic_label(&transaction.target_path));
        let mut lines = Vec::new();
        if let Some(target) = target_label.as_deref() {
            lines.push(format!("Target: {target}"));
        }
        lines.push("--- preview".to_string());
        for change in &diff.changes {
            if let Some(old) = sanitize_line(change.old.as_deref().unwrap_or_default()) {
                if !old.is_empty() {
                    lines.push(format!("-{old}"));
                }
            }
            if let Some(new) = sanitize_line(change.new.as_deref().unwrap_or_default()) {
                if !new.is_empty() {
                    let line =
                        if new.starts_with('+') || new.starts_with('-') || new.starts_with(' ') {
                            new
                        } else {
                            format!("+{new}")
                        };
                    lines.push(line);
                }
            }
        }
        Self {
            target_label,
            lines: sanitize_lines(lines),
        }
    }
}

pub fn render_runtime_text(state: &TuiState) -> Vec<String> {
    let snapshot = RenderSnapshot::from(state);
    let mut lines = Vec::new();
    lines.push("+--------------------------------------------------+".to_string());
    lines.push("| DBM_CLI                                          |".to_string());
    lines.push("| Explainable Governed Cognitive Runtime           |".to_string());
    lines.push("+--------------------------------------------------+".to_string());
    lines.push("| Conversation / Intent                            |".to_string());
    lines.push("+--------------------------------------------------+".to_string());
    for line in snapshot.runtime.runtime_panel_lines(false) {
        lines.push(format!("| {:<48} |", truncate(&line, 48)));
    }
    lines.push("+--------------------------------------------------+".to_string());
    for line in snapshot.runtime.diff_projection.lines {
        lines.push(format!("| {:<48} |", truncate(&line, 48)));
    }
    lines.push("+--------------------------------------------------+".to_string());
    lines.push(format!("| {} |", truncate(&snapshot.status.line, 48)));
    lines.push("+--------------------------------------------------+".to_string());
    lines
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
        .and_then(semantic_label)
}

fn resolved_transaction_label(state: &TuiState) -> Option<String> {
    state
        .active_transaction
        .as_ref()
        .map(|tx| tx.tx_id.as_str())
        .and_then(semantic_label)
}

fn semantic_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "preview" || contains_runtime_reference(trimmed) {
        return None;
    }
    let path = std::path::Path::new(trimmed);
    let display = if path.is_absolute() {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| {
                path.strip_prefix(cwd)
                    .ok()
                    .map(|relative| relative.to_path_buf())
            })
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    sanitize_line(&display.display().to_string())
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

fn sanitize_line(line: &str) -> Option<String> {
    const BANNED_SURFACE_TOKENS: &[&str] = &[
        "[IR-TRACE]",
        "[GRAPH]",
        "[SCORE]",
        "[CODING]",
        "TRACE:",
        "[ROUTE]",
        "[EXECUTE]",
        "[ANALYZE]",
        "runtime.active_preview",
        "RuntimeState",
        "ActivePreview",
        "active_preview",
    ];
    if BANNED_SURFACE_TOKENS
        .iter()
        .any(|token| line.contains(token))
    {
        None
    } else {
        Some(line.to_string())
    }
}

fn contains_runtime_reference(line: &str) -> bool {
    [
        "runtime.active_preview",
        "RuntimeState",
        "ActivePreview",
        "active_preview",
    ]
    .iter()
    .any(|token| line.contains(token))
}

fn truncate(input: &str, width: usize) -> String {
    input.chars().take(width).collect()
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
        let layout = LayoutMetadata {
            viewport: Rect::new(0, 0, 80, 24),
            header: Rect::new(0, 0, 80, 1),
            input: Rect::new(0, 1, 80, 5),
            runtime: Rect::new(0, 6, 30, 17),
            diff: Rect::new(30, 6, 50, 17),
            status: Rect::new(0, 23, 80, 1),
        };

        let first = FrameComposer::compose(RenderSnapshot::from(&state), layout);
        let second = FrameComposer::compose(RenderSnapshot::from(&state), layout);

        assert_eq!(first, second);
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
                .iter()
                .any(|line| line == "Target: apps/cli/src/core.rs")
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
        assert_eq!(snapshot.runtime.transaction_label.as_deref(), Some("tx-42"));
        assert_eq!(
            snapshot.status.line,
            "state=READY_TO_APPLY tx=tx-42 target=apps/cli/src/core.rs"
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
        assert_eq!(first.transaction_label.as_deref(), Some("tx-apply"));
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
        assert_eq!(
            projection.diff_projection.lines,
            vec!["No preview available.".to_string()]
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
                .is_some_and(|tx| tx.starts_with("tx-"))
        );
        assert!(
            projection
                .diff_projection
                .lines
                .iter()
                .any(|line| line == "Target: apps/cli/src/core.rs")
        );
    }

    #[test]
    fn failed_without_tx_cannot_retain_diff() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Failed;

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "IDLE");
        assert_eq!(projection.transaction_label, None);
        assert_eq!(
            projection.diff_projection.lines,
            vec!["No preview available.".to_string()]
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
