use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentGoal {
    Analyze,
    Mutation,
    Validation,
    SecurityReview,
    RuntimeOperation,
    InformationRequest,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendedAction {
    RunAnalyze,
    GenerateMutationPlan,
    RunMutationPreview,
    RunMutationApply,
    RunSecurityAudit,
    RequestClarification,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionCandidate {
    pub action: RecommendedAction,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedIntent {
    pub primary_goal: IntentGoal,
    pub confidence: f32,
    pub candidate_actions: Vec<ActionCandidate>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirmation {
    pub action: RecommendedAction,
    pub summary: String,
    pub expires_at: Option<SystemTime>,
}

pub struct IntentResolutionEngine;

impl IntentResolutionEngine {
    pub const CLARIFICATION_THRESHOLD: f32 = 0.75;

    pub fn resolve(input: &str) -> ResolvedIntent {
        let normalized = normalize(input);
        if normalized.is_empty() {
            return resolved(
                IntentGoal::Unknown,
                0.0,
                vec![candidate(
                    RecommendedAction::RequestClarification,
                    1.0,
                    "入力が空です",
                )],
            );
        }

        if has_any(&normalized, &["y", "yes", "はい", "実行", "apply", "承認"]) {
            return resolved(IntentGoal::RuntimeOperation, 0.90, Vec::new());
        }
        if has_any(
            &normalized,
            &["n", "no", "いいえ", "cancel", "キャンセル", "中止"],
        ) {
            return resolved(IntentGoal::RuntimeOperation, 0.90, Vec::new());
        }

        if has_security_signal(&normalized) {
            return resolved(
                IntentGoal::SecurityReview,
                0.88,
                vec![candidate(
                    RecommendedAction::RunSecurityAudit,
                    0.88,
                    "安全性確認を示す語が含まれています",
                )],
            );
        }

        if has_apply_signal(&normalized) {
            return resolved(
                IntentGoal::Mutation,
                0.92,
                vec![
                    candidate(
                        RecommendedAction::RunMutationApply,
                        0.92,
                        "変更適用を示す語が含まれています",
                    ),
                    candidate(
                        RecommendedAction::RunMutationPreview,
                        0.72,
                        "適用前確認が安全です",
                    ),
                ],
            );
        }

        if has_mutation_preview_signal(&normalized) {
            return resolved(
                IntentGoal::Mutation,
                0.88,
                vec![candidate(
                    RecommendedAction::RunMutationPreview,
                    0.88,
                    "変更プレビューを示す語が含まれています",
                )],
            );
        }

        if has_analyze_signal(&normalized) && !has_refactor_signal(&normalized) {
            return resolved(
                IntentGoal::Analyze,
                0.96,
                vec![candidate(
                    RecommendedAction::RunAnalyze,
                    0.96,
                    "構造分析を示す語が含まれています",
                )],
            );
        }

        if has_refactor_signal(&normalized) && has_target_signal(&normalized) {
            return resolved(
                IntentGoal::Mutation,
                0.70,
                vec![
                    candidate(
                        RecommendedAction::RunAnalyze,
                        0.70,
                        "対象整理の前に構造分析が有効です",
                    ),
                    candidate(
                        RecommendedAction::GenerateMutationPlan,
                        0.65,
                        "構造改善の計画生成に該当する可能性があります",
                    ),
                ],
            );
        }

        if has_any(&normalized, &["validate", "validation", "検証", "確認して"]) {
            return resolved(
                IntentGoal::Validation,
                0.82,
                vec![candidate(
                    RecommendedAction::RunAnalyze,
                    0.60,
                    "検証には現状分析が必要です",
                )],
            );
        }

        if has_any(
            &normalized,
            &["教えて", "知りたい", "what", "how", "なに", "何"],
        ) {
            return resolved(
                IntentGoal::InformationRequest,
                0.78,
                vec![candidate(
                    RecommendedAction::RequestClarification,
                    0.70,
                    "情報要求のため実行対象を確認します",
                )],
            );
        }

        resolved(
            IntentGoal::Unknown,
            0.35,
            vec![
                candidate(
                    RecommendedAction::RunAnalyze,
                    0.45,
                    "現状把握として構造分析が候補です",
                ),
                candidate(
                    RecommendedAction::GenerateMutationPlan,
                    0.40,
                    "改善計画生成が候補です",
                ),
            ],
        )
    }
}

pub struct ConfirmationEngine;

impl ConfirmationEngine {
    pub fn requires_confirmation(action: RecommendedAction) -> bool {
        match action {
            RecommendedAction::RunAnalyze => false,
            RecommendedAction::GenerateMutationPlan
            | RecommendedAction::RunMutationPreview
            | RecommendedAction::RunMutationApply
            | RecommendedAction::RunSecurityAudit => true,
            RecommendedAction::RequestClarification => false,
        }
    }

    pub fn prompt(intent: &ResolvedIntent) -> String {
        if intent.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD {
            return clarification_prompt(intent);
        }
        let action = intent
            .candidate_actions
            .first()
            .map(|candidate| candidate.action)
            .unwrap_or(RecommendedAction::RequestClarification);
        match action {
            RecommendedAction::RunAnalyze => {
                "要求を解釈しました。\n\n推定目的:\nシステム構造分析\n\n実行内容:\nAnalyze Engine\n\n分析を開始します。"
                    .to_string()
            }
            RecommendedAction::GenerateMutationPlan => {
                "要求を解釈しました。\n\n推定目的:\n構造改善\n\n推奨実行:\nMutation Plan生成\n\n実行しますか？\n\n[Y] 実行\n[N] キャンセル"
                    .to_string()
            }
            RecommendedAction::RunMutationApply => {
                "変更計画を評価しました。\n\n予測:\nWarning\n\n主な要因:\n・依存関係の集中\n\n影響:\n・apps::cli::runtime\n\n適用しますか？\n\n[Y] Apply\n[N] Cancel\n[P] Preview"
                    .to_string()
            }
            RecommendedAction::RunMutationPreview => {
                "要求を解釈しました。\n\n推定目的:\n変更内容の事前確認\n\n推奨実行:\nMutation Preview\n\n実行しますか？\n\n[Y] 実行\n[N] キャンセル"
                    .to_string()
            }
            RecommendedAction::RunSecurityAudit => {
                "要求を解釈しました。\n\n推定目的:\nSecurity Review\n\n推奨実行:\nSecurity Audit\n\n実行しますか？\n\n[Y] 実行\n[N] キャンセル"
                    .to_string()
            }
            RecommendedAction::RequestClarification => clarification_prompt(intent),
        }
    }

    pub fn pending(intent: &ResolvedIntent) -> Option<PendingConfirmation> {
        let action = intent.candidate_actions.first()?.action;
        Self::requires_confirmation(action).then(|| PendingConfirmation {
            action,
            summary: Self::prompt(intent),
            expires_at: None,
        })
    }
}

pub struct ExecutionRouter;

impl ExecutionRouter {
    pub fn route(action: RecommendedAction) -> Option<&'static str> {
        match action {
            RecommendedAction::RunAnalyze => Some("analyze"),
            RecommendedAction::GenerateMutationPlan => Some("mutation plan"),
            RecommendedAction::RunMutationPreview => Some("mutation preview"),
            RecommendedAction::RunMutationApply => Some("mutation apply"),
            RecommendedAction::RunSecurityAudit => Some("security audit"),
            RecommendedAction::RequestClarification => None,
        }
    }
}

impl ResolvedIntent {
    pub fn confirmation_status(&self) -> &'static str {
        if self.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD {
            "Clarification Required"
        } else if self.requires_confirmation {
            "Confirmation Required"
        } else {
            "Auto Executable"
        }
    }

    pub fn recommended_action(&self) -> RecommendedAction {
        self.candidate_actions
            .first()
            .map(|candidate| candidate.action)
            .unwrap_or(RecommendedAction::RequestClarification)
    }
}

fn resolved(
    primary_goal: IntentGoal,
    confidence: f32,
    candidate_actions: Vec<ActionCandidate>,
) -> ResolvedIntent {
    let requires_confirmation = candidate_actions
        .first()
        .map(|candidate| ConfirmationEngine::requires_confirmation(candidate.action))
        .unwrap_or(false);
    ResolvedIntent {
        primary_goal,
        confidence,
        candidate_actions,
        requires_confirmation,
    }
}

fn candidate(action: RecommendedAction, confidence: f32, reason: &str) -> ActionCandidate {
    ActionCandidate {
        action,
        confidence,
        reason: reason.to_string(),
    }
}

fn clarification_prompt(intent: &ResolvedIntent) -> String {
    let mut lines = vec![
        "要求を解釈しました。".to_string(),
        String::new(),
        "候補:".to_string(),
    ];
    for candidate in &intent.candidate_actions {
        lines.push(format!(
            "・{} ({:.0}%)",
            action_label(candidate.action),
            candidate.confidence * 100.0
        ));
    }
    lines.push(String::new());
    lines.push("どちらを希望しますか？".to_string());
    lines.join("\n")
}

pub fn goal_label(goal: IntentGoal) -> &'static str {
    match goal {
        IntentGoal::Analyze => "Analyze",
        IntentGoal::Mutation => "Mutation",
        IntentGoal::Validation => "Validation",
        IntentGoal::SecurityReview => "SecurityReview",
        IntentGoal::RuntimeOperation => "RuntimeOperation",
        IntentGoal::InformationRequest => "InformationRequest",
        IntentGoal::Unknown => "Unknown",
    }
}

pub fn action_label(action: RecommendedAction) -> &'static str {
    match action {
        RecommendedAction::RunAnalyze => "Analyze",
        RecommendedAction::GenerateMutationPlan => "Mutation Plan",
        RecommendedAction::RunMutationPreview => "Mutation Preview",
        RecommendedAction::RunMutationApply => "Mutation Apply",
        RecommendedAction::RunSecurityAudit => "Security Audit",
        RecommendedAction::RequestClarification => "Clarification",
    }
}

