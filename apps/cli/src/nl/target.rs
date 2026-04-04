use std::path::{Path, PathBuf};

use crate::nl::types::ResolvedTarget;
use crate::session::AgentSession;

pub fn resolve_target(input: &str, session: &AgentSession) -> ResolvedTarget {
    let mut target = ResolvedTarget {
        path: resolve_path(input, session),
        node: None,
        scope: None,
    };

    let lower = input.to_lowercase();
    if lower.contains("このプロジェクト") || lower.contains("project") || lower.contains("全体")
    {
        target.scope = Some("project".to_string());
    }
    if lower.contains("viewer") || lower.contains("gui") {
        target.node = Some("viewer".to_string());
    } else if lower.contains("rules engine") || lower.contains("rules") {
        target.node = Some("rules".to_string());
    }

    target
}

fn resolve_path(input: &str, session: &AgentSession) -> PathBuf {
    if let Some(explicit) = extract_explicit_path(input) {
        return explicit;
    }

    let lower = input.to_lowercase();
    if lower.contains("このプロジェクト") || lower.contains("project") || lower.contains("全体")
    {
        return PathBuf::from(".");
    }
    if lower.contains("apps/cli") {
        return PathBuf::from("apps/cli");
    }

    session
        .context
        .last_path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn extract_explicit_path(input: &str) -> Option<PathBuf> {
    for raw in input.split_whitespace() {
        let token = raw.trim_matches(|c: char| {
            matches!(
                c,
                ',' | '。' | '.' | '、' | ':' | ';' | '"' | '\'' | '「' | '」' | '(' | ')'
            )
        });
        if token.is_empty() {
            continue;
        }
        if is_path_like(token) {
            return Some(PathBuf::from(token));
        }
    }
    None
}

fn is_path_like(token: &str) -> bool {
    token.contains('/')
        || token.starts_with('.')
        || token.ends_with(".rs")
        || token.ends_with(".toml")
        || token.ends_with(".json")
        || Path::new(token).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_project_phrase_to_dot() {
        let session = AgentSession::new();
        let target = resolve_target("このプロジェクト全体を解析して", &session);
        assert_eq!(target.path, PathBuf::from("."));
        assert_eq!(target.scope.as_deref(), Some("project"));
    }

    #[test]
    fn resolves_explicit_path() {
        let session = AgentSession::new();
        let target = resolve_target("apps/cli を解析して", &session);
        assert_eq!(target.path, PathBuf::from("apps/cli"));
    }

    #[test]
    fn falls_back_to_last_path() {
        let mut session = AgentSession::new();
        session.context.set_last_path("src/lib.rs");
        let target = resolve_target("unsafe を減らして", &session);
        assert_eq!(target.path, PathBuf::from("src/lib.rs"));
    }
}
