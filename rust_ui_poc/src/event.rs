use crate::app::Action;
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
    pub fn next(&self) -> Result<Action> {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    return self.handle_key_event(key);
                }
            }
        }
        // If no key event, return a Tick action.
        Ok(Action::Tick)
    }

    /// Maps a `KeyEvent` to a corresponding `Action`.
    fn handle_key_event(&self, key: KeyEvent) -> Result<Action> {
        let action = match key.code {
            KeyCode::Char('q') => Action::Quit,
            // Add more key bindings here, e.g., for navigation
            // KeyCode::Char('j') => Action::SelectNext,
            // KeyCode::Char('k') => Action::SelectPrevious,
            _ => Action::Tick, // Default to a Tick for unhandled keys
        };
        Ok(action)
    }
}
