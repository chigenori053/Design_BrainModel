use std::io::{self, Stdout};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::runtime::event_queue::RuntimeEventQueue;
use crate::tui::{render, state::TuiState};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderScheduler {
    repaint_requested: bool,
    sequence: u64,
}

impl RenderScheduler {
    pub fn observe_queue(&mut self, queue: &RuntimeEventQueue) {
        if !queue.is_empty() {
            self.request_full_repaint();
        }
    }

    pub fn request(&mut self) {
        self.request_full_repaint();
    }

    pub fn request_full_repaint(&mut self) {
        self.repaint_requested = true;
    }

    pub fn take_pending(&mut self) -> bool {
        let pending = self.repaint_requested;
        self.repaint_requested = false;
        if pending {
            self.sequence = self.sequence.saturating_add(1);
        }
        pending
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

pub struct TerminalRenderer {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    active: bool,
}

impl TerminalRenderer {
    pub fn enter() -> Result<Self, String> {
        enable_raw_mode().map_err(|err| err.to_string())?;
        let mut stdout = io::stdout();
        if let Err(err) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            Hide,
            Clear(ClearType::All),
            MoveTo(0, 0)
        ) {
            disable_raw_mode().ok();
            return Err(err.to_string());
        }
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|err| err.to_string())?;
        terminal.clear().map_err(|err| err.to_string())?;
        Ok(Self {
            terminal,
            active: true,
        })
    }

    pub fn full_repaint(&mut self, state: &TuiState) -> Result<(), String> {
        self.terminal.clear().map_err(|err| err.to_string())?;
        self.terminal
            .draw(|frame| render::render(frame, state))
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn shutdown(mut self) {
        self.restore();
    }

    fn restore(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        disable_raw_mode().ok();
        execute!(
            self.terminal.backend_mut(),
            Show,
            Clear(ClearType::All),
            MoveTo(0, 0),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .ok();
        self.terminal.show_cursor().ok();
    }
}

impl Drop for TerminalRenderer {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_only_tracks_full_repaint_requests() {
        let mut scheduler = RenderScheduler::default();
        assert!(!scheduler.take_pending());

        scheduler.request();
        assert!(scheduler.take_pending());
        assert_eq!(scheduler.sequence(), 1);
        assert!(!scheduler.take_pending());

        scheduler.request_full_repaint();
        scheduler.request_full_repaint();
        assert!(scheduler.take_pending());
        assert_eq!(scheduler.sequence(), 2);
    }
}
