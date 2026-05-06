use std::path::Path;

use super::commands::GitCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPolicy {
    SafeRead,
    SafeWrite,
    Dangerous,
}

pub fn classify(command: &GitCommand) -> CommandPolicy {
    match command {
        GitCommand::Status | GitCommand::Diff | GitCommand::Log => CommandPolicy::SafeRead,
        GitCommand::AddFile(path) if path == Path::new(".") => CommandPolicy::Dangerous,
        GitCommand::AddFile(_) | GitCommand::Commit { .. } => CommandPolicy::SafeWrite,
    }
}
