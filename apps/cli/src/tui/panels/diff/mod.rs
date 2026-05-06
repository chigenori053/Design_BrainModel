use crate::tui::state::{Diff, TuiState};

pub fn diff_panel_lines(state: &TuiState) -> Vec<String> {
    let Some(diff) = state.session.diffs.last() else {
        return vec!["No preview available.".to_string()];
    };
    render_diff(diff)
}

fn render_diff(diff: &Diff) -> Vec<String> {
    let mut lines = vec![format!("Target: {}", diff.file), "--- preview".to_string()];
    for change in &diff.changes {
        if let Some(old) = &change.old {
            lines.push(format!("-{old}"));
        }
        if let Some(new) = &change.new {
            let line = if new.starts_with('+') || new.starts_with('-') || new.starts_with(' ') {
                new.clone()
            } else {
                format!("+{new}")
            };
            lines.push(line);
        }
    }
    lines
}
