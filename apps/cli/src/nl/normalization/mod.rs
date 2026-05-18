use std::path::PathBuf;

use crate::nl::language::detect_runtime_language;
use crate::nl::runtime_intent::{RuntimeIntent, RuntimeIntentCommand};
use crate::nl::types::SupportedLanguage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRuntimeInput {
    pub raw: String,
    pub language: SupportedLanguage,
    pub command: RuntimeIntentCommand,
}

pub fn normalize_runtime_input(input: &str) -> Option<NormalizedRuntimeInput> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    let language = detect_runtime_language(raw);
    let lower = raw.to_ascii_lowercase();

    let intent = if lower == "git status" {
        RuntimeIntent::GitStatus
    } else if lower == "git diff" {
        RuntimeIntent::GitDiff
    } else if lower == "undo" {
        RuntimeIntent::Rollback
    } else if lower.starts_with("preview")
        || raw.contains(" preview")
        || raw.contains("を preview")
        || lower.contains("generate")
        || lower.contains("create")
        || lower.contains("implement")
        || raw.contains("生成")
        || raw.contains("作成")
        || raw.contains("実装")
        || raw.contains("修正")
    {
        RuntimeIntent::Preview
    } else if lower == "apply"
        || lower.contains("apply this")
        || raw.contains("この変更を apply")
        || raw.contains("変更を apply")
    {
        RuntimeIntent::Apply
    } else if lower.starts_with("rollback") || raw.contains("rollback") {
        RuntimeIntent::Rollback
    } else if lower.starts_with("replay") || raw.contains("replay") {
        RuntimeIntent::Replay
    } else if lower.starts_with("analyze") || raw.contains("解析") {
        RuntimeIntent::Analyze
    } else if lower.contains("safely") || raw.contains("安全に") {
        RuntimeIntent::Preview
    } else {
        return None;
    };

    Some(NormalizedRuntimeInput {
        raw: raw.to_string(),
        language,
        command: RuntimeIntentCommand::new(intent, extract_target(raw, intent)),
    })
}

fn extract_target(input: &str, intent: RuntimeIntent) -> Option<PathBuf> {
    match intent {
        RuntimeIntent::Analyze | RuntimeIntent::Preview => {}
        _ => return None,
    }
    input
        .split_whitespace()
        .find(|token| looks_like_path(token))
        .map(clean_target)
        .filter(|target| !target.is_empty())
        .map(PathBuf::from)
}

fn looks_like_path(token: &str) -> bool {
    let cleaned = clean_target(token);
    cleaned.contains('.') || cleaned.contains('/')
}

fn clean_target(token: &str) -> String {
    token
        .trim_matches(|ch: char| {
            matches!(
                ch,
                '"' | '\'' | '`' | '「' | '」' | '『' | '』' | ',' | '.' | '、' | '。'
            )
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_english_preview() {
        let normalized = normalize_runtime_input("preview parser.rs").expect("intent");
        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(normalized.command.target, Some(PathBuf::from("parser.rs")));
        assert_eq!(normalized.command.to_runtime_input(), "preview parser.rs");
    }

    #[test]
    fn normalizes_japanese_preview_to_same_intent() {
        let en = normalize_runtime_input("preview parser.rs").expect("en");
        let ja = normalize_runtime_input("parser.rs を preview").expect("ja");

        assert_eq!(en.command.intent, ja.command.intent);
        assert_eq!(en.command.target, ja.command.target);
    }

    #[test]
    fn normalizes_apply_variants() {
        assert_eq!(
            normalize_runtime_input("apply this diff")
                .expect("en")
                .command
                .intent,
            RuntimeIntent::Apply
        );
        assert_eq!(
            normalize_runtime_input("この変更を apply")
                .expect("ja")
                .command
                .intent,
            RuntimeIntent::Apply
        );
    }

    #[test]
    fn normalizes_japanese_generate_target_as_preview_intent() {
        let normalized =
            normalize_runtime_input("apps/cli/src/test_runtime.rs を生成。").expect("intent");

        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/test_runtime.rs"))
        );
    }

    #[test]
    fn normalizes_empty_japanese_fix_as_unresolved_preview_intent() {
        let normalized = normalize_runtime_input("修正してください").expect("intent");

        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(normalized.command.target, None);
    }
}
