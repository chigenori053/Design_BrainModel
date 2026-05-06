use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::panels::{diff::diff_panel_lines, runtime::runtime_panel_lines};
use super::state::{Focus, TuiState};

pub fn render(frame: &mut Frame, state: &mut TuiState) {
    let area = frame.area();
    let rows = layout_rows(area);
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(rows[1]);

    render_input(frame, state, rows[0]);
    render_runtime_state(frame, state, middle[0]);
    render_diff_preview(frame, state, middle[1]);
    render_status_line(frame, state, rows[2]);
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

fn render_runtime_state(frame: &mut Frame, state: &TuiState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Runtime State ")
        .border_style(active_border(state.focus == Focus::Chat));
    let lines = runtime_panel_lines(state)
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_diff_preview(frame: &mut Frame, state: &TuiState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Diff / Preview ")
        .border_style(active_border(state.focus == Focus::Design));
    let lines = diff_panel_lines(state)
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

fn render_status_line(frame: &mut Frame, state: &TuiState, area: Rect) {
    frame.render_widget(
        Paragraph::new(state.status_line()).style(Style::default().fg(Color::DarkGray)),
        area,
    );
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

fn active_border(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
