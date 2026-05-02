pub mod composer;
pub mod confidence_rank;
pub mod core;
pub mod edit_block;
pub mod model;
pub mod proc_strip;
pub mod render;
pub mod review_batch;
pub mod state;

use std::io;
use std::{thread, time::Duration};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use self::core::RuntimeCoreBridge;
use self::model::UiPayload;
use self::state::{TuiAction, TuiState};

const FRAME_TIME: Duration = Duration::from_millis(16);

/// Launch the interactive TUI. Blocks until the user quits.
pub fn run_tui(payload: UiPayload) -> Result<(), String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| e.to_string())?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut state = TuiState::new(payload);
    let core = RuntimeCoreBridge::with_defaults();
    let result = run_event_loop(&mut terminal, &mut state, &core);

    // Always restore terminal on exit.
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
    core: &RuntimeCoreBridge,
) -> Result<(), String> {
    loop {
        state.handle_ui_events();

        if event::poll(Duration::ZERO).map_err(|e| e.to_string())?
            && let Event::Key(key) = event::read().map_err(|e| e.to_string())?
        {
            match state.handle_key_event(key) {
                TuiAction::Quit => break,
                TuiAction::Submit(input) => {
                    let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
                    self::core::handle_submit(state, core, input, working_dir);
                }
                TuiAction::None => {}
            }
        }

        terminal
            .draw(|frame| render::render(frame, state))
            .map_err(|e| e.to_string())?;

        thread::sleep(FRAME_TIME);
    }
    Ok(())
}
