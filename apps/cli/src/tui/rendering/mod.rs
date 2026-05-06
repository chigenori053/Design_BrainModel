use ratatui::layout::Rect;

use crate::tui::runtime::RuntimeShellState;
use crate::tui::state::{Focus, TuiState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSnapshot {
    pub runtime: RuntimeProjection,
    pub status: StatusModel,
    pub input: InputModel,
    pub focus: Focus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeProjection {
    pub state_label: String,
    pub target_label: Option<String>,
    pub transaction_label: Option<String>,
    pub diff_projection: DiffProjection,
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
        }
    }
}

impl RuntimeProjection {
    pub fn from_state(state: &TuiState) -> Self {
        let target_label = resolved_target_label(state);
        let diff_projection = DiffProjection::from_state(state, target_label.clone());
        Self {
            state_label: projection_state_label(state.runtime_state).to_string(),
            target_label,
            transaction_label: resolved_transaction_label(state),
            diff_projection,
        }
    }

    pub fn runtime_panel_lines(&self) -> Vec<String> {
        vec![
            format!("State: {}", self.state_label),
            format!(
                "Target: {}",
                self.target_label.as_deref().unwrap_or("(none)")
            ),
            format!(
                "Transaction: {}",
                self.transaction_label.as_deref().unwrap_or("(none)")
            ),
        ]
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
        let Some(diff) = state.session.diffs.last() else {
            return Self {
                target_label,
                lines: vec!["No preview available.".to_string()],
            };
        };
        let target_label = target_label.or_else(|| semantic_label(&diff.file));
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
    lines.push("| Input / Intent                                   |".to_string());
    lines.push("+-------------------+------------------------------+".to_string());
    for line in snapshot.runtime.runtime_panel_lines() {
        lines.push(format!("| {:<17} | {:<28} |", truncate(&line, 17), ""));
    }
    lines.push("+-------------------+------------------------------+".to_string());
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
        RuntimeShellState::Ready | RuntimeShellState::AwaitConfirmation => "READY_TO_APPLY",
        RuntimeShellState::Apply => "APPLYING",
        RuntimeShellState::Git => "APPLIED",
        other => other.label(),
    }
}

fn resolved_target_label(state: &TuiState) -> Option<String> {
    state
        .active_target
        .as_deref()
        .and_then(semantic_label)
        .or_else(|| {
            state
                .session
                .diffs
                .last()
                .and_then(|diff| semantic_label(&diff.file))
        })
}

fn resolved_transaction_label(state: &TuiState) -> Option<String> {
    state
        .active_transaction_id
        .as_deref()
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
        state.session.diffs.push(crate::tui::state::Diff {
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
            snapshot.runtime.runtime_panel_lines().join("\n"),
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
            input: Rect::new(0, 0, 80, 5),
            runtime: Rect::new(0, 5, 30, 18),
            diff: Rect::new(30, 5, 50, 18),
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
        state.session.diffs.push(crate::tui::state::Diff {
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
        state.session.diffs.push(crate::tui::state::Diff {
            file: "runtime.active_preview".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("fn semantic() {}".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
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
        state.active_transaction_id = Some("ActivePreview { transaction_id: \"tx\" }".to_string());
        state.session.diffs.push(crate::tui::state::Diff {
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

        let first = RuntimeProjection::from_state(&state);
        let second = RuntimeProjection::from_state(&state);

        assert_eq!(first, second);
        assert_eq!(first.state_label, "APPLYING");
        assert_eq!(first.transaction_label.as_deref(), Some("tx-apply"));
    }

    fn projection_surface(snapshot: &RenderSnapshot) -> String {
        format!(
            "{}\n{}\n{}",
            snapshot.runtime.runtime_panel_lines().join("\n"),
            snapshot.runtime.diff_projection.lines.join("\n"),
            snapshot.status.line
        )
    }
}
