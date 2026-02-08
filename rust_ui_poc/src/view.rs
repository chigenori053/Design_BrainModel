use crate::app::App;
use crate::model::{ActiveTab, ActiveView, DesignDraftStatus};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

pub fn render(app: &mut App, frame: &mut Frame) {
    if app.state.active_view == ActiveView::PhaseC {
        render_phasec(app, frame);
        return;
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Content + Input
        ])
        .split(frame.size());

    render_tabs(app, frame, main_layout[0]);
    render_content(app, frame, main_layout[1]);

    if app.state.show_help {
        render_help_overlay(app, frame);
    }
}

fn render_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    let titles = vec![
        Line::from(" 1: FreeNote "),
        Line::from(" 2: Understanding "),
        Line::from(" 3: Design Draft "),
    ];
    let selected = match app.state.active_tab {
        ActiveTab::FreeNote => 0,
        ActiveTab::Understanding => 1,
        ActiveTab::DesignDraft => 2,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(tabs, area);
}

fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    match app.state.active_tab {
        ActiveTab::FreeNote => render_free_notes(app, frame, area),
        ActiveTab::Understanding => render_understanding(app, frame, area),
        ActiveTab::DesignDraft => render_design_draft(app, frame, area),
    }
}

fn render_free_notes(app: &mut App, frame: &mut Frame, area: Rect) {
    render_messages_with_input(app, frame, area, " Free Notes ");
}

fn render_understanding(app: &mut App, frame: &mut Frame, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(area);
    render_l1_list(app, frame, layout[0]);
    render_messages_with_input(app, frame, layout[1], " Understanding (L1) ");
}

fn render_design_draft(app: &mut App, frame: &mut Frame, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(area);
    render_l2_list(app, frame, layout[0]);
    render_messages_with_input(app, frame, layout[1], " Design Draft (L2) ");
}

fn render_l1_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app.state.l1_atoms.iter()
        .map(|atom| {
            let lines = vec![
                Line::from(vec![
                    Span::styled(&atom.r#type, Style::default().add_modifier(Modifier::ITALIC)),
                    Span::raw(": "),
                    Span::raw(&atom.content),
                ]),
            ];
            ListItem::new(lines)
        })
        .collect();

    let mut state = ListState::default();
    state.select(app.state.selected_l1_index);

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Understanding Units "))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_l2_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app.state.l2_units.iter()
        .map(|draft| {
            let (border_color, badge) = match draft.status {
                DesignDraftStatus::Draft => (Color::Gray, " [DRAFT] "),
                DesignDraftStatus::Review => (Color::Blue, " [READY FOR REVIEW] "),
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled(badge, Style::default().fg(border_color).add_modifier(Modifier::BOLD)),
                    Span::styled(&draft.title, Style::default().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(format!("   Description: {}", draft.description)),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(&draft.feedback_text, Style::default().fg(Color::Gray)),
                ]),
            ];
            ListItem::new(lines)
        })
        .collect();

    let mut state = ListState::default();
    state.select(app.state.selected_l2_index);

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Design Draft Units "))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_messages_with_input(app: &mut App, frame: &mut Frame, area: Rect, title: &str) {
    let mut lines: Vec<Line> = app.state.tab_messages
        .get(&app.state.active_tab)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(Line::from)
        .collect();

    let input_prefix = if app.state.input_mode { "> " } else { ": " };
    lines.push(Line::from(format!("{}{}", input_prefix, app.state.input_buffer)));

    let help_hint = if app.state.input_mode {
        "Enter: submit | Esc: exit input"
    } else {
        "i: input | Tab: switch tab | j/k: select | ?: help | q: quit"
    };
    lines.push(Line::from(help_hint).fg(Color::DarkGray));

    let scroll = lines
        .len()
        .saturating_sub(area.height.saturating_sub(2) as usize) as u16;
    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn render_phasec(app: &mut App, frame: &mut Frame) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(9),
        ])
        .split(frame.size());

    render_overview_panel(app, frame, layout[0]);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(layout[1]);

    render_geometry_panel(app, frame, mid[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(mid[1]);
    render_proposal_detail_panel(app, frame, right[0]);
    render_comparison_panel(app, frame, right[1]);

    render_human_override_panel(app, frame, layout[2]);

    if app.state.show_help {
        render_help_overlay(app, frame);
    }
}

fn render_overview_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let proposal_count = app.state.phasec_state.as_ref().map(|s| s.proposals.len()).unwrap_or(0);
    let status = if app.state.phasec_state.is_some() { "Evaluated" } else { "Not Evaluated" };

    let text = Text::from(vec![
        Line::from(format!("Proposals: {}", proposal_count)),
        Line::from(format!("Status: {}", status)),
    ]);
    let block = Block::default().borders(Borders::ALL).title(" PhaseC Overview ");
    frame.render_widget(Paragraph::new(text).block(block), area);
}

