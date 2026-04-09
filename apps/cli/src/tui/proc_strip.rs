use std::time::Duration;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub const RUNNING_MIN_VISIBLE: Duration = Duration::from_millis(400);
pub const DONE_MIN_VISIBLE: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcPhase {
    Idle,
    Running,
    ReadingFiles,
    Planning,
    WritingEdit,
    Done,
    Error,
}

impl ProcPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::ReadingFiles => "reading_files",
            Self::Planning => "planning",
            Self::WritingEdit => "writing_edit",
            Self::Done => "done",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStripState {
    pub phase: ProcPhase,
    pub detail: String,
}

impl Default for ProcStripState {
    fn default() -> Self {
        Self::idle()
    }
}

impl ProcStripState {
    pub fn idle() -> Self {
        Self {
            phase: ProcPhase::Idle,
            detail: "ready".to_string(),
        }
    }

    pub fn set(&mut self, phase: ProcPhase, detail: impl Into<String>) {
        self.phase = phase;
        self.detail = detail.into();
    }

    pub fn reset(&mut self) {
        *self = Self::idle();
    }

    pub fn status_text(&self) -> String {
        format!("status: {}", self.phase.as_str())
    }

    pub fn label(&self) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("[ {} ]", self.phase.as_str()),
                badge_style(self.phase),
            ),
            Span::raw(" "),
            Span::styled(self.detail.clone(), Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(self.status_text(), Style::default().fg(Color::DarkGray)),
        ])
    }
}

pub fn render_proc_strip(frame: &mut Frame, state: &ProcStripState, area: Rect) {
    frame.render_widget(Paragraph::new(state.label()), area);
}

fn badge_style(phase: ProcPhase) -> Style {
    let color = match phase {
        ProcPhase::Idle => Color::DarkGray,
        ProcPhase::Running => Color::Blue,
        ProcPhase::ReadingFiles => Color::Cyan,
        ProcPhase::Planning => Color::Yellow,
        ProcPhase::WritingEdit => Color::Magenta,
        ProcPhase::Done => Color::Green,
        ProcPhase::Error => Color::Red,
    };
    Style::default()
        .fg(Color::Black)
        .bg(color)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_is_structured() {
        let mut state = ProcStripState::idle();
        state.set(ProcPhase::Planning, "planning request...");
        assert_eq!(state.status_text(), "status: planning");
    }
}
