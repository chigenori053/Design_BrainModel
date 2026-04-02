use std::path::PathBuf;

use super::types::CommandPlan;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConversationState {
    pub autonomous_label: Option<String>,
    pub last_target: Option<PathBuf>,
    pub last_node: Option<String>,
    pub last_plan: Option<CommandPlan>,
    pub last_viewer_session: Option<String>,
    pub last_analysis_summary: Option<String>,
}

impl ConversationState {
    pub fn prompt_label(&self) -> Option<&str> {
        self.autonomous_label.as_deref().or_else(|| {
            self.last_node.as_deref().or_else(|| {
                self.last_target
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
            })
        })
    }
}
