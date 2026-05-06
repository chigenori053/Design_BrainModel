use crate::nl::language::language_label;
use crate::tui::state::TuiState;

pub fn runtime_panel_lines(state: &TuiState) -> Vec<String> {
    vec![
        format!("State: {}", state.runtime_state.label()),
        format!(
            "Target: {}",
            state
                .active_transaction
                .as_ref()
                .map(|tx| tx.target_path.as_str())
                .unwrap_or("(none)")
        ),
        format!(
            "Transaction: {}",
            state
                .active_transaction
                .as_ref()
                .map(|tx| tx.tx_id.as_str())
                .unwrap_or("(none)")
        ),
        format!("Dirty tree: {}", state.dirty_tree_state),
        format!("Language: {}", language_label(state.language_mode)),
    ]
}
