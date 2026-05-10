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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelCellOwner {
    Header,
    Input,
    Runtime,
    Diff,
    Status,
}

pub fn render(frame: &mut Frame, snapshot: &RenderSnapshot) {
    let area = frame.area();
    let layout = layout_for_area(area);
    let immutable = FrameComposer::compose(snapshot.clone(), layout);
    let projection = FullSurfaceProjection { frame: immutable };
    SurfaceProjector::project(frame, &projection);
}

pub fn layout_for_area(area: Rect) -> LayoutMetadata {
    let rows = layout_rows(area);
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(rows[2]);

    LayoutMetadata {
        viewport: area,
        header: rows[0],
        input: rows[1],
        runtime: middle[0],
        diff: middle[1],
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

pub fn runtime_panel_bounds(layout: &LayoutMetadata) -> Rect {
    layout.runtime
}

pub fn panel_overlap_detected(layout: &LayoutMetadata) -> bool {
    let panels = [
        layout.header,
        layout.input,
        layout.runtime,
        layout.diff,
        layout.status,
    ];
    panels.iter().enumerate().any(|(index, panel)| {
        panels
            .iter()
            .skip(index + 1)
            .any(|other| rects_overlap(*panel, *other))
    })
}

pub fn cell_ownership_map(layout: &LayoutMetadata) -> Vec<(u16, u16, PanelCellOwner)> {
    let mut cells = Vec::new();
    push_owned_cells(&mut cells, layout.header, PanelCellOwner::Header);
    push_owned_cells(&mut cells, layout.input, PanelCellOwner::Input);
    push_owned_cells(&mut cells, layout.runtime, PanelCellOwner::Runtime);
    push_owned_cells(&mut cells, layout.diff, PanelCellOwner::Diff);
    push_owned_cells(&mut cells, layout.status, PanelCellOwner::Status);
    cells
}

pub fn frame_cell_rewrite_count(area: Rect) -> usize {
    usize::from(area.width) * usize::from(area.height)
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    let a_right = a.x.saturating_add(a.width);
    let b_right = b.x.saturating_add(b.width);
    let a_bottom = a.y.saturating_add(a.height);
    let b_bottom = b.y.saturating_add(b.height);
    a.x < b_right && b.x < a_right && a.y < b_bottom && b.y < a_bottom
}

fn push_owned_cells(
    cells: &mut Vec<(u16, u16, PanelCellOwner)>,
    area: Rect,
    owner: PanelCellOwner,
) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            cells.push((x, y, owner));
        }
    }
}

pub struct SurfaceProjector;

impl SurfaceProjector {
    pub fn project(frame: &mut Frame, projection: &FullSurfaceProjection) {
        let immutable = &projection.frame;
        frame.render_widget(Clear, immutable.layout.viewport);
        render_header(frame, immutable);
        render_input(frame, immutable);
        render_runtime_state(frame, immutable);
        render_diff_preview(frame, immutable);
        render_status_line(frame, immutable);
        if let Some(cursor) = immutable.cursor {
            frame.set_cursor_position((cursor.x, cursor.y));
        }
    }
}

fn render_header(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.header;
    let snapshot = &immutable.snapshot;
    let text = format!(
        " {} | {} ",
        snapshot.identity.runtime_name, snapshot.identity.runtime_descriptor
    );
    frame.render_widget(
        Paragraph::new(text).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        area,
    );
}