fn render_geometry_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Geometry Visualization (Abstract) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(state) = &app.state.phasec_state {
        let points = &state.report.geometry_points;
        if points.is_empty() {
            lines.push(Line::from("No visualization data."));
        } else {
            let selected_id = state.proposals.get(app.state.selected_proposal_index).map(|p| p.id.clone()).unwrap_or_default();
            let grid = build_geometry_grid(points, inner.width as usize, inner.height as usize, &selected_id);
            lines.extend(grid);
        }
    } else {
        lines.push(Line::from("PhaseC state not loaded."));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_proposal_detail_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Proposal Detail ");
    let mut lines: Vec<Line> = Vec::new();
    if let Some(state) = &app.state.phasec_state {
        if let Some(proposal) = state.proposals.get(app.state.selected_proposal_index) {
            lines.push(Line::from(format!("ID: {}", proposal.id)));
            lines.push(Line::from(format!("Title: {}", proposal.title)));
            lines.push(Line::from(format!("Type: {}", proposal.target_type)));
            if !proposal.constraints.is_empty() {
                lines.push(Line::from(format!("Constraints: {}", proposal.constraints.join(", "))));
            }
            lines.push(Line::from("Qualitative Status:"));
            lines.push(Line::from("  This proposal is being analyzed for consistency."));
        } else {
            lines.push(Line::from("No proposal selected."));
        }
    } else {
        lines.push(Line::from("PhaseC state not loaded."));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn render_comparison_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Relationship Analysis ");
    let mut lines: Vec<Line> = Vec::new();
    if let Some(_state) = &app.state.phasec_state {
        lines.push(Line::from("Analysis of distances and conflicts..."));
        lines.push(Line::from("Selected item shows high relevance to its neighbors."));
    } else {
        lines.push(Line::from("PhaseC state not loaded."));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn render_human_override_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Human Decision ");
    let mut lines: Vec<Line> = Vec::new();

    let hint = if app.state.override_input_mode {
        "Rationale input: type, Enter/Esc to finish"
    } else {
        "Actions: a=ACCEPT h=HOLD r=REJECT s=SAVE o=rationale"
    };

    lines.push(Line::from(hint));

    if let Some(state) = &app.state.phasec_state {
        if let Some(proposal) = state.proposals.get(app.state.selected_proposal_index) {
            lines.push(Line::from(format!("Target: {}", proposal.id)));
        }
    }

    if app.state.override_input_mode {
        lines.push(Line::from(format!("Rationale: {}", app.state.override_buffer)));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn render_help_overlay(_app: &mut App, frame: &mut Frame) {
    let area = frame.size();
    let help_block = Block::default().borders(Borders::ALL).title(" Help / Commands ").bg(Color::Black);
    let inner = help_block.inner(area);

    let lines = vec![
        Line::from("Global Commands"),
        Line::from("  Tab: Cycle Tabs"),
        Line::from("  q: Quit"),
        Line::from("  ?: Toggle Help"),
        Line::from("  c: Toggle PhaseC View"),
        Line::from(""),
        Line::from("Free Notes / Understanding / Design Draft"),
        Line::from("  i: Input Mode"),
        Line::from("  Enter (Input): Submit"),
        Line::from("  Esc (Input): Exit Input Mode"),
        Line::from(""),
        Line::from("Understanding / Design Draft Tabs"),
        Line::from("  j/k or ↓/↑: Select Unit"),
        Line::from(""),
        Line::from("PhaseC View"),
        Line::from("  j/k: Select Proposal"),
        Line::from("  a/h/r: Accept / Hold / Reject"),
        Line::from("  o: Enter Rationale"),
    ];

    frame.render_widget(help_block, area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn build_geometry_grid(
    points: &[crate::model::GeometryPointVm],
    width: usize,
    height: usize,
    selected_id: &str,
) -> Vec<Line<'static>> {
    if width < 2 || height < 2 {
        return vec![Line::from("Panel too small.")];
    }
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for p in points {
        xs.push(*p.vector.get(0).unwrap_or(&0.0));
        ys.push(*p.vector.get(1).unwrap_or(&0.0));
    }
    let min_x = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_x = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_y = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_y = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let mut grid = vec![vec![' '; width]; height];
    for p in points.iter() {
        let x = *p.vector.get(0).unwrap_or(&0.0);
        let y = *p.vector.get(1).unwrap_or(&0.0);
        let norm_x = if max_x == min_x { 0.5 } else { (x - min_x) / (max_x - min_x) };
        let norm_y = if max_y == min_y { 0.5 } else { (y - min_y) / (max_y - min_y) };
        let gx = (norm_x * (width - 1) as f64).round() as usize;
        let gy = (norm_y * (height - 1) as f64).round() as usize;
        let mark = if p.source_id == selected_id { 'X' } else { 'o' };
        grid[height - 1 - gy][gx] = mark;
    }

    grid.into_iter()
        .map(|row| Line::from(row.iter().collect::<String>()))
        .collect()
}
