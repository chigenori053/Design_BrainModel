use crate::app::App;
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
    // 1. Main content area (split into two panes).
    // 2. Footer area for logs.
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(8),
        ])
        .split(frame.size());

    // Split the main content area into a left and right pane.
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(main_layout[0]);

    // Render each pane.
    render_clusters_pane(app, frame, content_layout[0]);
    render_details_pane(app, frame, content_layout[1]);
    render_logs_pane(app, frame, main_layout[1]);
}

fn render_clusters_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app.state.clusters.iter().map(|cluster| {
        let line = Line::from(format!("{} ({})", cluster.id, cluster.l1_count));
        ListItem::new(line)
    }).collect();

    let list_block = Block::default().borders(Borders::ALL).title("Clusters");

    // Create a ListState to manage the selection.
    let mut list_state = ListState::default();
    list_state.select(app.state.selected_cluster_index);

    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_details_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Details");
    
    let details_text = if let Some(index) = app.state.selected_cluster_index {
        app.state.clusters.get(index).map_or_else(
            || Text::from("No cluster selected or index out of bounds."),
            |cluster| {
                Text::from(vec![
                    Line::from(vec!["ID: ".bold(), cluster.id.clone().into()]),
                    Line::from(vec!["Status: ".bold(), format!("{:?}", cluster.status).into()]),
                    Line::from(vec!["L1 Count: ".bold(), cluster.l1_count.to_string().into()]),
                    Line::from(vec!["Entropy: ".bold(), cluster.entropy.to_string().into()]),
                ])
            },
        )
    } else {
        Text::from("No cluster selected.")
    };
    
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
