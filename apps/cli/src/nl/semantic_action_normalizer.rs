//! SemanticActionNormalizer
//!
//! DBM-SEMANTIC-ACTION-NORMALIZATION-SPEC v1.0 に基づき、
//! 入力文から抽象アクションとターゲットを推論する。
//!
//! # 目的
//!
//! LanguageCore が理解できない入力（「棚卸し」「監査」など）を
//! [`SemanticAction`] に変換し、[`LanguageCoreIntent`] へのルーティングを可能にする。
//! 本モジュールは ReadOnly な分析系 Intent のみを生成し、Mutation/Apply/Git を生成しない。

/// 抽象アクション（初期セット）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAction {
    Analyze,
    Inventory,
    Audit,
    Classify,
    Search,
    Compare,
    Validate,
}

impl SemanticAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Analyze => "Analyze",
            Self::Inventory => "Inventory",
            Self::Audit => "Audit",
            Self::Classify => "Classify",
            Self::Search => "Search",
            Self::Compare => "Compare",
            Self::Validate => "Validate",
        }
    }
}

/// セマンティックターゲット。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTarget {
    /// テスト群（`#[test]` / `tests/` / integration / e2e）
    ProjectTests,
    /// プロジェクト全体
    Project,
    /// 不明
    Unknown,
}

/// セマンティック正規化の結果。
#[derive(Debug, Clone)]
pub struct SemanticNormalizationResult {
    pub action: SemanticAction,
    pub target: SemanticTarget,
    pub confidence: f32,
    /// マッチした語彙項目。
    pub matched_term: String,
}

// ── 語彙マッピング ────────────────────────────────────────────────────────────

/// (term, confidence)
const INVENTORY_TERMS: &[(&str, f32)] = &[
    ("棚卸し", 0.85),
    ("棚卸", 0.85),
    ("一覧化", 0.80),
    ("カタログ化", 0.80),
    ("整理", 0.70),
    ("inventory", 0.90),
    ("enumerate", 0.75),
];

const ANALYZE_TERMS: &[(&str, f32)] = &[
    ("調査", 0.80),
    ("解析", 0.85),
    ("分析", 0.85),
    ("analyze", 0.90),
    ("analyse", 0.90),
    ("investigation", 0.80),
];

const AUDIT_TERMS: &[(&str, f32)] = &[
    ("監査", 0.90),
    ("点検", 0.80),
    ("レビュー", 0.80),
    ("audit", 0.90),
    ("inspect", 0.80),
    ("review", 0.75),
];

const CLASSIFY_TERMS: &[(&str, f32)] = &[
    ("分類", 0.85),
    ("classify", 0.90),
    ("categorize", 0.85),
    ("カテゴリ", 0.75),
];

const SEARCH_TERMS: &[(&str, f32)] = &[
    ("検索", 0.85),
    ("search", 0.90),
    ("find", 0.80),
    ("探す", 0.80),
];

const COMPARE_TERMS: &[(&str, f32)] = &[("比較", 0.85), ("compare", 0.90), ("対比", 0.80)];

const VALIDATE_TERMS: &[(&str, f32)] = &[("検証", 0.85), ("validate", 0.90), ("verify", 0.85)];

// テスト関連キーワード（spec §ルート変換）
const TEST_TERMS: &[&str] = &[
    "テスト",
    "test",
    "tests",
    "unit test",
    "integration",
    "e2e",
    "#[test]",
    "mod tests",
];

// ── Public API ────────────────────────────────────────────────────────────────

/// 入力文からセマンティックアクションを推論する。
///
/// 識別できる語彙が含まれない場合は `None` を返す。
pub fn normalize_semantic_action(input: &str) -> Option<SemanticNormalizationResult> {
    let (action, confidence, matched_term) = detect_action(input)?;
    let target = detect_target(input);
    Some(SemanticNormalizationResult {
        action,
        target,
        confidence,
        matched_term: matched_term.to_string(),
    })
}

// ── Private helpers ───────────────────────────────────────────────────────────

