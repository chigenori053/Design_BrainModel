//! DocumentClassifier
//!
//! DBM-DOCUMENT-CLASSIFIER-SPEC v1.0 に基づく入力種別分類器。
//! LanguageCore に到達する前段で入力を分類し、文書入力の誤処理を防止する。

use std::fmt;

// ── InputKind ────────────────────────────────────────────────────────────────

/// 入力の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// スラッシュコマンドまたは基本操作コマンド (apply, undo, etc.)
    Command,
    /// 自然言語による指示
    NaturalLanguage,
    /// Markdown 形式の文書
    MarkdownDocument,
    /// JSON 形式のデータ
    JsonDocument,
    /// ログ出力またはエラーメッセージ
    LogDocument,
    /// DBM 構造化仕様書 (DBM-*-SPEC)
    StructuredSpec,
    /// 未分類
    Unknown,
}

impl fmt::Display for InputKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command => write!(f, "Command"),
            Self::NaturalLanguage => write!(f, "NaturalLanguage"),
            Self::MarkdownDocument => write!(f, "MarkdownDocument"),
            Self::JsonDocument => write!(f, "JsonDocument"),
            Self::LogDocument => write!(f, "LogDocument"),
            Self::StructuredSpec => write!(f, "StructuredSpec"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

// ── DocumentClassifier ───────────────────────────────────────────────────────

/// 入力文字列を解析し、適切な `InputKind` に分類する。
pub struct DocumentClassifier;

impl DocumentClassifier {
    /// 入力を分類する。
    pub fn classify(input: &str) -> InputKind {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return InputKind::Unknown;
        }

        // 1. Command 判定
        if Self::is_command(trimmed) {
            return InputKind::Command;
        }

        // 2. StructuredSpec 判定
        if Self::is_structured_spec(trimmed) {
            return InputKind::StructuredSpec;
        }

        // 3. JsonDocument 判定
        if Self::is_json(trimmed) {
            return InputKind::JsonDocument;
        }

        // 4. LogDocument 判定
        if Self::is_log(trimmed) {
            return InputKind::LogDocument;
        }

        // 5. MarkdownDocument 判定
        if Self::is_markdown(trimmed) {
            return InputKind::MarkdownDocument;
        }

        // 6. NaturalLanguage (デフォルト)
        // 日本語文字が含まれている、または一般的な文章らしい場合は NL とみなす
        if Self::is_natural_language(trimmed) {
            return InputKind::NaturalLanguage;
        }

        InputKind::Unknown
    }

    fn is_command(input: &str) -> bool {
        let cmd = input.to_lowercase();
        let first_word = cmd.split_whitespace().next().unwrap_or("");

        matches!(
            first_word,
            "apply" | "undo" | "retry" | "select" | "status" | "diff" | "help" | "exit" | "quit" |
            "git" | "cargo" | "rustc" | "ls" | "pwd" | "cd" | "mkdir" | "rm" | "mv" | "cp"
        ) || input.starts_with('/')
    }

    fn is_structured_spec(input: &str) -> bool {
        // DBM-*-SPEC v1.0 のような仕様書パターンを検出する。
        // DBM-*-PHASE-* / DBM-*-V* 形式のフェーズ識別子も対象とする。
        let first_line = input.lines().next().unwrap_or("").trim();
        let has_dbm_id = first_line.contains("DBM-")
            && (first_line.contains("-SPEC")
                || first_line.contains("-PHASE")
                || first_line.contains("-V2")
                || first_line.contains("-V1"));
        // 仕様書セクションキーワードが含まれる場合も StructuredSpec
        let has_spec_sections = input.contains("DBM-")
            && (input.contains("Deliverables") || input.contains("Goal") || input.contains("Constraints")
                || input.contains("Success Criteria") || input.contains("Assumptions"));
        has_dbm_id || (input.contains("DBM-") && input.contains("-SPEC") && input.contains("v1.0"))
            || has_spec_sections
    }

    fn is_json(input: &str) -> bool {
        (input.starts_with('{') && input.ends_with('}')) || 
        (input.starts_with('[') && input.ends_with(']'))
    }

    fn is_log(input: &str) -> bool {
        input.contains("[IR-TRACE]") || 
        input.contains("[CORE]") || 
        input.contains("[ERROR]") || 
        input.contains("error[E") || // Rust error codes
        input.contains("PANIC") ||
        input.lines().any(|l| l.contains("at src/") && l.contains(":"))
    }

    fn is_markdown(input: &str) -> bool {
        input.starts_with('#') || 
        input.contains("\n## ") || 
        input.contains("\n- ") ||
        input.contains("```")
    }

    fn is_natural_language(input: &str) -> bool {
        // 日本語文字 (ひらがな、カタカナ、漢字) が含まれているか
        let has_japanese = input.chars().any(|c| {
            ('\u{3040}'..='\u{309F}').contains(&c) || // ひらがな
            ('\u{30A0}'..='\u{30FF}').contains(&c) || // カタカナ
            ('\u{4E00}'..='\u{9FFF}').contains(&c)    // 漢字
        });

        if has_japanese {
            return true;
        }

        // 英語の文章っぽいか (スペースで区切られた単語が複数ある)
        let word_count = input.split_whitespace().count();
        word_count > 2 && !input.contains('{') && !input.contains('[')
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_command() {
        assert_eq!(DocumentClassifier::classify("apply"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("undo"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("select 1"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("/help"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("git add ."), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("git status"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("cargo test"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("cargo build"), InputKind::Command);
        assert_eq!(DocumentClassifier::classify("ls -la"), InputKind::Command);
    }

    #[test]
    fn test_classify_markdown() {
        assert_eq!(DocumentClassifier::classify("# Title\nContent"), InputKind::MarkdownDocument);
        assert_eq!(DocumentClassifier::classify("Text\n## Findings"), InputKind::MarkdownDocument);
        assert_eq!(DocumentClassifier::classify("- item 1\n- item 2"), InputKind::MarkdownDocument);
        assert_eq!(DocumentClassifier::classify("# Safety Analysis Result"), InputKind::MarkdownDocument);
    }

    #[test]
    fn test_classify_json() {
        assert_eq!(DocumentClassifier::classify("{\"key\": \"value\"}"), InputKind::JsonDocument);
        assert_eq!(DocumentClassifier::classify("[1, 2, 3]"), InputKind::JsonDocument);
    }

    #[test]
    fn test_classify_log() {
        assert_eq!(DocumentClassifier::classify("[IR-TRACE] some event"), InputKind::LogDocument);
        assert_eq!(DocumentClassifier::classify("error[E0425]: cannot find value"), InputKind::LogDocument);
        assert_eq!(DocumentClassifier::classify("error[E0425]"), InputKind::LogDocument);
    }

    #[test]
    fn test_classify_spec() {
        assert_eq!(
            DocumentClassifier::classify("DBM-DOCUMENT-CLASSIFIER-SPEC v1.0\nPurpose: ..."),
            InputKind::StructuredSpec
        );
    }

    #[test]
    fn test_classify_natural_language() {
        assert_eq!(
            DocumentClassifier::classify("このプロジェクトの全テストを棚卸ししてください"),
            InputKind::NaturalLanguage
        );
        assert_eq!(
            DocumentClassifier::classify("Please analyze the project structure"),
            InputKind::NaturalLanguage
        );
    }
}
