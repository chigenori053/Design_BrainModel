use crate::model::UiState;
use crate::tui;
use crate::vm_client::HybridVmClient;
use crate::view::{
    HeaderView, HeaderProps,
    CurrentDecisionView, CurrentDecisionProps,
    ExplanationView, ExplanationProps,
    DecisionHistoryView, HistoryProps,
    EventInputView, EventInputProps
};
use std::time::{Duration, Instant};
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout};

pub struct AppRoot {
    vm_client: HybridVmClient,
    ui_state: UiState,
}

impl AppRoot {
    pub fn new() -> Self {
        Self {
            vm_client: HybridVmClient::new(),
            ui_state: UiState::new(),
        }
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        let mut terminal = tui::init()?;
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();

        loop {
            // Draw
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1), // Header
                        Constraint::Length(10), // Current Decision
                        Constraint::Min(5),    // Explanation (Flexible)
                        Constraint::Length(8), // History
                        Constraint::Length(3), // Input
                    ])
                    .split(f.size());

                if let Some(decision) = &self.ui_state.latest_decision {
                     HeaderView::render(f, chunks[0], &HeaderProps {
                         system_status: "Online".to_string(), // In real app, derived from connection success
                         decision_status: decision.status.clone(),
                         human_override: decision.human_override,
                     });

                     CurrentDecisionView::render(f, chunks[1], &CurrentDecisionProps {
                         decision: decision.clone(),
                     });

                     ExplanationView::render(f, chunks[2], &ExplanationProps {
                         explanation_text: decision.explanation.clone(),
                     });
                } else {
                    // Fallback if no decision yet (should be covered by "Connecting..." mock in client)
                }

                DecisionHistoryView::render(f, chunks[3], &HistoryProps {
                    history: self.ui_state.decision_history.clone(),
                });

                EventInputView::render(f, chunks[4], &EventInputProps {
                    input_buffer: self.ui_state.input_buffer.clone(),
                });
            })?;

            // Handle Input
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => break,
                        KeyCode::Char(c) => self.ui_state.input_buffer.push(c),
                        KeyCode::Backspace => {
                            self.ui_state.input_buffer.pop();
                        }
                        KeyCode::Enter => {
                            let input = self.ui_state.input_buffer.drain(..).collect::<String>();
                            log::info!("User Input: {}", input);
                            if let Some(event) = EventInputView::parse_command(&input) {
                                self.vm_client.send_event(event);
                                // Trigger immediate refresh potentially
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Periodic Updates
            if last_tick.elapsed() >= tick_rate {
                self.refresh_state();
                last_tick = Instant::now();
            }
        }

        tui::restore()?;
        Ok(())
    }

    fn refresh_state(&mut self) {
        self.ui_state.latest_decision = Some(self.vm_client.fetch_latest_decision());
        self.ui_state.decision_history = self.vm_client.fetch_history();
    }
}
