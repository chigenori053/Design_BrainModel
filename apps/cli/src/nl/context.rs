use std::path::PathBuf;

use crate::session::AgentSession;

use super::session::ConversationState;
use super::target::{has_explicit_target_reference, resolve_target};
use super::types::ResolvedTarget;

pub fn merge_target(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> ResolvedTarget {
    let current = resolve_target(input, session);
    let has_explicit = has_explicit_target_reference(input);

    ResolvedTarget {
        path: if has_explicit || current.path != PathBuf::from(".") {
            current.path
        } else if let Some(path) = conversation.last_target.clone() {
            path
        } else {
            PathBuf::from(".")
        },
        node: current.node.or_else(|| conversation.last_node.clone()),
        scope: current.scope,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_target_is_used_for_ambiguous_turn() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli")),
            last_node: Some("presentation".to_string()),
            ..ConversationState::default()
        };
        let merged = merge_target("presentation layer だけ直して", &session, &conversation);
        assert_eq!(merged.path, PathBuf::from("apps/cli"));
        assert_eq!(merged.node.as_deref(), Some("presentation"));
    }

    #[test]
    fn wildcard_suffix_in_prose_does_not_override_last_target() {
        let session = AgentSession::new();
        let conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };
        let merged = merge_target(
            "ImportRebinding-only の diff では *_interface.rs を生成しない。",
            &session,
            &conversation,
        );
        assert_eq!(merged.path, PathBuf::from("apps/cli/src/coding.rs"));
    }
}
