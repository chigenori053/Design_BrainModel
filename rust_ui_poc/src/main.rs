mod model;
mod view;
mod event;
mod vm_client;
mod app;
mod tui;

use crate::app::{App, Action};
use crate::event::EventHandler;
use crate::tui::Tui;
use anyhow::Result;
use simplelog::{Config, LevelFilter, WriteLogger};
use std::fs::File;

fn main() -> Result<()> {
    // Initialize Logging
    let _ = WriteLogger::init(
        LevelFilter::Info,
        Config::default(),
        File::create("client.log").unwrap(),
    );

    // Create application instances
    let mut app = App::new();
    let mut tui = Tui::new()?;
    let event_handler = EventHandler::new();
    
    // Initialize the application state
    app.init()?;

    // Main application loop
    while app.state.is_running {
        // Render the UI
        tui.draw(&mut app)?;
        
        // Handle events
        let action = event_handler.next()?;
        app.dispatch(action)?;
    }

    // Restore the terminal on exit
    Tui::restore_terminal()?;
    Ok(())
}
