use crate::tui::panels::diff::diff_panel_lines;
use crate::tui::panels::runtime::runtime_panel_lines;
use crate::tui::state::TuiState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderState {
    pub runtime_state: crate::runtime::runtime_state::RuntimeState,
    pub debug_events: Vec<crate::runtime::runtime_events::DebugEvent>,
    pub diff_state: DiffState,
    pub status_state: StatusState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffState {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusState {
    pub line: String,
}

impl From<&TuiState> for RenderState {
    fn from(state: &TuiState) -> Self {
        Self {
            runtime_state: state.runtime_state,
            debug_events: state.debug_events.clone(),
            diff_state: DiffState {
                lines: diff_panel_lines(state),
            },
            status_state: StatusState {
                line: state.status_line(),
            },
        }
    }
}

pub fn render_runtime_text(state: &TuiState) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("+--------------------------------------------------+".to_string());
    lines.push("| Input / Intent                                   |".to_string());
    lines.push("+-------------------+------------------------------+".to_string());
    for line in runtime_panel_lines(state) {
        lines.push(format!("| {:<17} | {:<28} |", truncate(&line, 17), ""));
    }
    lines.push("+-------------------+------------------------------+".to_string());
    for line in diff_panel_lines(state) {
        lines.push(format!("| {:<48} |", truncate(&line, 48)));
    }
    lines.push("+--------------------------------------------------+".to_string());
    lines.push(format!("| {} |", truncate(&state.status_line(), 48)));
    lines.push("+--------------------------------------------------+".to_string());
    lines
}

fn truncate(input: &str, width: usize) -> String {
    input.chars().take(width).collect()
}
