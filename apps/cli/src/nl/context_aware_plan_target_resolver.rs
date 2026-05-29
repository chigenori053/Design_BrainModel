use crate::nl::language_core_ir_adapter::{ExecutionMode, IrAction, IrTarget};
use crate::{core::ExecutionStatus, runtime::autonomous_control::RiskLevel};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

/// 前回の解析コンテキスト。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §5.1
#[derive(Debug, Clone, PartialEq)]
pub struct PreviousAnalysisContext {
    pub action: IrAction,
    pub target: IrTarget,
    pub mode: ExecutionMode,
    pub status: ExecutionStatus,
    pub summary_hash: u64,
    pub timestamp: u64,
}

impl PreviousAnalysisContext {
    pub fn new(
        action: IrAction,
        target: IrTarget,
        mode: ExecutionMode,
        status: ExecutionStatus,
    ) -> Self {
        Self {
            action,
            target,
            mode,
            status,
            summary_hash: 0, // TODO: Implement summary hash if needed
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviousPlanContext {
    pub target: IrTarget,
    pub mode: ExecutionMode,
    pub candidate_count: usize,
    pub candidates: Vec<ChangePlanCandidate>,
    pub plan_hash: u64,
    pub status: ExecutionStatus,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    Passed,
    ReviewRequired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrowTarget {
    File(String),
    Module(String),
    Symbol(String),
}

impl std::fmt::Display for NarrowTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(path) => write!(f, "File({path})"),
            Self::Module(name) => write!(f, "Module({name})"),
            Self::Symbol(name) => write!(f, "Symbol({name})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangePlanCandidate {
    pub candidate_id: usize,
    pub title: String,
    pub target: NarrowTarget,
    pub proposed_change: String,
    pub rationale: String,
    pub risk_level: RiskLevel,
    pub requires_validation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanValidationResult {
    pub plan_hash: u64,
    pub candidate_id: usize,
    pub target: NarrowTarget,
    pub status: ValidationStatus,
    pub risk_level: RiskLevel,
    pub apply_allowed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviousValidationContext {
    pub plan_hash: u64,
    pub candidate_id: usize,
    pub target: NarrowTarget,
    pub validation_status: ValidationStatus,
    pub risk_level: RiskLevel,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedCandidateContext {
    pub candidate_id: usize,
    pub target: NarrowTarget,
    pub plan_hash: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedPlanContext {
    pub plan_hash: u64,
    pub candidate_id: usize,
    pub target: NarrowTarget,
    pub approved: bool,
    pub apply_allowed: bool,
    pub timestamp: u64,
}

/// 実行時制約。
/// Spec DBM-LANGUAGECORE-CONSTRAINT-RECOGNITION-SPEC v1.0
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConstraint {
    pub no_apply: bool,
    pub no_delete: bool,
    pub no_modify: bool,
    pub no_git_operation: bool,
    pub no_external_command: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplSessionContext {
    pub previous_analysis_context: Option<PreviousAnalysisContext>,
    pub previous_plan_context: Option<PreviousPlanContext>,
    pub previous_validation_context: Option<PreviousValidationContext>,
    pub selected_candidate: Option<SelectedCandidateContext>,
    pub validated_plan: Option<ValidatedPlanContext>,
    /// セッション全体で有効な実行制約
    pub constraints: RuntimeConstraint,
}

impl ReplSessionContext {
    pub fn store_analysis(
        &mut self,
        action: IrAction,
        target: IrTarget,
        mode: ExecutionMode,
        status: ExecutionStatus,
    ) {
        if status == ExecutionStatus::Executed
            && mode == ExecutionMode::ReadOnly
            && matches!(
                action,
                IrAction::AnalyzeProject
                    | IrAction::AnalyzeFile
                    | IrAction::AnalyzeSymbol
                    | IrAction::AnalyzeTests
            )
        {
            self.previous_analysis_context = Some(PreviousAnalysisContext::new(
                action.clone(),
                target.clone(),
                mode.clone(),
                status,
            ));
            self.clear_after_new_analysis();
            eprintln!(
                "[IR-TRACE][CONTEXT_STORE] kind=analysis action={} target={} mode={} status={:?}",
                action, target, mode, status
            );
        }
    }

    pub fn store_plan(
        &mut self,
        target: IrTarget,
        mode: ExecutionMode,
        candidate_count: usize,
        candidates: Vec<ChangePlanCandidate>,
        plan_hash: u64,
        status: ExecutionStatus,
    ) {
        if status == ExecutionStatus::Executed && mode == ExecutionMode::PlanOnly {
            self.previous_plan_context = Some(PreviousPlanContext {
                target: target.clone(),
                mode: mode.clone(),
                candidate_count,
                candidates,
                plan_hash,
                status,
                timestamp: current_timestamp_secs(),
            });
            self.clear_after_new_plan();
            eprintln!(
                "[IR-TRACE][CONTEXT_STORE] kind=plan target={} mode={} status={:?}",
                target, mode, status
            );
        }
    }

    pub fn store_selection(&mut self, candidate_id: usize, target: NarrowTarget) {
        let plan_hash = self
            .previous_plan_context
            .as_ref()
            .map(|ctx| ctx.plan_hash)
            .unwrap_or(0);
        self.selected_candidate = Some(SelectedCandidateContext {
            candidate_id,
            target: target.clone(),
            plan_hash,
            timestamp: current_timestamp_secs(),
        });
        self.previous_validation_context = None;
        self.validated_plan = None;
        eprintln!(
            "[IR-TRACE][CONTEXT_STORE] kind=selection candidate_id={candidate_id} target={target}"
        );
    }

    pub fn store_validation(&mut self, result: PlanValidationResult) {
        self.previous_validation_context = Some(PreviousValidationContext {
            plan_hash: result.plan_hash,
            candidate_id: result.candidate_id,
            target: result.target.clone(),
            validation_status: result.status.clone(),
            risk_level: result.risk_level,
            timestamp: current_timestamp_secs(),
        });
        eprintln!(
            "[IR-TRACE][CONTEXT_STORE] kind=validation status={:?} risk={:?}",
            result.status, result.risk_level
        );

        if result.apply_allowed {
            self.validated_plan = Some(ValidatedPlanContext {
                plan_hash: result.plan_hash,
                candidate_id: result.candidate_id,
                target: result.target,
                approved: true,
                apply_allowed: true,
                timestamp: current_timestamp_secs(),
            });
            eprintln!("[IR-TRACE][CONTEXT_STORE] kind=validated_plan apply_allowed=true");
        } else {
            self.validated_plan = None;
        }
    }

    fn clear_after_new_analysis(&mut self) {
        if self.previous_plan_context.is_some()
            || self.previous_validation_context.is_some()
            || self.selected_candidate.is_some()
            || self.validated_plan.is_some()
        {
            eprintln!(
                "[IR-TRACE][CONTEXT_CLEAR] reason=NewAnalysis clears plan/validation/selection"
            );
        }
        self.previous_plan_context = None;
        self.previous_validation_context = None;
        self.selected_candidate = None;
        self.validated_plan = None;
    }

    fn clear_after_new_plan(&mut self) {
        if self.previous_validation_context.is_some()
            || self.selected_candidate.is_some()
            || self.validated_plan.is_some()
        {
            eprintln!(
                "[IR-TRACE][CONTEXT_CLEAR] reason=NewPlan clears validation/selection/validated_plan"
            );
        }
        self.previous_validation_context = None;
        self.selected_candidate = None;
        self.validated_plan = None;
    }

    pub fn trace_load(&self) {
        eprintln!(
            "[IR-TRACE][CONTEXT_LOAD] previous_analysis_context={} previous_plan_context={} previous_validation_context={} selected_candidate={} validated_plan={}",
            option_label(self.previous_analysis_context.as_ref()),
            option_label(self.previous_plan_context.as_ref()),
            option_label(self.previous_validation_context.as_ref()),
            option_label(self.selected_candidate.as_ref()),
            option_label(self.validated_plan.as_ref())
        );
    }
}

pub fn stable_context_hash(input: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn option_label<T>(value: Option<&T>) -> &'static str {
    if value.is_some() { "Some" } else { "None" }
}

/// Plan ターゲットの解決結果。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §5.2
#[derive(Debug, Clone)]
pub struct PlanTargetResolution {
    pub action: IrAction,
    pub target: IrTarget,
    pub mode: ExecutionMode,
    pub apply: bool,
    pub previous_context_used: bool,
    pub reason: PlanTargetResolutionReason,
}

/// 解決理由。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §5.3
#[derive(Debug, Clone, PartialEq)]
pub enum PlanTargetResolutionReason {
    ExplicitTarget,
    PreviousProjectAnalysisContext,
    PreviousFileAnalysisContext,
    PreviousSymbolAnalysisContext,
    MissingContext,
    UnsafeWorkspaceMutation,
    UnsupportedIntent,
}

/// 自然言語入力に PlanOnly 特有のキーワードが含まれているか判定する。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §6.1
pub fn is_plan_only_intent(lower: &str) -> bool {
    let jp = [
        "プラン",
        "計画",
        "候補",
        "提案",
        "修正候補",
        "修正案",
        "改善案",
        "変更候補",
        "安全な修正",
        "最小で安全",
        "作成して",
        "出して",
        "まだ適用しない",
        "適用しないで",
    ];
    let en = [
        "plan",
        "candidate",
        "proposal",
        "change plan",
        "fix plan",
        "safe fix",
        "smallest safe change",
        "improvement proposal",
        "do not apply",
        "without applying",
        "plan only",
    ];
    jp.iter().any(|kw| lower.contains(kw)) || en.iter().any(|kw| lower.contains(kw))
}

/// 自然言語入力に前回コンテキストへの参照が含まれているか判定する。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §6.2
pub fn has_context_reference(lower: &str) -> bool {
    let jp = [
        "構造解析結果をもとに",
        "解析結果をもとに",
        "解析結果を元に",
        "解析結果を基に",
        "前回の解析結果",
        "今の解析結果",
        "この解析結果",
        "この結果をもとに",
        "先ほどの結果",
        "分析結果から",
    ];
    let en = [
        "based on previous analysis",
        "based on this result",
        "from the analysis result",
        "using the previous result",
        "based on the structure analysis",
    ];
    jp.iter().any(|kw| lower.contains(kw)) || en.iter().any(|kw| lower.contains(kw))
}

/// 明示的なターゲット指定なしで PlanOnly 要求が来た場合、前回コンテキストから解決を試みる。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §7
pub fn resolve_plan_target(
    explicit_target: IrTarget,
    is_plan_only: bool,
    has_context_ref: bool,
    session_context: Option<&ReplSessionContext>,
) -> PlanTargetResolution {
    // 7.1 明示的なターゲットがある場合
    if explicit_target != IrTarget::None
        && !(explicit_target == IrTarget::WorkspaceRoot && is_plan_only && has_context_ref)
    {
        return PlanTargetResolution {
            action: IrAction::GenerateChangePlan,
            target: explicit_target,
            mode: ExecutionMode::PlanOnly,
            apply: false,
            previous_context_used: false,
            reason: PlanTargetResolutionReason::ExplicitTarget,
        };
    }

    // 7.2 - 7.4 前回コンテキストの利用
    if is_plan_only
        && has_context_ref
        && let Some(ctx) =
            session_context.and_then(|session| session.previous_analysis_context.as_ref())
        && ctx.status == ExecutionStatus::Executed
    {
        let reason = match &ctx.target {
            IrTarget::WorkspaceRoot => PlanTargetResolutionReason::PreviousProjectAnalysisContext,
            IrTarget::File(_) => PlanTargetResolutionReason::PreviousFileAnalysisContext,
            IrTarget::Symbol(_) => PlanTargetResolutionReason::PreviousSymbolAnalysisContext,
            IrTarget::None => PlanTargetResolutionReason::MissingContext,
        };

        if reason != PlanTargetResolutionReason::MissingContext {
            let res = PlanTargetResolution {
                action: IrAction::GenerateChangePlan,
                target: ctx.target.clone(),
                mode: ExecutionMode::PlanOnly,
                apply: false,
                previous_context_used: true,
                reason,
            };
            emit_resolution_trace(&res);
            return res;
        }
    }

    // 7.5 明示ターゲットなし + コンテキストなし
    PlanTargetResolution {
        action: if is_plan_only {
            IrAction::GenerateChangePlan
        } else {
            IrAction::Unknown
        },
        target: if is_plan_only {
            IrTarget::WorkspaceRoot
        } else {
            IrTarget::None
        },
        mode: if is_plan_only {
            ExecutionMode::PlanOnly
        } else {
            ExecutionMode::ReadOnly
        },
        apply: false,
        previous_context_used: false,
        reason: PlanTargetResolutionReason::MissingContext,
    }
}

fn emit_resolution_trace(res: &PlanTargetResolution) {
    eprintln!(
        "[IR-TRACE][CONTEXT_RESOLUTION] previous_context_used={} reason={:?} target={} mode={:?}",
        res.previous_context_used, res.reason, res.target, res.mode
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ExecutionStatus;

    fn session_with_analysis(previous: PreviousAnalysisContext) -> ReplSessionContext {
        ReplSessionContext {
            previous_analysis_context: Some(previous),
            ..ReplSessionContext::default()
        }
    }

    #[test]
    fn context_plan_uses_previous_project_analysis_when_target_missing() {
        let prev_ctx = PreviousAnalysisContext::new(
            IrAction::AnalyzeProject,
            IrTarget::WorkspaceRoot,
            ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );
        let session = session_with_analysis(prev_ctx);
        let res = resolve_plan_target(IrTarget::None, true, true, Some(&session));
        assert!(res.previous_context_used);
        assert_eq!(res.target, IrTarget::WorkspaceRoot);
        assert_eq!(
            res.reason,
            PlanTargetResolutionReason::PreviousProjectAnalysisContext
        );
        assert_eq!(res.mode, ExecutionMode::PlanOnly);
    }

    #[test]
    fn context_plan_uses_previous_file_analysis_when_target_missing() {
        let prev_ctx = PreviousAnalysisContext::new(
            IrAction::AnalyzeFile,
            IrTarget::File("src/core.rs".to_string()),
            ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );
        let session = session_with_analysis(prev_ctx);
        let res = resolve_plan_target(IrTarget::None, true, true, Some(&session));
        assert!(res.previous_context_used);
        assert_eq!(res.target, IrTarget::File("src/core.rs".to_string()));
        assert_eq!(
            res.reason,
            PlanTargetResolutionReason::PreviousFileAnalysisContext
        );
    }

    #[test]
    fn context_plan_without_previous_context_requires_clarification() {
        let res = resolve_plan_target(IrTarget::None, true, true, None);
        assert!(!res.previous_context_used);
        assert_eq!(res.target, IrTarget::WorkspaceRoot);
        assert_eq!(res.action, IrAction::GenerateChangePlan);
        assert_eq!(res.mode, ExecutionMode::PlanOnly);
        assert_eq!(res.reason, PlanTargetResolutionReason::MissingContext);
    }

    #[test]
    fn explicit_target_overrides_context() {
        let prev_ctx = PreviousAnalysisContext::new(
            IrAction::AnalyzeProject,
            IrTarget::WorkspaceRoot,
            ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );
        let explicit = IrTarget::File("src/main.rs".to_string());
        let session = session_with_analysis(prev_ctx);
        let res = resolve_plan_target(explicit.clone(), true, true, Some(&session));
        assert!(!res.previous_context_used);
        assert_eq!(res.target, explicit);
        assert_eq!(res.reason, PlanTargetResolutionReason::ExplicitTarget);
    }

    #[test]
    fn resolver_prefers_explicit_target_over_context() {
        explicit_target_overrides_context();
    }

    #[test]
    fn resolver_uses_session_context_before_history() {
        let prev_ctx = PreviousAnalysisContext::new(
            IrAction::AnalyzeProject,
            IrTarget::WorkspaceRoot,
            ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );
        let session = session_with_analysis(prev_ctx);

        let res = resolve_plan_target(IrTarget::None, true, true, Some(&session));

        assert!(res.previous_context_used);
        assert_eq!(
            res.reason,
            PlanTargetResolutionReason::PreviousProjectAnalysisContext
        );
    }

    #[test]
    fn workspace_root_plan_only_is_allowed() {
        let prev_ctx = PreviousAnalysisContext::new(
            IrAction::AnalyzeProject,
            IrTarget::WorkspaceRoot,
            ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );
        let session = session_with_analysis(prev_ctx);
        let res = resolve_plan_target(IrTarget::None, true, true, Some(&session));
        assert_eq!(res.mode, ExecutionMode::PlanOnly);
        assert!(!res.apply);
    }

    #[test]
    fn plan_only_never_sets_apply_true() {
        let prev_ctx = PreviousAnalysisContext::new(
            IrAction::AnalyzeProject,
            IrTarget::WorkspaceRoot,
            ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );
        let session = session_with_analysis(prev_ctx);
        let res = resolve_plan_target(IrTarget::None, true, true, Some(&session));
        assert!(!res.apply);
    }

    #[test]
    fn is_plan_only_intent_detection() {
        assert!(is_plan_only_intent("修正プランを作成して"));
        assert!(is_plan_only_intent("make a fix plan"));
        assert!(is_plan_only_intent("まだ適用しないで"));
        assert!(!is_plan_only_intent("解析して"));
    }

    #[test]
    fn long_input_generate_plan_never_returns_target_none() {
        let res = resolve_plan_target(IrTarget::None, true, false, None);
        assert_eq!(res.action, IrAction::GenerateChangePlan);
        assert_eq!(res.mode, ExecutionMode::PlanOnly);
        assert_eq!(res.target, IrTarget::WorkspaceRoot);
    }

    #[test]
    fn has_context_reference_detection() {
        assert!(has_context_reference("構造解析結果をもとに"));
        assert!(has_context_reference("based on previous analysis"));
        assert!(!has_context_reference("直接直して"));
    }
}