type ActionMatchResult = (SemanticAction, f32, &'static str);

fn detect_action(input: &str) -> Option<ActionMatchResult> {
    let candidates: &[(&[(&str, f32)], SemanticAction)] = &[
        (INVENTORY_TERMS, SemanticAction::Inventory),
        (ANALYZE_TERMS, SemanticAction::Analyze),
        (AUDIT_TERMS, SemanticAction::Audit),
        (CLASSIFY_TERMS, SemanticAction::Classify),
        (SEARCH_TERMS, SemanticAction::Search),
        (COMPARE_TERMS, SemanticAction::Compare),
        (VALIDATE_TERMS, SemanticAction::Validate),
    ];

    // 最も高い confidence を持つ候補を選ぶ（同値なら先行優先）
    let mut best: Option<ActionMatchResult> = None;
    let lower = input.to_ascii_lowercase();

    for (terms, action) in candidates {
        for (term, confidence) in *terms {
            let matched = if term.is_ascii() {
                lower.contains(*term)
            } else {
                input.contains(*term)
            };
            if matched {
                match best {
                    Some((_, best_conf, _)) if *confidence <= best_conf => {}
                    _ => {
                        best = Some((*action, *confidence, term));
                    }
                }
            }
        }
    }
    best
}

fn detect_target(input: &str) -> SemanticTarget {
    let lower = input.to_ascii_lowercase();
    for term in TEST_TERMS {
        let hit = if term.is_ascii() {
            lower.contains(*term)
        } else {
            input.contains(*term)
        };
        if hit {
            return SemanticTarget::ProjectTests;
        }
    }
    if input.contains("プロジェクト") || lower.contains("project") {
        return SemanticTarget::Project;
    }
    SemanticTarget::Unknown
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// spec §テスト追加: semantic_normalize_inventory
    #[test]
    fn semantic_normalize_inventory() {
        let result = normalize_semantic_action("棚卸ししてください").expect("result");
        assert_eq!(result.action, SemanticAction::Inventory);
        assert_eq!(result.matched_term, "棚卸し");
        assert!(
            result.confidence >= 0.80,
            "confidence={} should be >= 0.80",
            result.confidence
        );
    }

    /// spec §テスト追加: semantic_normalize_audit
    #[test]
    fn semantic_normalize_audit() {
        let result = normalize_semantic_action("監査してください").expect("result");
        assert_eq!(result.action, SemanticAction::Audit);
        assert!(
            result.confidence >= 0.85,
            "confidence={}",
            result.confidence
        );
    }

    /// spec §テスト追加: semantic_normalize_analyze
    #[test]
    fn semantic_normalize_analyze() {
        let result = normalize_semantic_action("分析してください").expect("result");
        assert_eq!(result.action, SemanticAction::Analyze);
    }

    #[test]
    fn inventory_detects_project_tests_target() {
        let result = normalize_semantic_action("このプロジェクトの全テストを棚卸ししてください")
            .expect("result");
        assert_eq!(result.action, SemanticAction::Inventory);
        assert_eq!(result.target, SemanticTarget::ProjectTests);
        assert_eq!(result.matched_term, "棚卸し");
    }

    #[test]
    fn test_term_detection_case_insensitive() {
        let result = normalize_semantic_action("全 Test を棚卸し").expect("result");
        assert_eq!(result.target, SemanticTarget::ProjectTests);
    }

    #[test]
    fn english_audit_term_detected() {
        let result = normalize_semantic_action("audit all tests").expect("result");
        assert_eq!(result.action, SemanticAction::Audit);
        assert_eq!(result.target, SemanticTarget::ProjectTests);
    }

    #[test]
    fn unknown_input_returns_none() {
        assert!(normalize_semantic_action("hello world").is_none());
        assert!(normalize_semantic_action("xyz").is_none());
    }

    #[test]
    fn higher_confidence_term_wins() {
        // "監査" (0.90) > "レビュー" (0.80)
        let result = normalize_semantic_action("監査とレビュー").expect("result");
        assert_eq!(result.action, SemanticAction::Audit);
        assert_eq!(result.confidence, 0.90);
    }
}
