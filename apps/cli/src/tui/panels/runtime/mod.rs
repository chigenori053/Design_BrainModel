use crate::nl::language::language_label;
use crate::tui::state::TuiState;

pub fn runtime_panel_lines(state: &TuiState) -> Vec<String> {
    vec![
        format!("State: {}", state.runtime_state.label()),
        format!(
            "Target: {}",
            state.active_target.as_deref().unwrap_or("(none)")
        ),
        format!(
            "Transaction: {}",
            state.active_transaction_id.as_deref().unwrap_or("(none)")
        ),
        format!("Dirty tree: {}", state.dirty_tree_state),
        format!("Language: {}", language_label(state.language_mode)),
    ]
}
