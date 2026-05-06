use ratatui::layout::Rect;

use crate::tui::panels::diff::diff_panel_lines;
use crate::tui::panels::runtime::runtime_panel_lines;
use crate::tui::state::{Focus, TuiState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSnapshot {
    pub runtime: RuntimePanelModel,
    pub diff: DiffPanelModel,
    pub status: StatusModel,
    pub input: InputModel,
    pub focus: Focus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimePanelModel {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffPanelModel {
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
        Self {
            runtime: RuntimePanelModel {
                lines: sanitize_lines(runtime_panel_lines(state)),
            },
            diff: DiffPanelModel {
                lines: sanitize_lines(diff_panel_lines(state)),
            },
            status: StatusModel {
                line: sanitize_line(&state.status_line()).unwrap_or_default(),
            },
            input: InputModel {
                pipeline_label: sanitize_line(state.pipeline_state.label()).unwrap_or_default(),
                text: sanitize_line(&state.input.text).unwrap_or_default(),
                cursor: state.input.cursor.min(state.input.text.len()),
            },
            focus: state.focus,
        }
    }
}

pub fn render_runtime_text(state: &TuiState) -> Vec<String> {
    let snapshot = RenderSnapshot::from(state);
    let mut lines = Vec::new();
    lines.push("+--------------------------------------------------+".to_string());
    lines.push("| Input / Intent                                   |".to_string());
    lines.push("+-------------------+------------------------------+".to_string());
    for line in snapshot.runtime.lines {
        lines.push(format!("| {:<17} | {:<28} |", truncate(&line, 17), ""));
    }
    lines.push("+-------------------+------------------------------+".to_string());
    for line in snapshot.diff.lines {
        lines.push(format!("| {:<48} |", truncate(&line, 48)));
    }
    lines.push("+--------------------------------------------------+".to_string());
    lines.push(format!("| {} |", truncate(&snapshot.status.line, 48)));
    lines.push("+--------------------------------------------------+".to_string());
    lines
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
            snapshot.runtime.lines.join("\n"),
            snapshot.diff.lines.join("\n"),
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
}
