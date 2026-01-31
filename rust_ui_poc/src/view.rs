use crate::app::App;
use crate::model::L1Type;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

/// Renders the user interface.
pub fn render(app: &mut App, frame: &mut Frame) {
    // Create a main layout with three areas:
    // 1. Input area.
    // 2. Main content area (split into two panes).
    // 3. Footer area for logs.
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(0),
            Constraint::Length(8),
        ])
        .split(frame.size());

    render_input_pane(app, frame, main_layout[0]);

    // Split the main content area into a left and right pane.
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(main_layout[1]);

    // Render each pane.
    render_l1_atoms_pane(app, frame, content_layout[0]);
    render_details_pane(app, frame, content_layout[1]);
    render_logs_pane(app, frame, main_layout[2]);
}

fn render_input_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let mode_label = if app.state.input_mode { "INPUT MODE" } else { "VIEW MODE" };
    let l1_type_label = format!("{:?}", app.state.input_l1_type).to_uppercase();
    let hint = if app.state.input_mode {
        "Enter: submit | Esc: exit | Tab: cycle type | 1-5: set type"
    } else {
        "i: input mode | q: quit"
    };

    let mut lines = vec![
        Line::from(vec![
            "Mode: ".bold(),
            mode_label.into(),
            "   L1 Type: ".bold(),
            l1_type_label.into(),
        ]),
        Line::from(" "),
        Line::from(app.state.input_buffer.clone()),
        Line::from(" "),
        Line::from(hint),
    ];

    if app.state.input_l1_type_manual {
        lines.insert(2, Line::from("[Manual override enabled]".fg(Color::Yellow)));
    }

    let block = Block::default().borders(Borders::ALL).title("Text Input");
    let paragraph = Paragraph::new(Text::from(lines)).block(block);
    frame.render_widget(paragraph, area);
}

fn render_l1_atoms_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app.state.l1_atoms.iter().rev().map(|atom| {
        let prefix = format!("[{}]", atom.r#type);
        let line = Line::from(format!("{} {}", prefix, atom.content));
        ListItem::new(line)
    }).collect();

    let list_block = Block::default().borders(Borders::ALL).title("L1 Atoms (Latest)");

    // Create a ListState to manage the selection.
    let mut list_state = ListState::default();
    list_state.select(None);

    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_details_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Details / Policy");

    let details_text = Text::from(vec![
        Line::from("This UI only stores text as L1 atoms."),
        Line::from("Decision (L2) is read-only in this phase."),
        Line::from(""),
        Line::from("L1 Types:"),
        Line::from(format!("1: {:?}", L1Type::Observation)),
        Line::from(format!("2: {:?}", L1Type::Requirement)),
        Line::from(format!("3: {:?}", L1Type::Constraint)),
        Line::from(format!("4: {:?}", L1Type::Hypothesis)),
        Line::from(format!("5: {:?}", L1Type::Question)),
    ]);
    
    let paragraph = Paragraph::new(details_text).block(block);
    frame.render_widget(paragraph, area);
}

fn render_logs_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Logs");
    
    // Create a scrollable paragraph for logs.
    let log_text: Vec<Line> = app.state.logs.iter().cloned().map(Line::from).collect();
    
    let paragraph = Paragraph::new(log_text)
        .block(block)
        .scroll((app.state.logs.len().saturating_sub(area.height as usize - 2) as u16, 0));

    frame.render_widget(paragraph, area);
}
