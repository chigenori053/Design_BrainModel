use std::path::PathBuf;

use crate::session::AgentSession;

use super::session::ConversationState;
use super::target::resolve_target;
use super::types::ResolvedTarget;

pub fn merge_target(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> ResolvedTarget {
    let current = resolve_target(input, session);
    let has_explicit = has_explicit_target(input);

    ResolvedTarget {
        path: if has_explicit {
            current.path
        } else if current.path != PathBuf::from(".") {
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

fn has_explicit_target(input: &str) -> bool {
    input.split_whitespace().any(|token| {
        token.contains('/')
            || token.starts_with('.')
            || token.ends_with(".rs")
            || token.ends_with(".toml")
    })
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
}
