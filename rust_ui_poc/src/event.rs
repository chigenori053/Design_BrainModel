use crate::app::Action;
use crate::model::ActiveView;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::time::Duration;

/// Handles terminal events and maps them to application `Action`s.
pub struct EventHandler;

impl EventHandler {
    pub fn new() -> Self {
        Self
    }

    /// Blocks until a key event is received or a timeout occurs.
    pub fn next(&self, input_mode: bool, active_view: ActiveView, override_input_mode: bool) -> Result<Action> {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    return self.handle_key_event(key, input_mode, active_view, override_input_mode);
                }
            }
        }
        Ok(Action::Tick)
    }

    fn handle_key_event(
        &self,
        key: KeyEvent,
        input_mode: bool,
        active_view: ActiveView,
        override_input_mode: bool,
    ) -> Result<Action> {
        let action = if active_view == ActiveView::PhaseC {
            if override_input_mode {
                match key.code {
                    KeyCode::Esc => Action::ExitOverrideInput,
                    KeyCode::Enter => Action::ExitOverrideInput,
                    KeyCode::Backspace => Action::OverrideBackspace,
                    KeyCode::Char(ch) => Action::OverrideInputChar(ch),
                    _ => Action::Tick,
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => Action::Quit,
                    KeyCode::Char('c') => Action::TogglePhaseC,
                    KeyCode::Char('?') => Action::ToggleHelp,
                    KeyCode::Char('j') | KeyCode::Down => Action::SelectNextProposal,
                    KeyCode::Char('k') | KeyCode::Up => Action::SelectPrevProposal,
                    KeyCode::Char('a') => Action::OverrideAccept,
                    KeyCode::Char('h') => Action::OverrideHold,
                    KeyCode::Char('r') => Action::OverrideReject,
                    KeyCode::Char('s') => Action::OverrideSaveAsKnowledge,
                    KeyCode::Char('o') => Action::EnterOverrideInput,
                    _ => Action::Tick,
                }
            }
        } else if input_mode {
            match key.code {
                KeyCode::Esc => Action::ExitInput,
                KeyCode::Enter => Action::SubmitInput,
                KeyCode::Backspace => Action::Backspace,
                KeyCode::Char(ch) => Action::InputChar(ch),
                _ => Action::Tick,
            }
        } else {
            match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('i') => Action::EnterInput,
                KeyCode::Char('c') => Action::TogglePhaseC,
                KeyCode::Char('?') => Action::ToggleHelp,
                KeyCode::Tab => Action::CycleTab,
                KeyCode::Char('j') | KeyCode::Down => Action::SelectNextUnit,
                KeyCode::Char('k') | KeyCode::Up => Action::SelectPrevUnit,
                _ => Action::Tick,
            }
        };
        Ok(action)
    }
}
