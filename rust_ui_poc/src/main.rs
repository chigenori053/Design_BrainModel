mod model;
mod view;
mod event;
mod vm_client;
mod app;
mod tui;

use crate::app::App;
use crate::event::EventHandler;
use crate::tui::Tui;
use anyhow::Result;
use simplelog::{Config, LevelFilter, WriteLogger};
use std::fs::File;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // Initialize Logging
    let _ = WriteLogger::init(
        LevelFilter::Info,
        Config::default(),
        File::create("client.log").unwrap(),
    );

    // Start API server if needed
    let mut server_child = start_server_if_needed();

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
        let action = event_handler.next(
            app.state.input_mode,
            app.state.active_view.clone(),
            app.state.override_input_mode,
        )?;
        app.dispatch(action)?;
    }

    // Restore the terminal on exit
    Tui::restore_terminal()?;
    if let Some(child) = server_child.as_mut() {
        let _ = child.kill();
    }
    Ok(())
}

fn start_server_if_needed() -> Option<Child> {
    let host = std::env::var("DESIGN_BRAIN_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("DESIGN_BRAIN_SERVER_PORT").unwrap_or_else(|_| "8000".to_string());
    let addr = format!("{}:{}", host, port);

    if TcpStream::connect(&addr).is_ok() {
        return None;
    }

    let python_bin = std::env::var("DESIGN_BRAIN_SERVER_PY").ok().or_else(|| {
        let venv_py = Path::new(".venv/bin/python");
        if venv_py.exists() {
            Some(venv_py.to_string_lossy().to_string())
        } else {
            None
        }
    }).unwrap_or_else(|| "python3".to_string());

    let mut cmd = Command::new(python_bin);
    cmd.args([
        "-m",
        "uvicorn",
        "design_brain_model.hybrid_vm.interface_layer.api_server:app",
        "--host",
        &host,
        "--port",
        &port,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    let child = cmd.spawn().ok();

    // Wait briefly for server to come up
    for _ in 0..10 {
        if TcpStream::connect(&addr).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(300));
    }

    child
}
