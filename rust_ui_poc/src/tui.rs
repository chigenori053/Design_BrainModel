use crate::{app::App, view};
use anyhow::Result;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout, Stdout};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

/// Represents the Terminal UI, responsible for drawing and managing the terminal state.
pub struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Tui {
    /// Creates a new `Tui` instance and initializes the terminal.
    pub fn new() -> Result<Self> {
        let terminal = Self::init_terminal()?;
        Ok(Self { terminal })
    }

    /// Initializes the terminal for TUI rendering.
    fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        Ok(terminal)
    }

    /// Draws the application's UI by calling the main render function.
    pub fn draw(&mut self, app: &mut App) -> Result<()> {
        self.terminal.draw(|frame| view::render(app, frame))?;
        Ok(())
    }

    /// Restores the terminal to its original state.
    pub fn restore_terminal() -> Result<()> {
        execute!(stdout(), LeaveAlternateScreen)?;
        disable_raw_mode()?;
        Ok(())
    }
}
