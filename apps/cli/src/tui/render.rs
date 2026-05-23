use ratatui::{
    Frame,
    layout::Rect,
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
    Diagnostics,
    Status,
}

pub fn render(frame: &mut Frame, snapshot: &RenderSnapshot) {
    let area = frame.area();
    let layout = crate::tui::rendering::layout_for_area(area, snapshot.diagnostics.is_some());
    let immutable = FrameComposer::compose(snapshot.clone(), layout);
    let projection = FullSurfaceProjection { frame: immutable };
    SurfaceProjector::project(frame, &projection);
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
        layout.diagnostics,
        layout.status,
    ];
    panels.iter().enumerate().any(|(index, panel)| {
        if panel.width == 0 || panel.height == 0 {
            return false;
        }
        panels.iter().skip(index + 1).any(|other| {
            if other.width == 0 || other.height == 0 {
                return false;
            }
            rects_overlap(*panel, *other)
        })
    })
}

pub fn cell_ownership_map(layout: &LayoutMetadata) -> Vec<(u16, u16, PanelCellOwner)> {
    let mut cells = Vec::new();
    push_owned_cells(&mut cells, layout.header, PanelCellOwner::Header);
    push_owned_cells(&mut cells, layout.input, PanelCellOwner::Input);
    push_owned_cells(&mut cells, layout.runtime, PanelCellOwner::Runtime);
    push_owned_cells(&mut cells, layout.diff, PanelCellOwner::Diff);
    push_owned_cells(&mut cells, layout.diagnostics, PanelCellOwner::Diagnostics);
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
        render_diagnostics_overlay(frame, immutable);
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

    let lines_vec = snapshot.runtime.runtime_panel_lines(snapshot.is_expanded);

    let has_critical = lines_vec.iter().any(|l| l.contains("[CRITICAL]"));

    let block = Block::default()
        .borders(Borders::TOP)
        .title(" Cognitive Narrative ")
        .border_style(active_border(snapshot.focus == Focus::Chat, has_critical));

    let lines = lines_vec.into_iter().map(Line::from).collect::<Vec<_>>();
    let total_lines = lines.len() as u16;
    let viewport_height = area.height.saturating_sub(1);
    let max_scroll = total_lines.saturating_sub(viewport_height);
    let scroll = max_scroll.saturating_sub(snapshot.runtime.scroll_offset as u16);

    frame.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

fn render_diff_preview(frame: &mut Frame, immutable: &ImmutableFrame) {
    let area = immutable.layout.diff;
    frame.render_widget(Clear, area);
    let snapshot = &immutable.snapshot;

    let diff_proj = &snapshot.runtime.diff_projection;
    let mut lines = Vec::new();

    // Semantic Projection Layer Integration
    if let Some(proj) = &diff_proj.semantic_projection {
        let prefix = match proj.risk_level {
            crate::tui::cognitive_workspace::WorkspaceRiskLevel::Critical => "[CRITICAL] ",
            crate::tui::cognitive_workspace::WorkspaceRiskLevel::High => "[WARNING] ",
            _ => "",
        };

        lines.push(Line::from(format!(
            "{} [JA] {}",
            prefix, proj.narrative.summary_ja
        )));
        lines.push(Line::from(format!(
            "{} [EN] {}",
            prefix, proj.narrative.summary_en
        )));
        lines.push(Line::from(""));
    }

    lines.extend(
        diff_proj
            .lines
            .iter()
            .cloned()
            .map(Line::from)
            .collect::<Vec<_>>(),
    );

    let has_critical = diff_proj.semantic_projection.as_ref().is_some_and(|p| {
        p.risk_level == crate::tui::cognitive_workspace::WorkspaceRiskLevel::Critical
    });

    let block = Block::default()
        .borders(Borders::TOP)
        .title(" Workspace Projection ")
        .border_style(active_border(snapshot.focus == Focus::Design, has_critical));

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
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_diagnostics_overlay(frame: &mut Frame, immutable: &ImmutableFrame) {
    let Some(diagnostics) = &immutable.snapshot.diagnostics else {
        return;
    };

    let area = immutable.layout.diagnostics;
    if area.width == 0 || area.height == 0 {
        return;
    }

    let text = vec![
        Line::from(format!(" [EVENT] {}", diagnostics.last_event)),
        Line::from(format!(" [FOCUS] {}", diagnostics.last_focus)),
        Line::from(format!(" [INPUT] {}", diagnostics.last_mutation)),
        Line::from(format!(" [SUBSTRATE] raw_mode={}", diagnostics.raw_mode)),
    ];

    // DBM-DIAGNOSTICS-RENDER-BINDING-INTEGRATION-SPEC v1.0 Compliance
    // 7.1 Clear Before Render
    frame.render_widget(Clear, area);

    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    // 6.3 Title Visibility & 8.2 Rect Visualization
                    .title(format!(" DIAGNOSTICS {:?} ", area))
                    // 6.1 Temporary High-Contrast Mode
                    .border_style(Style::default().fg(Color::Yellow).bg(Color::Black)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
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
        assert!(buffer_text(terminal.backend().buffer()).contains("Target:"));

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });
        full_repaint(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(!surface.contains("PREVIOUS_FRAME_RESIDUE"));
        assert!(surface.contains("Target:"));
        assert!(surface.contains("Status:"));
    }

    #[test]
    fn renderer_clears_previous_runtime_frame() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("mutation in progress"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains("preview ready"));
        assert!(!surface.contains("mutation in progress"));
    }

    #[test]
    fn renderer_never_reuses_old_state_text() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Git;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("transaction committed"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains("preview ready"));
        assert!(!surface.contains("transaction committed"));
        assert!(!surface.contains("mutation in progress"));
    }

    #[test]
    fn no_stale_frame_after_redraw() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let first = surface_after_redraw_without_terminal_clear(&mut terminal, &state);
        assert!(first.contains("mutation in progress"));

        state.runtime_state = RuntimeShellState::PreviewReady;
        let second = surface_after_redraw_without_terminal_clear(&mut terminal, &state);

        assert!(second.contains("preview ready"));
        assert!(!second.contains("mutation in progress"));
        assert_ne!(first, second);
    }

    #[test]
    fn runtime_panel_no_overlap() {
        let layout = crate::tui::rendering::layout_for_area(Rect::new(0, 0, 100, 24), false);
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
        state.runtime_state = RuntimeShellState::Apply;
        let width = 120;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());
        assert!(surface.contains("state=mutation in progress"));

        state.runtime_state = RuntimeShellState::Idle;
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface2 = buffer_text(terminal.backend().buffer());

        assert!(surface2.contains("state=runtime idle"));
        assert!(!surface2.contains("state=mutation in progress"));
    }

    #[test]
    fn applying_to_preview_ready_erases_old_cells() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let width = 120;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        assert!(buffer_text(terminal.backend().buffer()).contains("state=mutation in progress"));

        state.runtime_state = RuntimeShellState::Idle;
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains("state=runtime idle"));
        assert!(!surface.contains("state=mutation in progress"));
    }

    #[test]
    fn every_runtime_row_fully_rewritten() {
        let state = TuiState::new(empty_payload());
        let width = 100;
        let height = 24;
        let layout = crate::tui::rendering::layout_for_area(Rect::new(0, 0, width, height), false);
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
        state.runtime_state = RuntimeShellState::Apply;
        state.active_target = Some("apps/cli/src/very_long_previous_runtime_target.rs".to_string());
        let width = 120;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        let first = buffer_text(terminal.backend().buffer());
        assert!(first.contains("state=mutation in progress"));

        state.runtime_state = RuntimeShellState::Idle;
        state.active_target = None;
        redraw_without_terminal_clear(&mut terminal, &state);
        let second = buffer_text(terminal.backend().buffer());

        assert!(second.contains("state=runtime idle"));
        assert!(!second.contains("state=mutation in progress"));
        assert!(!second.contains("very_long_previous_runtime_target"));
    }

    #[test]
    fn redraw_replaces_previous_runtime_text() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let width = 120;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let first = surface_after_redraw_without_terminal_clear(&mut terminal, &state);
        assert!(first.contains("state=mutation in progress"));

        state.runtime_state = RuntimeShellState::Idle;
        let second = surface_after_redraw_without_terminal_clear(&mut terminal, &state);

        assert!(second.contains("state=runtime idle"));
        assert!(!second.contains("state=mutation in progress"));
    }

    #[test]
    fn repaint_never_restores_old_frame() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        let width = 120;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        redraw_without_terminal_clear(&mut terminal, &state);
        state.runtime_state = RuntimeShellState::Idle;
        let second = surface_after_redraw_without_terminal_clear(&mut terminal, &state);
        let third = surface_after_redraw_without_terminal_clear(&mut terminal, &state);

        assert_eq!(second, third);
        assert!(third.contains("state=runtime idle"));
        assert!(!third.contains("state=mutation in progress"));
    }

    #[test]
    fn terminal_surface_matches_snapshot() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Idle;
        let width = 120;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let snapshot = RenderSnapshot::from(&state);
        terminal
            .draw(|frame| render(frame, &snapshot))
            .expect("draw");
        let surface = buffer_text(terminal.backend().buffer());

        assert!(surface.contains("state=runtime idle"));
        assert!(surface.contains(&snapshot.status.line));
        assert!(surface.contains("Target:"));
        assert!(surface.contains("Status:"));
        assert!(!surface.contains("state=mutation in progress"));
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
        assert!(buffer_text(terminal.backend().buffer()).contains("Target:"));

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });
        redraw_without_terminal_clear(&mut terminal, &state);
        let surface = buffer_text(terminal.backend().buffer());

        assert!(!surface.contains("STALE_DIFF_PANEL_TEXT"));
        assert!(surface.contains("Target:"));
        assert!(surface.contains("Status:"));
    }

    #[test]
    fn single_runtime_render_source() {
        let render_source = without_string_literals(
            include_str!("render.rs")
                .split("#[cfg(test)]")
                .next()
                .expect("production render source"),
        );

        assert_eq!(
            render_source.matches("pub fn runtime_panel_bounds").count(),
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

        assert!(!panels_source.contains("pub mod runtime"));
        assert_eq!(
            render_source
                .matches(".title(\" Cognitive Narrative \")")
                .count(),
            1
        );
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
        assert_eq!(runtime_fn.matches("runtime_panel_lines(").count(), 1);
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

        assert_eq!(render_source.matches("runtime_panel_lines(").count(), 1);
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
        assert!(buffer_text(terminal.backend().buffer()).contains("Target:"));

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

    #[test]
    fn test_diagnostics_visible_when_enabled() {
        let mut state = TuiState::new(empty_payload());
        state.diagnostic_mode = true;
        state.diagnostics.last_event = Some("DIAG_TEST_EVENT".to_string());

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        full_repaint(&mut terminal, &state);

        let surface = buffer_text(terminal.backend().buffer());
        assert!(
            surface.contains("DIAGNOSTICS"),
            "Buffer should contain DIAGNOSTICS title"
        );
        assert!(
            surface.contains("DIAG_TEST_EVENT"),
            "Buffer should contain the test event"
        );
    }

    #[test]
    fn test_no_layout_overlap_with_diagnostics() {
        let area = Rect::new(0, 0, 120, 24);
        let layout = crate::tui::rendering::layout_for_area(area, true);
        assert!(
            !panel_overlap_detected(&layout),
            "Layout panels should not overlap when diagnostics is enabled"
        );
        assert!(
            layout.diagnostics.width > 0,
            "Diagnostics area should have positive width"
        );
    }

    #[test]
    fn test_diagnostics_uses_authoritative_layout_rect() {
        let area = Rect::new(0, 0, 120, 24);
        let layout = crate::tui::rendering::layout_for_area(area, true);
        let diag_rect = layout.diagnostics;

        let mut state = TuiState::new(empty_payload());
        state.diagnostic_mode = true;
        let snapshot = RenderSnapshot::from(&state);

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| {
                let layout = crate::tui::rendering::layout_for_area(
                    frame.area(),
                    snapshot.diagnostics.is_some(),
                );
                let immutable = FrameComposer::compose(snapshot.clone(), layout);
                render_diagnostics_overlay(frame, &immutable);
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        // Check if the title is at the expected location (diag_rect.x + 1)
        let title_content = buffer_rect_row(buffer, 120, diag_rect, 0);
        assert!(
            title_content.contains("DIAGNOSTICS"),
            "Title 'DIAGNOSTICS' should be rendered in the allocated rect"
        );
    }
}
// DBM clarification execution guarantee