fn normalize(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace("::", "/")
        .replace('　', " ")
}

fn has_any(input: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| input.contains(needle))
}

fn has_analyze_signal(input: &str) -> bool {
    has_any(
        input,
        &[
            "analyze",
            "analysis",
            "解析",
            "分析",
            "構造分析",
            "構造解析",
            "プロジェクト構造",
        ],
    )
}

fn has_refactor_signal(input: &str) -> bool {
    has_any(
        input,
        &[
            "整理",
            "改善",
            "良くしたい",
            "もっと良く",
            "refactor",
            "restructure",
            "clean up",
        ],
    )
}

fn has_target_signal(input: &str) -> bool {
    input.contains('/')
        || input.contains("src")
        || input.contains("apps")
        || input.contains("crates")
        || input.contains("core")
        || input.contains("runtime")
        || input.contains("module")
        || input.contains("モジュール")
}

fn has_apply_signal(input: &str) -> bool {
    has_any(
        input,
        &[
            "適用",
            "apply",
            "変更を適用",
            "この変更を適用",
            "反映",
            "実行して",
        ],
    )
}

fn has_mutation_preview_signal(input: &str) -> bool {
    has_any(input, &["preview", "プレビュー", "事前確認", "下見"])
}

fn has_security_signal(input: &str) -> bool {
    has_any(
        input,
        &["security", "安全", "脆弱", "audit", "監査", "セキュリティ"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_project_structure_analyze_with_high_confidence() {
        let intent = IntentResolutionEngine::resolve("プロジェクト構造分析してほしい");
        assert_eq!(intent.primary_goal, IntentGoal::Analyze);
        assert!(intent.confidence >= 0.95);
        assert_eq!(intent.recommended_action(), RecommendedAction::RunAnalyze);
        assert!(!intent.requires_confirmation);
    }

    #[test]
    fn refactor_like_input_returns_multiple_candidates() {
        let intent = IntentResolutionEngine::resolve("apps::cli::core を整理したい");
        assert_eq!(intent.primary_goal, IntentGoal::Mutation);
        assert!(intent.candidate_actions.len() >= 2);
        assert!(intent.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD);
    }

    #[test]
    fn apply_requires_confirmation() {
        let intent = IntentResolutionEngine::resolve("この変更を適用して");
        assert_eq!(
            intent.recommended_action(),
            RecommendedAction::RunMutationApply
        );
        assert!(intent.requires_confirmation);
    }

    #[test]
    fn ambiguous_improvement_requires_clarification() {
        let intent = IntentResolutionEngine::resolve("もっと良くしたい");
        assert_eq!(intent.primary_goal, IntentGoal::Unknown);
        assert_eq!(intent.confidence, 0.35);
        assert!(intent.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD);
    }

    #[test]
    fn execution_router_maps_confirmed_actions() {
        assert_eq!(
            ExecutionRouter::route(RecommendedAction::RunAnalyze),
            Some("analyze")
        );
        assert_eq!(
            ExecutionRouter::route(RecommendedAction::RequestClarification),
            None
        );
    }
}
