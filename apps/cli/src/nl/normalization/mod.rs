use std::path::PathBuf;

use crate::nl::language::detect_runtime_language;
use crate::nl::runtime_intent::{MutationOperation, RuntimeIntent, RuntimeIntentCommand};
use crate::nl::types::SupportedLanguage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRuntimeInput {
    pub raw: String,
    pub language: SupportedLanguage,
    pub target: Option<PathBuf>,
    pub validated_target: Option<PathBuf>,
    pub command: RuntimeIntentCommand,
    pub rejection: Option<RuntimeNormalizationRejection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeNormalizationRejection {
    AmbiguousTarget,
    UnresolvedTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetResolutionFailure {
    ConfirmationTokenLike { raw: String },
    Unresolved { raw: Option<String> },
}

pub fn normalize_runtime_input(input: &str) -> Option<NormalizedRuntimeInput> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    let language = detect_runtime_language(raw);
    let lower = raw.to_ascii_lowercase();
    let sentences = segment_sentences(raw);

    let intent = if lower == "git status"
        || raw == "git status を確認"
        || raw == "git status確認"
        || raw == "状態を確認"
    {
        RuntimeIntent::GitStatus
    } else if lower == "git diff" || raw == "差分を確認" {
        RuntimeIntent::GitDiff
    } else if lower == "undo" {
        RuntimeIntent::Rollback
    } else if lower == "apply"
        || lower.contains("apply this")
        || raw.contains("この変更を apply")
        || raw.contains("変更を apply")
    {
        RuntimeIntent::Apply
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
        || has_mutation_sentence(&sentences)
    {
        RuntimeIntent::Preview
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
    let mut aggregation = aggregate_sentences(&sentences, intent);
    let validated_target = aggregation
        .target
        .as_ref()
        .and_then(validated_authority_target);
    if matches!(intent, RuntimeIntent::Preview | RuntimeIntent::Analyze)
        && !aggregation.operations.is_empty()
        && validated_target.is_none()
        && aggregation.rejection.is_none()
    {
        aggregation.rejection = Some(RuntimeNormalizationRejection::UnresolvedTarget);
    }

    Some(NormalizedRuntimeInput {
        raw: raw.to_string(),
        language,
        target: aggregation.target.clone(),
        validated_target: validated_target.clone(),
        command: RuntimeIntentCommand::with_operations(
            intent,
            validated_target,
            aggregation.operations,
        ),
        rejection: aggregation.rejection,
    })
}

pub fn target_only_input_target(input: &str) -> Option<PathBuf> {
    target_only_input_resolution(input).ok().flatten()
}

pub fn target_only_input_resolution(
    input: &str,
) -> Result<Option<PathBuf>, TargetResolutionFailure> {
    let raw = input.trim();
    if raw.is_empty() || target_only_has_mutation(raw) {
        return Ok(None);
    }

    let target_text = raw
        .strip_prefix("Target:")
        .or_else(|| raw.strip_prefix("target:"))
        .or_else(|| raw.strip_prefix("対象:"))
        .map(str::trim)
        .unwrap_or(raw);

    if target_text.is_empty() || target_text.split_whitespace().count() != 1 {
        return Ok(None);
    }

    let target = clean_target(target_text);
    if crate::nl::language_core_ir_adapter::is_confirmation_token_like_target(&target) {
        eprintln!("[IR-TRACE][TARGET_REJECTED] target={target} reason=ConfirmationTokenLike");
        return Err(TargetResolutionFailure::ConfirmationTokenLike { raw: target });
    }
    if target.is_empty() || !looks_like_explicit_target_token(&target) {
        return Ok(None);
    }

    Ok(Some(PathBuf::from(target)))
}

pub fn confirmation_like_target_failure(input: &str) -> Option<TargetResolutionFailure> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    let sentences = segment_sentences(raw);
    for sentence in sentences {
        if let Some(failure) = extract_header_target_failure(&sentence) {
            return Some(failure);
        }
        if let Some(failure) = extract_sentence_target_failure(&sentence) {
            return Some(failure);
        }
    }
    None
}

fn target_only_has_mutation(input: &str) -> bool {
    let sentences = segment_sentences(input);
    has_mutation_sentence(&sentences)
        || sentences
            .iter()
            .any(|sentence| has_explicit_operation_marker(sentence))
}

fn validated_authority_target(target: &PathBuf) -> Option<PathBuf> {
    if target.as_os_str().is_empty()
        || target == &PathBuf::from(".")
        || target.is_absolute()
        || target.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        return None;
    }
    Some(target.clone())
}

/// Extracts a target only from an explicit header sentence of the form
/// `Target: <path>`, `target: <path>`, or `対象: <path>`.
///
/// Unlike [`target_only_input_target`], this function:
/// - Requires the prefix to be present (no bare-path fallback).
/// - Does not run the mutation-keyword filter, making it safe to apply to
///   individual sentences drawn from a multi-sentence context.
fn extract_header_target(sentence: &str) -> Option<PathBuf> {
    let raw = sentence.trim();
    let target_text = raw
        .strip_prefix("Target:")
        .or_else(|| raw.strip_prefix("target:"))
        .or_else(|| raw.strip_prefix("対象:"))?
        .trim();
    if target_text.is_empty() || target_text.split_whitespace().count() != 1 {
        return None;
    }
    let target = clean_target(target_text);
    if crate::nl::language_core_ir_adapter::is_confirmation_token_like_target(&target) {
        eprintln!("[IR-TRACE][TARGET_REJECTED] target={target} reason=ConfirmationTokenLike");
        return None;
    }
    if target.is_empty() || !looks_like_explicit_target_token(&target) {
        return None;
    }
    Some(PathBuf::from(target))
}

fn extract_header_target_failure(sentence: &str) -> Option<TargetResolutionFailure> {
    let raw = sentence.trim();
    let target_text = raw
        .strip_prefix("Target:")
        .or_else(|| raw.strip_prefix("target:"))
        .or_else(|| raw.strip_prefix("対象:"))?
        .trim();
    if target_text.is_empty() || target_text.split_whitespace().count() != 1 {
        return None;
    }
    confirmation_like_failure(clean_target(target_text))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceSemanticKind {
    ExplicitTarget,
    OperationOnly,
    Mixed,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceSemanticUnit {
    pub source: String,
    pub kind: SentenceSemanticKind,
    pub explicit_target: Option<String>,
    pub inherited_target: Option<String>,
    pub operations: Vec<MutationOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSemanticAuthority {
    pub explicit_target: Option<String>,
    pub source_sentence_index: usize,
}

struct IntentAggregation {
    target: Option<PathBuf>,
    operations: Vec<MutationOperation>,
    rejection: Option<RuntimeNormalizationRejection>,
}

fn aggregate_sentences(sentences: &[String], intent: RuntimeIntent) -> IntentAggregation {
    if !matches!(intent, RuntimeIntent::Analyze | RuntimeIntent::Preview) {
        return IntentAggregation {
            target: None,
            operations: Vec::new(),
            rejection: None,
        };
    }

    // Pre-scan: if any sentence carries an explicit "Target: <path>" header, lock the
    // authority target immediately.  All remaining sentences are then treated as
    // operation-only so that incidental path-like tokens in prose (e.g.
    // "see apps/cli/src/other.rs for context") are never misidentified as target
    // candidates and never trigger AmbiguousTarget rejection.
    if let Some(locked_target) = sentences.iter().find_map(|s| extract_header_target(s)) {
        let mut operations = Vec::new();
        for sentence in sentences {
            if extract_header_target(sentence).is_some() {
                continue; // header sentence defines the target only, not operations
            }
            if intent == RuntimeIntent::Preview {
                operations.extend(extract_sentence_operations(sentence));
            }
        }
        if intent == RuntimeIntent::Preview && operations.is_empty() {
            operations.push(MutationOperation::Modify);
        }
        return IntentAggregation {
            target: Some(locked_target),
            operations,
            rejection: None,
        };
    }

    let mut authority = RuntimeSemanticAuthority {
        explicit_target: None,
        source_sentence_index: 0,
    };
    let mut operations = Vec::new();
    let mut units = Vec::new();
    for (index, sentence) in sentences.iter().enumerate() {
        let mut unit = sentence_semantic_unit(sentence, authority.explicit_target.as_deref());
        match unit.kind {
            SentenceSemanticKind::ExplicitTarget | SentenceSemanticKind::Mixed => {
                let Some(found) = unit.explicit_target.as_deref() else {
                    units.push(unit);
                    continue;
                };
                match authority.explicit_target.as_deref() {
                    Some(existing) if existing != found => {
                        return IntentAggregation {
                            target: None,
                            operations: Vec::new(),
                            rejection: Some(RuntimeNormalizationRejection::AmbiguousTarget),
                        };
                    }
                    Some(_) => {}
                    None => {
                        authority.explicit_target = Some(found.to_string());
                        authority.source_sentence_index = index;
                    }
                }
                if intent == RuntimeIntent::Preview {
                    operations.extend(unit.operations.clone());
                }
            }
            SentenceSemanticKind::OperationOnly => {
                unit.inherited_target = authority.explicit_target.clone();
                if intent == RuntimeIntent::Preview && authority.explicit_target.is_some() {
                    operations.extend(unit.operations.clone());
                } else if intent == RuntimeIntent::Preview && !unit.operations.is_empty() {
                    return IntentAggregation {
                        target: None,
                        operations: Vec::new(),
                        rejection: Some(RuntimeNormalizationRejection::UnresolvedTarget),
                    };
                }
            }
            SentenceSemanticKind::Empty => {}
        }
        units.push(unit);
    }

    if authority.explicit_target.is_some() {
        debug_assert!(authority.source_sentence_index < sentences.len());
    }
    let target = authority.explicit_target.map(PathBuf::from);
    if intent == RuntimeIntent::Preview && target.is_some() && operations.is_empty() {
        operations.push(MutationOperation::Modify);
    }
    if !operations.is_empty() && target.is_none() {
        return IntentAggregation {
            target: None,
            operations: Vec::new(),
            rejection: Some(RuntimeNormalizationRejection::UnresolvedTarget),
        };
    }

    IntentAggregation {
        target,
        operations,
        rejection: None,
    }
}

fn sentence_semantic_unit(sentence: &str, inherited_target: Option<&str>) -> SentenceSemanticUnit {
    let explicit_target =
        extract_sentence_target(sentence).map(|target| target.display().to_string());
    let operations = extract_sentence_operations(sentence);
    let kind = classify_sentence_kind_with_parts(sentence, explicit_target.as_deref(), &operations);
    SentenceSemanticUnit {
        source: sentence.to_string(),
        kind,
        explicit_target,
        inherited_target: inherited_target.map(ToString::to_string),
        operations,
    }
}

pub fn classify_sentence_kind(sentence: &str) -> SentenceSemanticKind {
    let explicit_target = extract_sentence_target(sentence);
    let operations = extract_sentence_operations(sentence);
    classify_sentence_kind_with_parts(
        sentence,
        explicit_target
            .as_ref()
            .map(|target| target.to_string_lossy())
            .as_deref(),
        &operations,
    )
}

fn classify_sentence_kind_with_parts(
    sentence: &str,
    explicit_target: Option<&str>,
    operations: &[MutationOperation],
) -> SentenceSemanticKind {
    let has_target = explicit_target.is_some();
    let has_operation = !operations.is_empty();
    if has_target && has_operation && has_explicit_operation_marker(sentence) {
        SentenceSemanticKind::Mixed
    } else if has_target {
        SentenceSemanticKind::ExplicitTarget
    } else if has_operation {
        SentenceSemanticKind::OperationOnly
    } else {
        SentenceSemanticKind::Empty
    }
}

fn has_explicit_operation_marker(sentence: &str) -> bool {
    let lower = sentence.to_ascii_lowercase();
    lower.contains("add")
        || lower.contains("insert")
        || lower.contains("replace")
        || lower.contains("delete")
        || lower.contains("edit")
        || lower.contains("modify")
        || lower.contains("generate")
        || lower.contains("create")
        || lower.contains("implement")
        || lower.contains("call")
        || lower.contains("invoke")
        || sentence.contains("追加")
        || sentence.contains("挿入")
        || sentence.contains("置換")
        || sentence.contains("削除")
        || sentence.contains("呼び出す")
        || sentence.contains("参照")
        || sentence.contains("更新")
        || sentence.contains("変更")
        || sentence.contains("修正")
        || sentence.contains("生成")
        || sentence.contains("作成")
        || sentence.contains("実装")
}

pub fn build_sentence_semantic_units(sentences: &[String]) -> Vec<SentenceSemanticUnit> {
    let mut authority: Option<String> = None;
    sentences
        .iter()
        .map(|sentence| {
            let mut unit = sentence_semantic_unit(sentence, authority.as_deref());
            match unit.kind {
                SentenceSemanticKind::ExplicitTarget | SentenceSemanticKind::Mixed => {
                    if let Some(target) = unit.explicit_target.clone() {
                        authority = Some(target);
                    }
                }
                SentenceSemanticKind::OperationOnly => {
                    unit.inherited_target = authority.clone();
                }
                SentenceSemanticKind::Empty => {}
            }
            unit
        })
        .collect()
}

fn extract_sentence_target(sentence: &str) -> Option<PathBuf> {
    sentence
        .split_whitespace()
        .find(|token| looks_like_explicit_target_token(token))
        .map(clean_target)
        .inspect(|target| {
            if crate::nl::language_core_ir_adapter::is_confirmation_token_like_target(target) {
                eprintln!(
                    "[IR-TRACE][TARGET_REJECTED] target={target} reason=ConfirmationTokenLike"
                );
            }
        })
        .filter(|target| {
            !crate::nl::language_core_ir_adapter::is_confirmation_token_like_target(target)
        })
        .filter(|target| !target.is_empty())
        .map(PathBuf::from)
}

fn extract_sentence_target_failure(sentence: &str) -> Option<TargetResolutionFailure> {
    sentence
        .split_whitespace()
        .filter(|token| looks_like_explicit_target_token(token))
        .map(clean_target)
        .find_map(confirmation_like_failure)
}

fn confirmation_like_failure(target: String) -> Option<TargetResolutionFailure> {
    if crate::nl::language_core_ir_adapter::is_confirmation_token_like_target(&target) {
        eprintln!("[IR-TRACE][TARGET_REJECTED] target={target} reason=ConfirmationTokenLike");
        Some(TargetResolutionFailure::ConfirmationTokenLike { raw: target })
    } else {
        None
    }
}

fn segment_sentences(input: &str) -> Vec<String> {
    let mut segmented = input.replace("\r\n", "\n").replace('\r', "\n");
    segmented = segmented
        .replace("&&", "\n")
        .replace("||", "\n")
        .replace(['。', '、', '；', ';'], "\n");
    segmented = split_ascii_sentence_markers(&segmented);
    segmented
        .lines()
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn split_ascii_sentence_markers(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        if starts_with_word_boundary(&chars, index, "and then") {
            output.push('\n');
            index += "and then".len();
            continue;
        }
        if starts_with_word_boundary(&chars, index, "then") {
            output.push('\n');
            index += "then".len();
            continue;
        }
        output.push(chars[index]);
        if chars[index] == '.'
            && chars
                .get(index + 1)
                .is_some_and(|next| next.is_whitespace())
        {
            output.push('\n');
        }
        index += 1;
    }
    output
}

fn starts_with_word_boundary(chars: &[char], index: usize, needle: &str) -> bool {
    let end = index + needle.len();
    if end > chars.len() {
        return false;
    }
    let candidate = chars[index..end]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase();
    if candidate != needle {
        return false;
    }
    let before_ok = index == 0 || !chars[index - 1].is_ascii_alphanumeric();
    let after_ok = end == chars.len() || !chars[end].is_ascii_alphanumeric();
    before_ok && after_ok
}

fn has_mutation_sentence(sentences: &[String]) -> bool {
    sentences.iter().any(|sentence| {
        let lower = sentence.to_ascii_lowercase();
        lower.contains("add")
            || lower.contains("insert")
            || lower.contains("replace")
            || lower.contains("delete")
            || lower.contains("edit")
            || lower.contains("modify")
            || lower.contains("generate")
            || lower.contains("create")
            || lower.contains("implement")
            || lower.contains("const")
            || lower.contains("call")
            || lower.contains("invoke")
            || sentence.contains("()")
            || sentence.contains("追加")
            || sentence.contains("挿入")
            || sentence.contains("置換")
            || sentence.contains("削除")
            || sentence.contains("呼び出す")
            || sentence.contains("参照")
            || sentence.contains("更新")
            || sentence.contains("修正")
            || sentence.contains("変更")
            || sentence.contains("生成")
            || sentence.contains("作成")
            || sentence.contains("実装")
    })
}

fn extract_sentence_operations(sentence: &str) -> Vec<MutationOperation> {
    let mut operations = Vec::new();
    let lower = sentence.to_ascii_lowercase();
    if lower.contains("replace") || sentence.contains("置換") {
        operations.push(MutationOperation::ReplaceBlock {
            text: sentence.to_string(),
        });
    } else if lower.contains("delete") || sentence.contains("削除") {
        operations.push(MutationOperation::DeleteLine {
            text: sentence.to_string(),
        });
    } else if let Some(name) = extract_const_name(sentence) {
        operations.push(MutationOperation::AddConst { name });
    } else if lower.contains("insert")
        || lower.contains("add")
        || lower.contains("call")
        || lower.contains("invoke")
        || sentence.contains("()")
        || sentence.contains("追加")
        || sentence.contains("挿入")
        || sentence.contains("呼び出す")
        || sentence.contains("参照")
        || sentence.contains("更新")
        || sentence.contains("変更")
    {
        operations.push(MutationOperation::InsertLine {
            text: sentence.to_string(),
        });
    } else if lower.contains("modify")
        || lower.contains("preview")
        || lower.contains("generate")
        || lower.contains("create")
        || lower.contains("implement")
        || sentence.contains("修正")
        || sentence.contains("生成")
        || sentence.contains("作成")
        || sentence.contains("実装")
    {
        operations.push(MutationOperation::Modify);
    }
    operations
}

fn extract_const_name(sentence: &str) -> Option<String> {
    if !sentence.to_ascii_lowercase().contains("const") && !sentence.contains("定数") {
        return None;
    }
    sentence.split_whitespace().map(clean_target).find(|token| {
        !token.is_empty()
            && token
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
            && token.chars().any(|ch| ch == '_')
    })
}

fn looks_like_explicit_target_token(token: &str) -> bool {
    let cleaned = clean_target(token);
    if cleaned.contains('(') || cleaned.contains(')') {
        return false;
    }
    cleaned.contains('/')
        || [".rs", ".toml", ".md", ".json", ".yaml", ".yml", ".txt"]
            .iter()
            .any(|extension| cleaned.ends_with(extension))
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
    fn target_only_input_extracts_target_without_mutation() {
        assert_eq!(
            target_only_input_target("Target: apps/cli/src/git_guard.rs"),
            Some(PathBuf::from("apps/cli/src/git_guard.rs"))
        );
        assert_eq!(
            target_only_input_target("target: apps/cli/src/git_guard.rs"),
            Some(PathBuf::from("apps/cli/src/git_guard.rs"))
        );
        assert_eq!(
            target_only_input_target("対象: apps/cli/src/git_guard.rs"),
            Some(PathBuf::from("apps/cli/src/git_guard.rs"))
        );
        assert_eq!(
            target_only_input_target("apps/cli/src/git_guard.rs"),
            Some(PathBuf::from("apps/cli/src/git_guard.rs"))
        );
    }

    #[test]
    fn target_only_input_rejects_mutation_instruction() {
        assert_eq!(
            target_only_input_target("apps/cli/src/git_guard.rs を修正"),
            None
        );
        assert_eq!(
            target_only_input_target("apps/cli/src/git_guard.rs に関数を追加"),
            None
        );
        assert_eq!(
            target_only_input_target("apps/cli/src/git_guard.rs の parser を変更"),
            None
        );
    }

    #[test]
    fn confirmation_like_target_rejection_returns_typed_failure() {
        assert_eq!(
            target_only_input_resolution("Target: yes/no"),
            Err(TargetResolutionFailure::ConfirmationTokenLike {
                raw: "yes/no".to_string()
            })
        );
    }

    #[test]
    fn confirmation_like_target_rejection_does_not_return_plain_none() {
        assert!(matches!(
            target_only_input_resolution("Target: yes/no"),
            Err(TargetResolutionFailure::ConfirmationTokenLike { .. })
        ));
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
        assert!(normalized.command.operations.is_empty());
    }

    #[test]
    fn normalizes_multi_statement_target_and_operations() {
        let normalized = normalize_runtime_input(
            "apps/cli/src/repl.rs を修正。\nREPL_RUNTIME_TEST_5 const を追加。\nvalidate_runtime() を追加。",
        )
        .expect("intent");

        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert!(normalized.command.operations.len() > 1);
        assert!(
            normalized
                .command
                .operations
                .contains(&MutationOperation::Modify)
        );
        assert!(
            normalized
                .command
                .operations
                .contains(&MutationOperation::AddConst {
                    name: "REPL_RUNTIME_TEST_5".to_string()
                })
        );
        assert!(normalized.command.operations.iter().any(|operation| {
            matches!(
                operation,
                MutationOperation::InsertLine { text } if text.contains("validate_runtime()")
            )
        }));
    }

    #[test]
    fn validated_target_matches_explicit_authority() {
        let normalized =
            normalize_runtime_input("apps/cli/src/repl.rs を修正。\nvalidate_runtime() を追加。")
                .expect("intent");

        assert_eq!(
            normalized.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert_eq!(
            normalized.validated_target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
    }

    #[test]
    fn operation_sentences_inherit_current_target_context() {
        let normalized = normalize_runtime_input(
            "apps/cli/src/repl.rs を修正。\nREPL_RUNTIME_TEST_5 const を追加。\nvalidate_runtime() を追加。",
        )
        .expect("intent");

        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert!(normalized.rejection.is_none());
        assert!(normalized.command.operations.len() >= 2);
    }

    #[test]
    fn multi_sentence_operation_only_inherits_target() {
        let input = "apps/cli/src/repl.rs を修正。\nvalidate_runtime() を追加。\nDisplay impl から validate_runtime() を呼び出す。";
        let sentences = segment_sentences(input);
        let units = build_sentence_semantic_units(&sentences);
        let normalized = normalize_runtime_input(input).expect("intent");

        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert_eq!(normalized.command.operations.len(), 3);
        assert!(normalized.rejection.is_none());
        assert_eq!(units[0].kind, SentenceSemanticKind::Mixed);
        assert_eq!(units[1].kind, SentenceSemanticKind::OperationOnly);
        assert_eq!(units[2].kind, SentenceSemanticKind::OperationOnly);
        assert_eq!(
            units[1].inherited_target.as_deref(),
            Some("apps/cli/src/repl.rs")
        );
        assert_eq!(
            units[2].inherited_target.as_deref(),
            Some("apps/cli/src/repl.rs")
        );
    }

    #[test]
    fn function_call_does_not_create_target_candidate() {
        let unit = sentence_semantic_unit("validate_runtime() を追加", None);

        assert_eq!(
            classify_sentence_kind("validate_runtime() を追加"),
            SentenceSemanticKind::OperationOnly
        );
        assert_eq!(unit.kind, SentenceSemanticKind::OperationOnly);
        assert_eq!(unit.explicit_target, None);
    }

    #[test]
    fn impl_reference_does_not_create_target() {
        let unit = sentence_semantic_unit("Display impl から validate_runtime() を呼び出す", None);

        assert_eq!(unit.kind, SentenceSemanticKind::OperationOnly);
        assert_eq!(unit.explicit_target, None);
    }

    #[test]
    fn rejects_ambiguous_multi_statement_targets() {
        let normalized = normalize_runtime_input("a.rs を修正。\nb.rs を修正。").expect("intent");

        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(normalized.command.target, None);
        assert_eq!(
            normalized.rejection,
            Some(RuntimeNormalizationRejection::AmbiguousTarget)
        );
    }

    #[test]
    fn operation_without_target_rejects_unresolved_target() {
        let normalized = normalize_runtime_input("validate_runtime() を追加。").expect("intent");

        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(normalized.command.target, None);
        assert!(normalized.command.operations.is_empty());
        assert_eq!(
            normalized.rejection,
            Some(RuntimeNormalizationRejection::UnresolvedTarget)
        );
    }

    #[test]
    fn operation_only_does_not_generate_workspace_target() {
        let normalized =
            normalize_runtime_input("Display impl から validate_runtime() を呼び出す。")
                .expect("intent");

        assert_eq!(normalized.command.intent, RuntimeIntent::Preview);
        assert_eq!(normalized.command.target, None);
        assert!(normalized.command.operations.is_empty());
        assert_eq!(
            normalized.rejection,
            Some(RuntimeNormalizationRejection::UnresolvedTarget)
        );
    }

    #[test]
    fn validate_runtime_does_not_become_workspace_target() {
        let normalized = normalize_runtime_input("validate_runtime()").expect("intent");

        assert_eq!(normalized.command.target, None);
        assert!(normalized.command.operations.is_empty());
        assert_eq!(
            normalized.rejection,
            Some(RuntimeNormalizationRejection::UnresolvedTarget)
        );
    }

    // --- Long-context target hardening ---

    #[test]
    fn extract_header_target_requires_explicit_prefix() {
        // All three prefix forms are recognized.
        assert_eq!(
            extract_header_target("Target: apps/cli/src/repl.rs"),
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert_eq!(
            extract_header_target("target: apps/cli/src/repl.rs"),
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert_eq!(
            extract_header_target("対象: apps/cli/src/repl.rs"),
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        // Bare path without prefix is NOT treated as a header.
        assert_eq!(extract_header_target("apps/cli/src/repl.rs"), None);
        // Path embedded in prose is NOT treated as a header.
        assert_eq!(
            extract_header_target("See apps/cli/src/repl.rs for context"),
            None
        );
        // Multiple tokens after the prefix → rejected.
        assert_eq!(
            extract_header_target("Target: apps/cli/src/repl.rs extra"),
            None
        );
    }

    #[test]
    fn long_spec_explicit_header_suppresses_body_path_ambiguity() {
        // Without the fix this returns AmbiguousTarget because the path in
        // the "See …" sentence competes with the header path.
        let input = "Target: apps/cli/src/repl.rs\n\
                     Add multiline capture support.\n\
                     See apps/cli/src/other.rs for reference context.";
        let normalized = normalize_runtime_input(input).expect("intent");

        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert!(normalized.rejection.is_none(), "{:?}", normalized.rejection);
    }

    #[test]
    fn long_spec_header_preserves_body_operations() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     Add capture_buf field.\n\
                     Implement the dispatch logic.";
        let normalized = normalize_runtime_input(input).expect("intent");

        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert!(
            !normalized.command.operations.is_empty(),
            "operations must be extracted from body sentences"
        );
        assert!(normalized.rejection.is_none());
    }

    #[test]
    fn long_spec_header_wins_over_multiple_body_paths() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     apps/cli/src/shell.rs や apps/cli/Cargo.toml を参照して実装する。";
        let normalized = normalize_runtime_input(input).expect("intent");

        assert_eq!(
            normalized.command.target,
            Some(PathBuf::from("apps/cli/src/repl.rs"))
        );
        assert!(normalized.rejection.is_none(), "{:?}", normalized.rejection);
    }

    #[test]
    fn long_spec_without_header_still_rejects_ambiguous_paths() {
        // Regression: absence of an explicit header must not change existing behaviour.
        let input = "apps/cli/src/repl.rs を修正。\napps/cli/src/other.rs も修正してください。";
        let normalized = normalize_runtime_input(input).expect("intent");

        assert_eq!(
            normalized.rejection,
            Some(RuntimeNormalizationRejection::AmbiguousTarget)
        );
    }
}