fn render_runtime_state(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.runtime;
    frame.render_widget(Clear, area);
    let snapshot = &immutable.snapshot;

    let lines_vec = snapshot
        .runtime
        .runtime_panel_lines(snapshot.is_expanded);

    let has_critical = lines_vec.iter().any(|l| l.contains("[CRITICAL]"));

    let block = Block::default()
        .borders(Borders::TOP)
        .title(" Cognitive Narrative ")
        .border_style(active_border(snapshot.focus == Focus::Chat, has_critical));

    let lines = lines_vec
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_diff_preview(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.diff;
    frame.render_widget(Clear, area);
    let snapshot = &immutable.snapshot;
    let block = Block::default()
        .borders(Borders::TOP)
        .title(" Workspace Projection ")
        .border_style(active_border(snapshot.focus == Focus::Design, false));
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
    frame.render_widget(Clear, area);
    let snapshot = &immutable.snapshot;
    let block = Block::default()
        .borders(Borders::TOP)
        .title(format!(
            " Conversation / Intent [{}] ",
            snapshot.input.pipeline_label
        ))
        .border_style(active_border(snapshot.focus == Focus::Input, false));

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
    frame.render_widget(Clear, immutable.layout.status);
    frame.render_widget(
        Paragraph::new(immutable.snapshot.status.line.clone())
            .style(Style::default().fg(Color::DarkGray)),
        immutable.layout.status,
    );
}

fn active_border(active: bool, critical: bool) -> Style {
    if critical {
        Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else if active {
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
    use crate::tui::runtime::RuntimeShellState;
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

    fn without_string_literals(source: &str) -> String {
        let mut stripped = String::with_capacity(source.len());
        let mut in_string = false;
        let mut escaped = false;
        for ch in source.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                    stripped.push(' ');
                } else if ch == '\\' {
                    escaped = true;
                    stripped.push(' ');
                } else if ch == '"' {
                    in_string = false;
                    stripped.push('"');
                } else {
                    stripped.push(' ');
                }
            } else {
                if ch == '"' {
                    in_string = true;
                }
                stripped.push(ch);
            }
        }
        stripped
    }

    fn full_repaint(terminal: &mut Terminal<TestBackend>, state: &TuiState) {
        let snapshot = RenderSnapshot::from(state);
        terminal.clear().expect("clear");
        terminal
            .draw(|frame| render(frame, &snapshot))
            .expect("draw");
    }

    fn redraw_without_terminal_clear(terminal: &mut Terminal<TestBackend>, state: &TuiState) {
        let snapshot = RenderSnapshot::from(state);
        terminal
            .draw(|frame| render(frame, &snapshot))
            .expect("draw");
    }

    fn surface_after_redraw_without_terminal_clear(
        terminal: &mut Terminal<TestBackend>,
        state: &TuiState,
    ) -> String {
        redraw_without_terminal_clear(terminal, state);
        buffer_text(terminal.backend().buffer())
    }

    fn buffer_rect_row(buffer: &Buffer, terminal_width: u16, area: Rect, row: u16) -> String {
        let y = area.y.saturating_add(row);
        let start = usize::from(y) * usize::from(terminal_width) + usize::from(area.x);
        buffer
            .content()
            .iter()
            .skip(start)
            .take(usize::from(area.width))
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    fn runtime_content_row(
        buffer: &Buffer,
        terminal_width: u16,
        layout: &LayoutMetadata,
    ) -> String {
        buffer_rect_row(buffer, terminal_width, layout.runtime, 1)
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
        state.active_target = Some("old.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["PREVIOUS_FRAME_RESIDUE_SHOULD_DISAPPEAR".to_string()],
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
    fn renderer_clears_previous_runtime_frame() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("APPLYING"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains("PREVIEW_READY"));
        assert!(!surface.contains("APPLYING"));
    }

    #[test]
    fn renderer_never_reuses_old_state_text() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Git;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("APPLIED"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains("PREVIEW_READY"));
        assert!(!surface.contains("APPLIED"));
        assert!(!surface.contains("APPLYING"));
    }

    #[test]
    fn no_stale_frame_after_redraw() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let first = surface_after_redraw_without_terminal_clear(&mut terminal, &state);
        assert!(first.contains("APPLYING"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        let second = surface_after_redraw_without_terminal_clear(&mut terminal, &state);

        assert!(second.contains("PREVIEW_READY"));
        assert!(!second.contains("APPLYING"));
        assert_ne!(first, second);
    }

    #[test]
    fn runtime_panel_no_overlap() {
        let layout = layout_for_area(Rect::new(0, 0, 100, 24));
        let runtime = runtime_panel_bounds(&layout);
        let ownership = cell_ownership_map(&layout);
        let runtime_cells = ownership
            .iter()
            .filter(|(_, _, owner)| *owner == PanelCellOwner::Runtime)
            .count();
        let unique_cells = ownership
            .iter()
            .map(|(x, y, _)| (*x, *y))
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(runtime, layout.runtime);
        assert!(!panel_overlap_detected(&layout));
        assert_eq!(runtime_cells, frame_cell_rewrite_count(runtime));
        assert_eq!(unique_cells.len(), ownership.len());
    }

    #[test]
    fn shorter_runtime_state_overwrites_longer_state() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::PreviewReady;
        let width = 100;
        let height = 24;
        let layout = layout_for_area(Rect::new(0, 0, width, height));
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        let first_row = runtime_content_row(terminal.backend().buffer(), width, &layout);
        assert!(first_row.contains("PREVIEW_READY"));

        state.runtime_state = RuntimeShellState::Idle;
        redraw_without_terminal_clear(&mut terminal, &state);
        let second_row = runtime_content_row(terminal.backend().buffer(), width, &layout);

        assert!(second_row.contains("State: IDLE"));
        assert!(!second_row.contains("PREVIEW_READY"));
        assert_eq!(
            second_row.chars().count(),
            usize::from(layout.runtime.width)
        );
    }

    #[test]
    fn applying_to_preview_ready_erases_old_cells() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let width = 100;
        let height = 24;
        let layout = layout_for_area(Rect::new(0, 0, width, height));
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(
            runtime_content_row(terminal.backend().buffer(), width, &layout).contains("APPLYING")
        );

        state.runtime_state = RuntimeShellState::PreviewReady;
        redraw_without_terminal_clear(&mut terminal, &state);
        let row = runtime_content_row(terminal.backend().buffer(), width, &layout);

        assert!(row.contains("PREVIEW_READY"));
        assert!(!row.contains("APPLYING"));
        assert_eq!(row.chars().count(), usize::from(layout.runtime.width));
    }

    #[test]
    fn every_runtime_row_fully_rewritten() {
        let state = TuiState::new(empty_payload());
        let width = 100;
        let height = 24;
        let layout = layout_for_area(Rect::new(0, 0, width, height));
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);

        assert_eq!(
            frame_cell_rewrite_count(layout.runtime),
            usize::from(layout.runtime.width) * usize::from(layout.runtime.height)
        );
        for row in 0..layout.runtime.height {
            let text = buffer_rect_row(terminal.backend().buffer(), width, layout.runtime, row);
            assert_eq!(text.chars().count(), usize::from(layout.runtime.width));
        }
    }

    #[test]
    fn no_stale_cells_after_state_shrink() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::PreviewReady;
        state.active_target = Some("apps/cli/src/very_long_previous_runtime_target.rs".to_string());
        let width = 110;
        let height = 24;
        let layout = layout_for_area(Rect::new(0, 0, width, height));
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        let first = buffer_text(terminal.backend().buffer());
        assert!(first.contains("PREVIEW_READY"));

        state.runtime_state = RuntimeShellState::Idle;
        state.active_target = None;
        redraw_without_terminal_clear(&mut terminal, &state);
        let second = buffer_text(terminal.backend().buffer());
        let row = runtime_content_row(terminal.backend().buffer(), width, &layout);

        assert!(row.contains("State: IDLE"));
        assert!(!second.contains("PREVIEW_READY"));
        assert!(!second.contains("very_long_previous_runtime_target"));
        assert_eq!(row.chars().count(), usize::from(layout.runtime.width));
    }

    #[test]
    fn redraw_replaces_previous_runtime_text() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Git;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let first = surface_after_redraw_without_terminal_clear(&mut terminal, &state);
        assert!(first.contains("APPLIED"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        let second = surface_after_redraw_without_terminal_clear(&mut terminal, &state);

        assert!(second.contains("PREVIEW_READY"));
        assert!(!second.contains("APPLIED"));
        assert!(!second.contains("APPLYING"));
    }

    #[test]
    fn repaint_never_restores_old_frame() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        state.runtime_state = RuntimeShellState::PreviewReady;
        let second = surface_after_redraw_without_terminal_clear(&mut terminal, &state);
        let third = surface_after_redraw_without_terminal_clear(&mut terminal, &state);

        assert_eq!(second, third);
        assert!(third.contains("PREVIEW_READY"));
        assert!(!third.contains("APPLYING"));
    }

    #[test]
    fn terminal_surface_matches_snapshot() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::PreviewReady;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let snapshot = RenderSnapshot::from(&state);
        terminal
            .draw(|frame| render(frame, &snapshot))
            .expect("draw");
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains(&snapshot.runtime.state_label));
        assert!(surface.contains(&snapshot.status.line));
        assert!(surface.contains("No preview available."));
        assert!(!surface.contains("APPLYING"));
        assert!(!surface.contains("APPLIED"));
    }

    #[test]
    fn runtime_panel_full_redraw() {
        let source = include_str!("render.rs");
        let runtime_fn = source
            .split("fn render_runtime_state")
            .nth(1)
            .and_then(|rest| rest.split("fn render_diff_preview").next())
            .expect("runtime function source");

        assert!(runtime_fn.contains("frame.render_widget(Clear, area);"));
    }

    #[test]
    fn diff_panel_full_redraw() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("old.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["STALE_DIFF_PANEL_TEXT_SHOULD_DISAPPEAR".to_string()],
        });
        let backend = TestBackend::new(110, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("STALE_DIFF_PANEL_TEXT"));

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(!surface.contains("STALE_DIFF_PANEL_TEXT"));
        assert!(surface.contains("No preview available."));
    }

    #[test]
    fn single_runtime_render_source() {
        let rendering_source = include_str!("rendering/mod.rs");
        let render_source = without_string_literals(
            include_str!("render.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("production render source"),
        );

        assert_eq!(rendering_source.matches("format!(\"State: {}\"").count(), 1);
        assert_eq!(
            rendering_source
                .matches("pub fn runtime_panel_lines")
                .count(),
            1
        );
        assert_eq!(render_source.matches("fn render_runtime_state").count(), 1);
    }

    #[test]
    fn no_legacy_runtime_writer() {
        let panels_source = include_str!("panels/mod.rs");
        let render_source = include_str!("render.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production render source");
        let rendering_source = include_str!("rendering/mod.rs");

        assert!(!panels_source.contains("pub mod runtime"));
        assert_eq!(
            render_source.matches(".title(\" Cognitive Narrative \")").count(),
            1
        );
        assert_eq!(rendering_source.matches("State: {}").count(), 1);
    }

    #[test]
    fn runtime_panel_written_once_per_frame() {
        let source = without_string_literals(
            include_str!("render.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("production render source"),
        );
        let runtime_fn = source
            .split("fn render_runtime_state")
            .nth(1)
            .and_then(|rest| rest.split("fn render_diff_preview").next())
            .expect("runtime function source");

        assert_eq!(
            runtime_fn
                .matches("frame.render_widget(Clear, area);")
                .count(),
            1
        );
        assert_eq!(runtime_fn.matches("frame.render_widget(").count(), 2);
        assert_eq!(runtime_fn.matches("Paragraph::new(lines)").count(), 1);
        assert_eq!(runtime_fn.matches("runtime_panel_lines()").count(), 1);
    }

    #[test]
    fn no_secondary_runtime_overlay() {
        let render_source = without_string_literals(
            include_str!("render.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("production render source"),
        );
        let rendering_source = include_str!("rendering/mod.rs");
        let combined = format!("{render_source}\n{rendering_source}");

        assert_eq!(render_source.matches("runtime_panel_lines()").count(), 1);
        assert!(!combined.contains("runtime_overlay"));
        assert!(!combined.contains("Runtime Overlay"));
        assert!(!combined.contains("overlay_runtime"));
    }

    #[test]
    fn resize_full_redraw_is_deterministic_and_clean() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("wide.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["WIDE_FRAME_ONLY_TEXT".to_string()],
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
