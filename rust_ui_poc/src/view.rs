use crate::model::{ConfidenceLevel, ConsensusStatus, DecisionDto, DecisionSummaryDto, EntropyLevel};
use crate::event::UiEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

// --- Color Helpers ---
fn status_color(status: &ConsensusStatus) -> Color {
    match status {
        ConsensusStatus::Reached => Color::Green,      // ACCEPT
        ConsensusStatus::Reevaluating => Color::Yellow, // REVIEW
        ConsensusStatus::Failed => Color::Red,         // ESCALATE
        ConsensusStatus::Pending => Color::Gray,       // PENDING
    }
}

// --- HeaderView ---
pub struct HeaderView;

pub struct HeaderProps {
    pub system_status: String,
    pub decision_status: ConsensusStatus,
    pub human_override: bool,
}

impl HeaderView {
    pub fn render(frame: &mut Frame, area: Rect, props: &HeaderProps) {
        let status_color = status_color(&props.decision_status);
        
        let override_alert = if props.human_override {
            Span::styled(" ⚠ HUMAN OVERRIDE ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        } else {
            Span::raw("")
        };

        let content = Line::from(vec![
            Span::raw(" System: "),
            Span::styled(format!(" {} ", props.system_status), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | Decision: "),
            Span::styled(format!(" {:?} ", props.decision_status), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            override_alert,
        ]);

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::BOTTOM))
            .style(Style::default().fg(Color::White));

        frame.render_widget(paragraph, area);
    }
}

// --- CurrentDecisionView ---
pub struct CurrentDecisionView;

pub struct CurrentDecisionProps {
    pub decision: DecisionDto,
}

impl CurrentDecisionView {
    pub fn render(frame: &mut Frame, area: Rect, props: &CurrentDecisionProps) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Current Decision ");

        let active_area = block.inner(area);
        frame.render_widget(block, area);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Status
                Constraint::Length(1), // ID & Evaluators
                Constraint::Length(1), // Metrics
                Constraint::Min(1),    // Selected Candidate (if any)
            ])
            .split(active_area);

        // 1. Status Line
        let status_color = status_color(&props.decision.status);
        let status_line = Line::from(vec![
            Span::raw("Status: "),
            Span::styled(format!("{:?}", props.decision.status), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]);
        frame.render_widget(Paragraph::new(status_line), layout[0]);

        // 2. Metadata
        let meta_line = Line::from(vec![
            Span::raw(format!("ID: {} | Evaluators: {}", props.decision.id, props.decision.evaluator_count)),
        ]);
        frame.render_widget(Paragraph::new(meta_line), layout[1]);

        // 3. Metrics
        let conf_color = match props.decision.confidence {
            ConfidenceLevel::High => Color::Green,
            ConfidenceLevel::Medium => Color::Yellow,
            ConfidenceLevel::Low => Color::Red,
        };
        let ent_color = match props.decision.entropy {
            EntropyLevel::Low => Color::Green,
            EntropyLevel::Medium => Color::Yellow,
            EntropyLevel::High => Color::Red,
        };

        let metrics_line = Line::from(vec![
            Span::raw("Confidence: "),
            Span::styled(format!("{:?}", props.decision.confidence), Style::default().fg(conf_color)),
            Span::raw(" | Entropy: "),
            Span::styled(format!("{:?}", props.decision.entropy), Style::default().fg(ent_color)),
        ]);
        frame.render_widget(Paragraph::new(metrics_line), layout[2]);

        // 4. Candidate
        if let Some(candidate) = &props.decision.selected_candidate {
            let candidate_line = Line::from(vec![
                Span::raw("Selected: "),
                Span::styled(candidate, Style::default().add_modifier(Modifier::BOLD)),
            ]);
            frame.render_widget(Paragraph::new(candidate_line), layout[3]);
        }
    }
}

// --- ExplanationView ---
pub struct ExplanationView;

pub struct ExplanationProps {
    pub explanation_text: String,
}

impl ExplanationView {
    pub fn render(frame: &mut Frame, area: Rect, props: &ExplanationProps) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Explanation ");

        // If Pending/Connecting, dim the text or show specific message (handled by logic passing "Connecting...")
        let paragraph = Paragraph::new(props.explanation_text.clone())
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}

// --- DecisionHistoryView ---
pub struct DecisionHistoryView;

pub struct HistoryProps {
    pub history: Vec<DecisionSummaryDto>,
}

impl DecisionHistoryView {
    pub fn render(frame: &mut Frame, area: Rect, props: &HistoryProps) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" History ");

        let items: Vec<ListItem> = props.history.iter().map(|item| {
            let indent = if item.is_reevaluation { "  ↳ " } else { "" };
            let status_color = status_color(&item.status);
            
            let content = Line::from(vec![
                Span::raw(indent),
                Span::styled(format!("{:?}", item.status), Style::default().fg(status_color)),
                Span::raw(format!(" {}", item.id)),
            ]);
            ListItem::new(content)
        }).collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }
}

// --- EventInputView ---
pub struct EventInputView;

pub struct EventInputProps {
    pub input_buffer: String,
}

impl EventInputView {
    pub fn render(frame: &mut Frame, area: Rect, props: &EventInputProps) {
        let block = Block::default()
            .borders(Borders::TOP)
            .title(" Input ");

        let content = Line::from(vec![
            Span::styled("> ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&props.input_buffer),
            Span::styled("█", Style::default().fg(Color::Gray)), // Cursor simulation
        ]);

        let paragraph = Paragraph::new(content)
            .block(block);
        
        frame.render_widget(paragraph, area);
    }

    // Reuse logic from previous phase, or keep it in App
    // We'll keep parsing logic here or move to a separate helper if needed.
    // For now, simple parsing is fine.
    pub fn parse_command(input: &str) -> Option<UiEvent> {
        use crate::event::UiEvent;
        
        let trimmed = input.trim();
        if trimmed.is_empty() {
             return None;
        }
        
        match trimmed {
            "reevaluate" | "r" => Some(UiEvent::RequestReevaluation),
            s if s.starts_with("override ") => {
                 let parts: Vec<&str> = s.splitn(2, ' ').collect();
                 if parts.len() == 2 {
                     Some(UiEvent::HumanOverride {
                         decision: ConsensusStatus::Reached, // Mock Default, real usage might need specific status
                         reason: parts[1].to_string(),
                     })
                 } else {
                     Some(UiEvent::UserInput(s.to_string()))
                 }
            },
            "help" => None, // Just consumes it, maybe show help in future
            _ => Some(UiEvent::UserInput(trimmed.to_string())),
        }
    }
}
