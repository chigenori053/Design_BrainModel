use crate::app::Action;
use crate::model::L1Type;
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
    pub fn next(&self, input_mode: bool) -> Result<Action> {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    return self.handle_key_event(key, input_mode);
                }
            }
        }
        // If no key event, return a Tick action.
        Ok(Action::Tick)
    }

    /// Maps a `KeyEvent` to a corresponding `Action`.
    fn handle_key_event(&self, key: KeyEvent, input_mode: bool) -> Result<Action> {
        let action = if input_mode {
            match key.code {
                KeyCode::Esc => Action::ExitInput,
                KeyCode::Enter => Action::SubmitInput,
                KeyCode::Tab => Action::CycleL1Type,
                KeyCode::Backspace => Action::Backspace,
                KeyCode::Char('1') => Action::SetL1Type(L1Type::Observation),
                KeyCode::Char('2') => Action::SetL1Type(L1Type::Requirement),
                KeyCode::Char('3') => Action::SetL1Type(L1Type::Constraint),
                KeyCode::Char('4') => Action::SetL1Type(L1Type::Hypothesis),
                KeyCode::Char('5') => Action::SetL1Type(L1Type::Question),
                KeyCode::Char(ch) => Action::InputChar(ch),
                _ => Action::Tick,
            }
        } else {
            match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('i') => Action::EnterInput,
                _ => Action::Tick,
            }
        };
        Ok(action)
    }
}
