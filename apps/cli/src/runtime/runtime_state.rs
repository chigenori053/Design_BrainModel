pub use crate::tui::runtime::RuntimeShellState as RuntimeState;

pub fn initial_runtime_state() -> RuntimeState {
    RuntimeState::Idle
}
