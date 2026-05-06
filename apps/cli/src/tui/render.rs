use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::Focus;
use crate::tui::rendering::{
    FrameComposer, FullSurfaceProjection, ImmutableFrame, LayoutMetadata, RenderSnapshot,
};

pub fn render(frame: &mut Frame, snapshot: &RenderSnapshot) {
    let area = frame.area();
    let rows = layout_rows(area);
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(rows[1]);

    let layout = LayoutMetadata {
        viewport: area,
        input: rows[0],
        runtime: middle[0],
        diff: middle[1],
        status: rows[2],
    };
    let immutable = FrameComposer::compose(snapshot.clone(), layout);
    let projection = FullSurfaceProjection { frame: immutable };
    SurfaceProjector::project(frame, &projection);
}

fn layout_rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area)
}

pub struct SurfaceProjector;

impl SurfaceProjector {
    pub fn project(frame: &mut Frame, projection: &FullSurfaceProjection) {
        let immutable = &projection.frame;
        frame.render_widget(Clear, immutable.layout.viewport);
        render_input(frame, immutable);
        render_runtime_state(frame, immutable);
        render_diff_preview(frame, immutable);
        render_status_line(frame, immutable);
        if let Some(cursor) = immutable.cursor {
            frame.set_cursor_position((cursor.x, cursor.y));
        }
    }
}

fn render_runtime_state(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.runtime;
    let snapshot = &immutable.snapshot;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Runtime State ")
        .border_style(active_border(snapshot.focus == Focus::Chat));
    let lines = snapshot
        .runtime
        .runtime_panel_lines()
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_diff_preview(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.diff;
    let snapshot = &immutable.snapshot;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Diff / Preview ")
        .border_style(active_border(snapshot.focus == Focus::Design));
    let lines = snapshot
        .runtime
        .diff_projection
        .lines
        .iter()
        .cloned()
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_input(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.input;
    let snapshot = &immutable.snapshot;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Command / Input [{}] ",
            snapshot.input.pipeline_label
        ))
        .border_style(active_border(snapshot.focus == Focus::Input));

    let display_text = if snapshot.input.text.is_empty() {
        "> ".to_string()
    } else {
        format!("> {}", snapshot.input.text)
    };
    let paragraph = Paragraph::new(display_text)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_status_line(frame: &mut Frame, immutable: &ImmutableFrame) {
    frame.render_widget(
        Paragraph::new(immutable.snapshot.status.line.clone())
            .style(Style::default().fg(Color::DarkGray)),
        immutable.layout.status,
    );
}

fn active_border(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
    use crate::tui::state::{DiffChunk, TuiState, UiEvent};

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

    fn buffer_text(buffer: &Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    fn full_repaint(terminal: &mut Terminal<TestBackend>, state: &TuiState) {
        let snapshot = RenderSnapshot::from(state);
        terminal.clear().expect("clear");
        terminal
            .draw(|frame| render(frame, &snapshot))
            .expect("draw");
    }

    #[test]
    fn repeated_full_redraw_is_identical() {
        let state = TuiState::new(empty_payload());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        full_repaint(&mut terminal, &state);
        let first = terminal.backend().buffer().clone();
        full_repaint(&mut terminal, &state);
        let second = terminal.backend().buffer().clone();

        assert_eq!(first, second);
    }

    #[test]
    fn full_redraw_removes_previous_frame_cells() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Diff {
            file: "old.rs".to_string(),
            changes: vec![DiffChunk {
                old_line: None,
                new_line: Some(1),
                old: None,
                new: Some("PREVIOUS_FRAME_RESIDUE_SHOULD_DISAPPEAR".to_string()),
            }],
        });

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        full_repaint(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("PREVIOUS_FRAME_RESIDUE"));

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });
        full_repaint(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(!surface.contains("PREVIOUS_FRAME_RESIDUE"));
        assert!(surface.contains("No preview available."));
    }

    #[test]
    fn resize_full_redraw_is_deterministic_and_clean() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Diff {
            file: "wide.rs".to_string(),
            changes: vec![DiffChunk {
                old_line: None,
                new_line: Some(1),
                old: None,
                new: Some("WIDE_FRAME_ONLY_TEXT".to_string()),
            }],
        });

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        full_repaint(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("WIDE_FRAME_ONLY_TEXT"));

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });
        terminal.resize(Rect::new(0, 0, 50, 16)).expect("resize");
        full_repaint(&mut terminal, &state);
        let first = terminal.backend().buffer().clone();
        full_repaint(&mut terminal, &state);
        let second = terminal.backend().buffer().clone();

        assert_eq!(first, second);
        assert!(!buffer_text(&second).contains("WIDE_FRAME_ONLY_TEXT"));
    }

    #[test]
    fn telemetry_tokens_never_project_to_surface() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Diff {
            file: "trace.rs".to_string(),
            changes: vec![
                DiffChunk {
                    old_line: None,
                    new_line: Some(1),
                    old: None,
                    new: Some("[IR-TRACE] leaked".to_string()),
                },
                DiffChunk {
                    old_line: None,
                    new_line: Some(2),
                    old: None,
                    new: Some("[GRAPH] [SCORE] [CODING] leaked".to_string()),
                },
            ],
        });
        state.input.set_text("[IR-TRACE] input residue".to_string());

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        full_repaint(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        for token in ["[IR-TRACE]", "[GRAPH]", "[SCORE]", "[CODING]"] {
            assert!(!surface.contains(token), "{token} should be filtered");
        }
    }
}
