use crate::core::CoreRequestKind;
use crate::git::commands::GitCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingResult {
    Core(CoreRequestKind),
    Git(GitCommand),
    NaturalLanguage,
    Rejected(String),
}
