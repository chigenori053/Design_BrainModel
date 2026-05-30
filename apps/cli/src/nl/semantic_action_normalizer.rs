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
    Constraint,
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
            Self::Constraint => "Constraint",
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
    /// 適用 (Apply)
    Apply,
    /// 削除 (Delete)
    Delete,
    /// 修正・変更 (Modify/Modify)
    Modify,
    /// Git 操作
    Git,
    /// 外部コマンド
    ExternalCommand,
    /// Role: Reviewer
    ReviewerRole,
    /// Role: Developer
    DeveloperRole,
    /// Role: OperatorRole
    OperatorRole,
    /// 構造的問題
    StructuralProblem,
    /// 未知
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

const CONSTRAINT_TERMS: &[(&str, f32)] = &[
    ("禁止", 0.95),
    ("停止", 0.80),
    ("行わない", 0.85),
    ("しないで", 0.90),
    ("禁止です", 0.95),
    ("実行しないで", 0.95),
    ("不要です", 0.70),
    ("不要", 0.70),
    ("として実行", 0.90),
    ("モード", 0.80),
    ("にしてください", 0.70),
];

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

const DEAD_TEST_TERMS: &[(&str, f32)] = &[
    ("危険なテスト", 0.95),
    ("不要なテスト", 0.95),
    ("死んだテスト", 0.95),
    ("dead test", 0.95),
    ("unreferenced", 0.85),
    ("unreachable", 0.85),
];

const REGRESSION_TERMS: &[(&str, f32)] = &[
    ("回帰テスト", 0.95),
    ("regression", 0.95),
    ("デグレ", 0.85),
    ("デグレード", 0.85),
];

const STRUCTURAL_PROBLEM_TERMS: &[(&str, f32)] = &[
    ("構造的問題", 0.95),
    ("設計問題", 0.95),
    ("依存関係の問題", 0.95),
    ("循環依存", 0.95),
    ("アーキテクチャ問題", 0.95),
    ("構造診断", 0.95),
    ("設計診断", 0.95),
    ("structural problem", 0.95),
    ("design problem", 0.95),
    ("dependency problem", 0.95),
    ("circular dependency", 0.95),
    ("architectural problem", 0.95),
    ("structural diagnosis", 0.95),
    ("design diagnosis", 0.95),
];

const APPLY_TERMS: &[&str] = &["apply", "適用", "反映"];
const DELETE_TERMS: &[&str] = &["削除", "delete", "rm", "消去"];
const MODIFY_TERMS: &[&str] = &["修正", "変更", "更新", "modify", "edit", "change"];
const GIT_TERMS: &[&str] = &["git", "commit", "push", "checkout"];
const EXTERNAL_TERMS: &[&str] = &["外部コマンド", "外部実行", "external command", "shell command"];
const REVIEWER_TERMS: &[&str] = &["reviewer", "査読者", "閲覧のみ"];
const DEVELOPER_TERMS: &[&str] = &["developer", "開発者"];
const OPERATOR_TERMS: &[&str] = &["operator", "運用者", "管理者", "admin"];

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
        (CONSTRAINT_TERMS, SemanticAction::Constraint),
        (DEAD_TEST_TERMS, SemanticAction::Analyze),
        (REGRESSION_TERMS, SemanticAction::Analyze),
        (STRUCTURAL_PROBLEM_TERMS, SemanticAction::Analyze),
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
    
    // 制約系ターゲットを優先
    for term in APPLY_TERMS {
        if lower.contains(*term) { return SemanticTarget::Apply; }
    }
    for term in DELETE_TERMS {
        if lower.contains(*term) { return SemanticTarget::Delete; }
    }
    for term in MODIFY_TERMS {
        if lower.contains(*term) { return SemanticTarget::Modify; }
    }
    for term in GIT_TERMS {
        if lower.contains(*term) { return SemanticTarget::Git; }
    }
    for term in EXTERNAL_TERMS {
        if lower.contains(*term) { return SemanticTarget::ExternalCommand; }
    }

    // Role
    for term in REVIEWER_TERMS {
        if lower.contains(*term) { return SemanticTarget::ReviewerRole; }
    }
    for term in DEVELOPER_TERMS {
        if lower.contains(*term) { return SemanticTarget::DeveloperRole; }
    }
    for term in OPERATOR_TERMS {
        if lower.contains(*term) { return SemanticTarget::OperatorRole; }
    }

    for (term, _) in STRUCTURAL_PROBLEM_TERMS {
        let hit = if term.is_ascii() {
            lower.contains(*term)
        } else {
            input.contains(*term)
        };
        if hit {
            return SemanticTarget::StructuralProblem;
        }
    }

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

    #[test]
    fn test_normalize_constraint() {
        let res = normalize_semantic_action("apply しないで").expect("result");
        assert_eq!(res.action, SemanticAction::Constraint);
        assert_eq!(res.target, SemanticTarget::Apply);

        let res = normalize_semantic_action("git は禁止です").expect("result");
        assert_eq!(res.action, SemanticAction::Constraint);
        assert_eq!(res.target, SemanticTarget::Git);

        let res = normalize_semantic_action("外部コマンド禁止").expect("result");
        assert_eq!(res.action, SemanticAction::Constraint);
        assert_eq!(res.target, SemanticTarget::ExternalCommand);
    }

    #[test]
    fn test_normalize_structural_problem() {
        let res = normalize_semantic_action("構造的問題を検出してください").expect("result");
        assert_eq!(res.action, SemanticAction::Analyze);
        assert_eq!(res.target, SemanticTarget::StructuralProblem);

        let res = normalize_semantic_action("設計診断を実行").expect("result");
        assert_eq!(res.action, SemanticAction::Analyze);
        assert_eq!(res.target, SemanticTarget::StructuralProblem);
    }
}
