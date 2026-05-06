use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeIntent {
    Analyze,
    Preview,
    Apply,
    Rollback,
    Replay,
    GitStatus,
    GitDiff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeIntentCommand {
    pub intent: RuntimeIntent,
    pub target: Option<PathBuf>,
}

impl RuntimeIntentCommand {
    pub fn new(intent: RuntimeIntent, target: Option<PathBuf>) -> Self {
        Self { intent, target }
    }

    pub fn to_runtime_input(&self) -> String {
        match self.intent {
            RuntimeIntent::Analyze => self
                .target
                .as_ref()
                .map(|target| format!("analyze {}", target.display()))
                .unwrap_or_else(|| "analyze".to_string()),
            RuntimeIntent::Preview => self
                .target
                .as_ref()
                .map(|target| format!("preview {}", target.display()))
                .unwrap_or_else(|| "preview".to_string()),
            RuntimeIntent::Apply => "apply".to_string(),
            RuntimeIntent::Rollback => "rollback".to_string(),
            RuntimeIntent::Replay => "replay".to_string(),
            RuntimeIntent::GitStatus => "git status".to_string(),
            RuntimeIntent::GitDiff => "git diff".to_string(),
        }
    }
}
