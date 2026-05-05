use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::state::{DESIGN_MAX_LINES, Focus, TuiState};

pub fn render(frame: &mut Frame, state: &mut TuiState) {
    let area = frame.area();
    let rows = layout_rows(area, state.design_collapsed);

    render_design(frame, state, rows[0]);
    render_chat(frame, state, rows[1]);
    render_input(frame, state, rows[2]);
    render_help_bar(frame, state, area);
}

fn layout_rows(area: Rect, design_collapsed: bool) -> std::rc::Rc<[Rect]> {
    let input_rows = 5_u16.min(area.height.saturating_sub(2)).max(3);
    let design = if design_collapsed {
        Constraint::Length(3)
    } else {
        Constraint::Percentage(25)
    };

    Layout::default()
        .direction(Direction::Vertical)
        .constraints([design, Constraint::Min(6), Constraint::Length(input_rows)])
        .split(area)
}

fn render_design(frame: &mut Frame, state: &TuiState, area: Rect) {
    let title = if state.design_collapsed {
        format!(
            " Design Convergence View v{} [+] ",
            state.design_doc.version
        )
    } else {
        let version = state
            .core_snapshot
            .design
            .as_ref()
            .map_or(state.design_doc.version, |d| d.version);
        let marker = if state.design_updated { " updated" } else { "" };
        format!(
            " Design Panel v{}{} | {} ",
            version,
            marker,
            state.core_snapshot.status.label()
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(active_border(state.focus == Focus::Design));

    if state.design_collapsed {
        frame.render_widget(
            Paragraph::new(" [DESIGN] collapsed - focus and press d to expand").block(block),
            area,
        );
        return;
    }

    let height = block.inner(area).height as usize;
    let max_rows = height.min(DESIGN_MAX_LINES);
    let panel_lines = state.design_panel_lines();
    let lines: Vec<Line> = panel_lines
        .iter()
        .skip(state.design_scroll)
        .take(max_rows)
        .map(|line| {
            if state.design_updated && !line.is_empty() {
                Line::from(Span::styled(
                    line.as_str(),
                    Style::default().fg(Color::Yellow),
                ))
            } else {
                Line::from(line.as_str())
            }
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_chat(frame: &mut Frame, state: &TuiState, area: Rect) {
    let follow = if state.chat_scroll.is_following {
        "follow"
    } else {
        "paused"
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Chat Stream ({follow}) "))
        .border_style(active_border(state.focus == Focus::Chat));

    let inner_height = block.inner(area).height as usize;
    let all_lines = state.flattened_chat_lines();
    let visible = visible_tail_window(&all_lines, inner_height, state.chat_scroll.offset);
    let lines: Vec<Line> = visible
        .iter()
        .map(|line| {
            let style = chat_line_style(line);
            Line::from(Span::styled(line.as_str(), style))
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_input(frame: &mut Frame, state: &TuiState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Command / Input [{}] ",
            state.pipeline_state.label()
        ))
        .border_style(active_border(state.focus == Focus::Input));

    let display_text = if state.input.text.is_empty() {
        "> ".to_string()
    } else {
        format!("> {}", state.input.text)
    };
    let paragraph = Paragraph::new(display_text)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    if state.focus == Focus::Input {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        let (row, col) = input_cursor_position(&state.input.text, state.input.cursor);
        let x = inner.x + 2 + col as u16;
        let y = inner.y + row as u16;
        if x < inner.x + inner.width && y < inner.y + inner.height {
            frame.set_cursor_position((x, y));
        }
    }
}

fn render_help_bar(frame: &mut Frame, state: &TuiState, area: Rect) {
    let help_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let focus = match state.focus {
        Focus::Input => "Input",
        Focus::Chat => "Chat",
        Focus::Design => "Design",
    };
    let text = format!(
        " focus:{focus}   Enter send   Shift+Enter newline   select <n>   y/n confirm   cancel   /save design   Ctrl+q quit "
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
        help_area,
    );
}

fn visible_tail_window(lines: &[String], height: usize, scroll_from_tail: usize) -> &[String] {
    if height == 0 || lines.is_empty() {
        return &[];
    }
    let end = lines.len().saturating_sub(scroll_from_tail);
    let start = end.saturating_sub(height);
    &lines[start..end]
}

fn input_cursor_position(text: &str, cursor: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;
    for ch in text[..cursor].chars() {
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (row, col)
}

fn chat_line_style(line: &str) -> Style {
    if line.starts_with("[THINKING]") {
        kind_style(Color::Cyan)
    } else if line.starts_with("[EDITING]") {
        kind_style(Color::Yellow)
    } else if line.starts_with("[PLAN]") {
        kind_style(Color::Blue)
    } else if line.starts_with("[PREVIEW]") {
        kind_style(Color::Magenta)
    } else if line.starts_with("[EXECUTION]") {
        kind_style(Color::Yellow)
    } else if line.starts_with("[DIFF]") {
        kind_style(Color::Magenta)
    } else if line.starts_with("[DESIGN]") {
        kind_style(Color::Cyan)
    } else if line.starts_with("[DESIGN DIFF]") {
        kind_style(Color::Cyan)
    } else if line.starts_with("[RECOVERY]") {
        kind_style(Color::LightRed)
    } else if line.starts_with("[RESULT]") {
        kind_style(Color::Green)
    } else if line.starts_with("[PIPELINE]") {
        kind_style(Color::Gray)
    } else if line.starts_with("[NEXT]") {
        kind_style(Color::White)
    } else if line.starts_with("[ERROR]") {
        kind_style(Color::Red)
    } else if line.starts_with("[DEBUG]") {
        kind_style(Color::DarkGray)
    } else {
        Style::default()
    }
}

fn kind_style(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
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
