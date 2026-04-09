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

pub fn extract_explicit_path(input: &str) -> Option<PathBuf> {
    extract_target_from_cli_flag(input)
        .or_else(|| extract_target_from_explicit_sentence(input))
        .or_else(|| extract_target_from_validated_path_token(input))
}

pub fn has_explicit_target_reference(input: &str) -> bool {
    extract_explicit_path(input).is_some()
}

fn extract_target_from_cli_flag(input: &str) -> Option<PathBuf> {
    let tokens = input.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(2) {
        if window[0] == "--target" {
            let token = trim_target_token(window[1]);
            if is_valid_target_token(token) {
                return Some(PathBuf::from(token));
            }
        }
    }
    None
}

fn extract_target_from_explicit_sentence(input: &str) -> Option<PathBuf> {
    for line in input.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("対象は") {
            let token = trim_target_token(rest);
            if is_valid_target_token(token) {
                return Some(PathBuf::from(token));
            }
        }
        if let Some(rest) = trimmed.strip_prefix("target:") {
            let token = trim_target_token(rest);
            if is_valid_target_token(token) {
                return Some(PathBuf::from(token));
            }
        }
        if let Some(rest) = trimmed.strip_prefix("Target:") {
            let token = trim_target_token(rest);
            if is_valid_target_token(token) {
                return Some(PathBuf::from(token));
            }
        }
    }
    None
}

fn extract_target_from_validated_path_token(input: &str) -> Option<PathBuf> {
    for raw in input.split_whitespace() {
        let token = trim_target_token(raw);
        if token.is_empty() || !is_valid_target_token(token) {
            continue;
        }
        if is_specific_path_token(token) || Path::new(token).exists() {
            return Some(PathBuf::from(token));
        }
    }
    None
}

fn trim_target_token(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                ',' | '。' | '.' | '、' | ':' | ';' | '"' | '\'' | '「' | '」' | '(' | ')' | '>'
            )
        })
        .trim()
}

fn is_valid_target_token(token: &str) -> bool {
    !token.is_empty() && !contains_wildcard(token) && is_path_like(token)
}

fn is_specific_path_token(token: &str) -> bool {
    token.contains("src/")
        || token.contains("apps/")
        || token.contains("crates/")
        || token.ends_with(".rs")
        || token.ends_with(".toml")
        || token.ends_with(".json")
        || token.ends_with(".md")
}

fn contains_wildcard(token: &str) -> bool {
    token.contains('*') || token.contains('?')
}

fn is_path_like(token: &str) -> bool {
    token.contains('/')
        || token.starts_with('.')
        || token.ends_with(".rs")
        || token.ends_with(".toml")
        || token.ends_with(".json")
        || token.ends_with(".md")
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
    fn wildcard_suffix_is_not_an_explicit_target() {
        let session = AgentSession::new();
        let target = resolve_target(
            "ImportRebinding-only の diff では *_interface.rs を生成しない",
            &session,
        );
        assert_eq!(target.path, PathBuf::from("."));
    }

    #[test]
    fn explicit_target_sentence_is_preferred() {
        let session = AgentSession::new();
        let target = resolve_target("対象は apps/cli/src/coding.rs。", &session);
        assert_eq!(target.path, PathBuf::from("apps/cli/src/coding.rs"));
    }

    #[test]
    fn falls_back_to_last_path() {
        let mut session = AgentSession::new();
        session.context.set_last_path("src/lib.rs");
        let target = resolve_target("unsafe を減らして", &session);
        assert_eq!(target.path, PathBuf::from("src/lib.rs"));
    }
}
