use std::path::PathBuf;
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
    pub target_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirmation {
    pub action: RecommendedAction,
    pub summary: String,
    pub expires_at: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionContext {
    pub user_input: String,
    pub resolved_intent: ResolvedIntent,
    pub workspace_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub narrative: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Completed,
    Failed,
    WaitingConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    Idle,
    PendingConfirmation,
    Running,
    Completed,
    Failed,
}

pub struct IntentResolutionEngine;

impl IntentResolutionEngine {
    pub const CLARIFICATION_THRESHOLD: f32 = 0.75;

    pub fn resolve(input: &str) -> ResolvedIntent {
        let target_hint = extract_target_hint(input);
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
            return resolved_with_target(
                IntentGoal::RuntimeOperation,
                0.90,
                Vec::new(),
                target_hint,
            );
        }
        if has_any(
            &normalized,
            &["n", "no", "いいえ", "cancel", "キャンセル", "中止"],
        ) {
            return resolved_with_target(
                IntentGoal::RuntimeOperation,
                0.90,
                Vec::new(),
                target_hint,
            );
        }

        if has_security_signal(&normalized) {
            return resolved_with_target(
                IntentGoal::SecurityReview,
                0.88,
                vec![candidate(
                    RecommendedAction::RunSecurityAudit,
                    0.88,
                    "安全性確認を示す語が含まれています",
                )],
                target_hint,
            );
        }

        if has_apply_signal(&normalized) {
            return resolved_with_target(
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
                target_hint,
            );
        }

        if has_mutation_preview_signal(&normalized) {
            return resolved_with_target(
                IntentGoal::Mutation,
                0.88,
                vec![candidate(
                    RecommendedAction::RunMutationPreview,
                    0.88,
                    "変更プレビューを示す語が含まれています",
                )],
                target_hint,
            );
        }

        if has_analyze_signal(&normalized) && !has_refactor_signal(&normalized) {
            return resolved_with_target(
                IntentGoal::Analyze,
                0.96,
                vec![candidate(
                    RecommendedAction::RunAnalyze,
                    0.96,
                    "構造分析を示す語が含まれています",
                )],
                target_hint,
            );
        }

        if has_refactor_signal(&normalized) && has_target_signal(&normalized) {
            return resolved_with_target(
                IntentGoal::Mutation,
                0.76,
                vec![
                    candidate(
                        RecommendedAction::GenerateMutationPlan,
                        0.76,
                        "構造改善の計画生成に該当する可能性があります",
                    ),
                    candidate(
                        RecommendedAction::RunAnalyze,
                        0.70,
                        "対象整理の前に構造分析が有効です",
                    ),
                ],
                target_hint,
            );
        }

        if has_any(&normalized, &["validate", "validation", "検証", "確認して"]) {
            return resolved_with_target(
                IntentGoal::Validation,
                0.82,
                vec![candidate(
                    RecommendedAction::RunAnalyze,
                    0.60,
                    "検証には現状分析が必要です",
                )],
                target_hint,
            );
        }

        if has_any(
            &normalized,
            &["教えて", "知りたい", "what", "how", "なに", "何"],
        ) {
            return resolved_with_target(
                IntentGoal::InformationRequest,
                0.78,
                vec![candidate(
                    RecommendedAction::RequestClarification,
                    0.70,
                    "情報要求のため実行対象を確認します",
                )],
                target_hint,
            );
        }

        resolved_with_target(
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
            target_hint,
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
    pub fn execute(action: RecommendedAction, context: &ExecutionContext) -> ExecutionResult {
        if context.resolved_intent.confidence < IntentResolutionEngine::CLARIFICATION_THRESHOLD
            || action == RecommendedAction::RequestClarification
        {
            return ExecutionResult {
                status: ExecutionStatus::WaitingConfirmation,
                narrative: "追加情報が必要なため、実行を保留しています。".to_string(),
            };
        }

        let Some(_runtime_input) = Self::route_with_context(action, context) else {
            return ExecutionResult {
                status: ExecutionStatus::Failed,
                narrative: "実行可能なルートを決定できませんでした。".to_string(),
            };
        };

        ExecutionResult {
            status: ExecutionStatus::Completed,
            narrative: execution_narrative(action),
        }
    }

    pub fn route(action: RecommendedAction) -> Option<&'static str> {
        match action {
            RecommendedAction::RunAnalyze => Some("analyze ."),
            RecommendedAction::GenerateMutationPlan => None,
            RecommendedAction::RunMutationPreview => Some("mutation preview"),
            RecommendedAction::RunMutationApply => Some("mutation apply"),
            RecommendedAction::RunSecurityAudit => Some("security audit"),
            RecommendedAction::RequestClarification => None,
        }
    }

    pub fn route_with_context(
        action: RecommendedAction,
        context: &ExecutionContext,
    ) -> Option<String> {
        match action {
            RecommendedAction::GenerateMutationPlan => context
                .resolved_intent
                .target_hint
                .as_deref()
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .map(|target| format!("mutation plan {target}")),
            _ => Self::route(action).map(str::to_string),
        }
    }
}

fn execution_narrative(action: RecommendedAction) -> String {
    match action {
        RecommendedAction::RunAnalyze => {
            "構造分析を開始しました。\n\nAnalyze Engine を実行しています。".to_string()
        }
        RecommendedAction::GenerateMutationPlan => "変更計画を生成しています。".to_string(),
        RecommendedAction::RunMutationPreview => "変更影響を評価しています。".to_string(),
        RecommendedAction::RunMutationApply => {
            "変更を適用しています。\n\n構造安定性を評価しています。".to_string()
        }
        RecommendedAction::RunSecurityAudit => "セキュリティ監査を開始しました。".to_string(),
        RecommendedAction::RequestClarification => {
            "追加情報が必要なため、実行を保留しています。".to_string()
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
    resolved_with_target(primary_goal, confidence, candidate_actions, None)
}

fn resolved_with_target(
    primary_goal: IntentGoal,
    confidence: f32,
    candidate_actions: Vec<ActionCandidate>,
    target_hint: Option<String>,
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
        target_hint,
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

pub fn execution_state_label(state: ExecutionState) -> &'static str {
    match state {
        ExecutionState::Idle => "Idle",
        ExecutionState::PendingConfirmation => "PendingConfirmation",
        ExecutionState::Running => "Running",
        ExecutionState::Completed => "Completed",
        ExecutionState::Failed => "Failed",
    }
}

fn normalize(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace("::", "/")
        .replace('　', " ")
}

fn extract_target_hint(input: &str) -> Option<String> {
    input.split_whitespace().find_map(|token| {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(
                ch,
                ',' | '.' | ':' | ';' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        });
        if trimmed.contains("::")
            || trimmed.starts_with("apps/")
            || trimmed.starts_with("crates/")
            || trimmed.starts_with("apps::")
            || trimmed.starts_with("crates::")
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
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
        assert_eq!(intent.target_hint.as_deref(), Some("apps::cli::core"));
        assert!(intent.candidate_actions.len() >= 2);
        assert_eq!(
            intent.recommended_action(),
            RecommendedAction::GenerateMutationPlan
        );
        assert!(intent.requires_confirmation);
    }

    #[test]
    fn execution_router_routes_mutation_plan_with_target_hint() {
        let resolved_intent = IntentResolutionEngine::resolve("apps::cli::core を整理したい");
        let context = ExecutionContext {
            user_input: "apps::cli::core を整理したい".to_string(),
            resolved_intent,
            workspace_path: PathBuf::from("."),
        };

        assert_eq!(
            ExecutionRouter::route_with_context(RecommendedAction::GenerateMutationPlan, &context),
            Some("mutation plan apps::cli::core".to_string())
        );
    }

    #[test]
    fn execution_router_does_not_fallback_mutation_plan_without_target() {
        let resolved_intent = IntentResolutionEngine::resolve("整理したい");
        let context = ExecutionContext {
            user_input: "整理したい".to_string(),
            resolved_intent,
            workspace_path: PathBuf::from("."),
        };

        assert_eq!(
            ExecutionRouter::route_with_context(RecommendedAction::GenerateMutationPlan, &context),
            None
        );
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
            Some("analyze .")
        );
        assert_eq!(
            ExecutionRouter::route(RecommendedAction::RequestClarification),
            None
        );
    }

    #[test]
    fn execution_router_executes_analyze_route() {
        let resolved_intent = IntentResolutionEngine::resolve("プロジェクト構造分析してほしい");
        let context = ExecutionContext {
            user_input: "プロジェクト構造分析してほしい".to_string(),
            resolved_intent,
            workspace_path: PathBuf::from("."),
        };

        let result = ExecutionRouter::execute(RecommendedAction::RunAnalyze, &context);

        assert_eq!(result.status, ExecutionStatus::Completed);
        assert!(result.narrative.contains("Analyze Engine"));
    }

    #[test]
    fn execution_router_routes_security_audit() {
        let resolved_intent = IntentResolutionEngine::resolve("セキュリティ監査を実施して");
        let context = ExecutionContext {
            user_input: "セキュリティ監査を実施して".to_string(),
            resolved_intent,
            workspace_path: PathBuf::from("."),
        };

        let result = ExecutionRouter::execute(RecommendedAction::RunSecurityAudit, &context);

        assert_eq!(result.status, ExecutionStatus::Completed);
        assert!(result.narrative.contains("セキュリティ監査"));
    }
}
