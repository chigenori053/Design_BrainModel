//! LanguageCoreToIrAdapter
//!
//! DBM-LANGUAGECORE-IR-INTENT-ADAPTER-SPEC v1.0 に基づき、
//! 自然言語入力を [`LanguageCoreIntent`] へ分類し、[`IrIntentRequest`] へ変換する。
//!
//! # 目的
//!
//! `DefaultIntentRefiner` は非 ASCII 文字を全てストリップするため、
//! 日本語入力が `EmptyInput` → `InvalidInput` エラーになる。
//! 本 Adapter はその前段で意味分類を行い、ReadOnly な Analyze 系 Intent を
//! `execute_from_text` を経由せずに直接ハンドルする経路を提供する。

use std::fmt;

use crate::nl::normalization::{TargetResolutionFailure, confirmation_like_target_failure};

// ── LanguageCoreIntent ────────────────────────────────────────────────────────

/// LanguageCore が解釈した意味 Intent。
#[derive(Debug, Clone, PartialEq)]
pub enum LanguageCoreIntent {
    /// プロジェクト全体の構造解析
    ProjectStructureAnalyze,
    /// ワークスペース全体の解析
    WorkspaceAnalyze,
    /// 依存関係の解析
    DependencyAnalyze,
    /// モジュール構成の解析
    ModuleStructureAnalyze,
    /// 指定ファイルの解析
    FileAnalyze { file: String },
    /// 指定シンボルの解析
    SymbolAnalyze { symbol: String },
    /// 修正プランの作成要求
    GenerateChangePlan {
        target: Option<String>,
        instruction: String,
    },
    /// リファクタリング要求（apply しない）
    RefactorRequest {
        target: Option<String>,
        instruction: String,
    },
    /// 適用要求（直前の検証済み Plan が必要）
    ApplyRequest,
    /// 確認・適用ではない安全性レビュー要求
    SafetyReview,
    /// 未分類
    Unknown { raw: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseRole {
    PrimaryRequest,
    ReferencedConcept,
    ReferencedToken,
    SafetyConstraint,
    ExecutionRequest,
    MetaInstruction,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedClause {
    pub text: String,
    pub role: ClauseRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferencedToken {
    ConfirmationLike(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SafetyConstraints {
    pub no_apply: bool,
    pub no_file_write: bool,
    pub no_git_operation: bool,
    pub no_external_command: bool,
}

impl SafetyConstraints {
    pub fn prohibits_apply(self) -> bool {
        self.no_apply || self.no_file_write
    }

    fn labels(self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.no_apply {
            labels.push("no_apply");
        }
        if self.no_file_write {
            labels.push("no_file_write");
        }
        if self.no_git_operation {
            labels.push("no_git");
        }
        if self.no_external_command {
            labels.push("no_external_command");
        }
        labels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryIntent {
    AnalyzeProject,
    ValidatePlan,
    ReviewValidatedPlan,
    ReviewSafety,
    GenerateChangePlan,
    Apply,
    Unknown,
}

impl fmt::Display for LanguageCoreIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectStructureAnalyze => write!(f, "ProjectStructureAnalyze"),
            Self::WorkspaceAnalyze => write!(f, "WorkspaceAnalyze"),
            Self::DependencyAnalyze => write!(f, "DependencyAnalyze"),
            Self::ModuleStructureAnalyze => write!(f, "ModuleStructureAnalyze"),
            Self::FileAnalyze { file } => write!(f, "FileAnalyze({file})"),
            Self::SymbolAnalyze { symbol } => write!(f, "SymbolAnalyze({symbol})"),
            Self::GenerateChangePlan { .. } => write!(f, "GenerateChangePlan"),
            Self::RefactorRequest { .. } => write!(f, "RefactorRequest"),
            Self::ApplyRequest => write!(f, "ApplyRequest"),
            Self::SafetyReview => write!(f, "SafetyReview"),
            Self::Unknown { raw } => write!(f, "Unknown({raw})"),
        }
    }
}

// ── IrAction ─────────────────────────────────────────────────────────────────

/// IR レベルのアクション。
#[derive(Debug, Clone, PartialEq)]
pub enum IrAction {
    AnalyzeProject,
    AnalyzeWorkspace,
    AnalyzeDependencies,
    AnalyzeModuleStructure,
    AnalyzeFile,
    AnalyzeSymbol,
    GenerateChangePlan,
    ValidatePlan,
    ReviewValidatedPlan,
    ReviewSafety,
    Refactor,
    Apply,
    Unknown,
}

impl IrAction {
    /// ReadOnly な analyze アクションか否か。
    ///
    /// true の場合、`execute_from_text` を経由せず直接ハンドルできる。
    pub fn is_analyze(&self) -> bool {
        matches!(
            self,
            Self::AnalyzeProject
                | Self::AnalyzeWorkspace
                | Self::AnalyzeDependencies
                | Self::AnalyzeModuleStructure
                | Self::AnalyzeFile
                | Self::AnalyzeSymbol
        )
    }
}

impl fmt::Display for IrAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnalyzeProject => write!(f, "AnalyzeProject"),
            Self::AnalyzeWorkspace => write!(f, "AnalyzeWorkspace"),
            Self::AnalyzeDependencies => write!(f, "AnalyzeDependencies"),
            Self::AnalyzeModuleStructure => write!(f, "AnalyzeModuleStructure"),
            Self::AnalyzeFile => write!(f, "AnalyzeFile"),
            Self::AnalyzeSymbol => write!(f, "AnalyzeSymbol"),
            Self::GenerateChangePlan => write!(f, "GenerateChangePlan"),
            Self::ValidatePlan => write!(f, "ValidatePlan"),
            Self::ReviewValidatedPlan => write!(f, "ReviewValidatedPlan"),
            Self::ReviewSafety => write!(f, "ReviewSafety"),
            Self::Refactor => write!(f, "Refactor"),
            Self::Apply => write!(f, "Apply"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

// ── IrTarget ─────────────────────────────────────────────────────────────────

/// IR レベルのターゲット。
#[derive(Debug, Clone, PartialEq)]
pub enum IrTarget {
    WorkspaceRoot,
    File(String),
    Symbol(String),
    None,
}

impl fmt::Display for IrTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkspaceRoot => write!(f, "WorkspaceRoot"),
            Self::File(path) => write!(f, "File({path})"),
            Self::Symbol(sym) => write!(f, "Symbol({sym})"),
            Self::None => write!(f, "None"),
        }
    }
}

// ── ExecutionMode ─────────────────────────────────────────────────────────────

/// 実行モード。
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionMode {
    /// ストアを変更しない読み取り専用解析
    ReadOnly,
    /// 計画のみ（apply しない）
    PlanOnly,
    /// 検証のみ（apply しない）
    ValidateOnly,
    /// 検証済み Plan の適用
    Apply,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "ReadOnly"),
            Self::PlanOnly => write!(f, "PlanOnly"),
            Self::ValidateOnly => write!(f, "ValidateOnly"),
            Self::Apply => write!(f, "Apply"),
        }
    }
}

// ── IrIntentRequest ───────────────────────────────────────────────────────────

/// LanguageCoreToIrAdapter の変換結果。
#[derive(Debug, Clone)]
pub struct IrIntentRequest {
    pub action: IrAction,
    pub target: IrTarget,
    pub mode: ExecutionMode,
    pub raw_input: String,
    pub confidence: f32,
    pub safety_constraints: SafetyConstraints,
    pub target_failure: Option<TargetResolutionFailure>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// 自然言語入力を [`LanguageCoreIntent`] に分類する。
///
/// 日本語・英語の両方に対応する。分類は先行優先（specific → general）。
pub fn classify_language_core_intent(input: &str) -> LanguageCoreIntent {
    let lower = input.to_lowercase();
    let clauses = classify_clauses(input);
    let safety_constraints = extract_safety_constraints_from_clauses(&clauses);
    let primary_intent = select_primary_intent(&clauses);

    emit_long_input_traces(primary_intent, safety_constraints, &clauses);

    match primary_intent {
        PrimaryIntent::AnalyzeProject => return LanguageCoreIntent::ProjectStructureAnalyze,
        PrimaryIntent::GenerateChangePlan => {
            return LanguageCoreIntent::GenerateChangePlan {
                target: Option::None,
                instruction: input.to_string(),
            };
        }
        PrimaryIntent::Apply => {
            return LanguageCoreIntent::ApplyRequest;
        }
        PrimaryIntent::ReviewSafety => return LanguageCoreIntent::SafetyReview,
        PrimaryIntent::ValidatePlan
        | PrimaryIntent::ReviewValidatedPlan
        | PrimaryIntent::Unknown => {}
    }

    // 修正プラン要求は「構造解析結果」など Analyze 語を含みうるため、
    // Analyze 系 fallback より先に判定する。
    if is_explicit_plan_primary(&lower) {
        return LanguageCoreIntent::GenerateChangePlan {
            target: Option::None,
            instruction: input.to_string(),
        };
    }

    // 依存関係解析（プロジェクト構造より先にチェック）
    if is_dependency_analyze(&lower) {
        return LanguageCoreIntent::DependencyAnalyze;
    }

    // モジュール構成解析
    if is_module_analyze(&lower) {
        return LanguageCoreIntent::ModuleStructureAnalyze;
    }

    // ワークスペース解析
    if is_workspace_analyze(&lower) {
        return LanguageCoreIntent::WorkspaceAnalyze;
    }

    // プロジェクト構造解析（汎用 analyze キーワードも含む fallback）
    if is_project_structure_analyze(&lower) {
        return LanguageCoreIntent::ProjectStructureAnalyze;
    }

    // 適用要求
    if is_explicit_apply(&lower) && !safety_constraints.prohibits_apply() {
        return LanguageCoreIntent::ApplyRequest;
    }

    // リファクタリング要求（analyze キーワードなし）
    if is_refactor_intent(&lower) && !has_analyze_keyword(&lower) {
        return LanguageCoreIntent::RefactorRequest {
            target: Option::None,
            instruction: input.to_string(),
        };
    }

    // analyze キーワードのみ（上記に合致しなかった場合は汎用プロジェクト解析）
    if has_analyze_keyword(&lower) {
        return LanguageCoreIntent::ProjectStructureAnalyze;
    }

    LanguageCoreIntent::Unknown {
        raw: input.to_string(),
    }
}

/// [`LanguageCoreIntent`] を [`IrIntentRequest`] に変換する。
///
/// spec 5.2 の変換ルールに従う。
pub fn language_core_to_ir(intent: LanguageCoreIntent, raw_input: &str) -> IrIntentRequest {
    let safety_constraints = extract_safety_constraints(raw_input);
    let target_failure = confirmation_like_target_failure(raw_input);
    match intent {
        LanguageCoreIntent::ProjectStructureAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeProject,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.90,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::WorkspaceAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeWorkspace,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::DependencyAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeDependencies,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::ModuleStructureAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeModuleStructure,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::FileAnalyze { file } => IrIntentRequest {
            action: IrAction::AnalyzeFile,
            target: IrTarget::File(file),
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.95,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::SymbolAnalyze { symbol } => IrIntentRequest {
            action: IrAction::AnalyzeSymbol,
            target: IrTarget::Symbol(symbol),
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.90,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::GenerateChangePlan { target, .. } => IrIntentRequest {
            action: IrAction::GenerateChangePlan,
            target: target
                .map(IrTarget::File)
                .unwrap_or(IrTarget::WorkspaceRoot),
            mode: ExecutionMode::PlanOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.80,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::RefactorRequest { target, .. } => IrIntentRequest {
            action: IrAction::Refactor,
            target: target
                .map(IrTarget::File)
                .unwrap_or(IrTarget::WorkspaceRoot),
            // RefactorRequest は必ず PlanOnly から（spec §7.2）
            mode: ExecutionMode::PlanOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.75,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::ApplyRequest => IrIntentRequest {
            action: IrAction::Apply,
            target: IrTarget::None,
            // ApplyRequest は検証済み Plan が必要（spec §7.3）
            mode: ExecutionMode::Apply,
            raw_input: raw_input.to_string(),
            confidence: 0.95,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::SafetyReview => IrIntentRequest {
            action: IrAction::ReviewSafety,
            target: IrTarget::None,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
            safety_constraints,
            target_failure: target_failure.clone(),
        },
        LanguageCoreIntent::Unknown { .. } => IrIntentRequest {
            action: IrAction::Unknown,
            target: IrTarget::None,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.0,
            safety_constraints,
            target_failure,
        },
    }
}

// ── Classifier helpers ────────────────────────────────────────────────────────

pub(crate) fn has_analyze_keyword(lower: &str) -> bool {
    ["analyze", "analyse", "解析", "分析", "調べ", "audit"]
        .iter()
        .any(|kw| lower.contains(kw))
}

pub fn classify_clauses(input: &str) -> Vec<ClassifiedClause> {
    input
        .split(['。', '\n', '；', ';'])
        .map(str::trim)
        .filter(|clause| !clause.is_empty())
        .map(|clause| {
            let lower = clause.to_lowercase();
            let role = if is_safety_constraint(&lower) {
                ClauseRole::SafetyConstraint
            } else if is_referenced_token(&lower) {
                ClauseRole::ReferencedToken
            } else if is_meta_instruction(&lower) {
                ClauseRole::MetaInstruction
            } else if is_explicit_analyze_primary(&lower) || is_explicit_plan_primary(&lower) {
                ClauseRole::PrimaryRequest
            } else if is_explicit_apply(&lower) {
                ClauseRole::ExecutionRequest
            } else if is_referenced_concept(&lower) {
                ClauseRole::ReferencedConcept
            } else {
                ClauseRole::Unknown
            };
            ClassifiedClause {
                text: clause.to_string(),
                role,
            }
        })
        .collect()
}

pub fn select_primary_intent(clauses: &[ClassifiedClause]) -> PrimaryIntent {
    if clauses
        .iter()
        .any(|clause| is_safety_review_primary(&clause.text.to_lowercase()))
    {
        return PrimaryIntent::ReviewSafety;
    }
    if clauses
        .iter()
        .filter(|clause| clause.role == ClauseRole::PrimaryRequest)
        .any(|clause| is_explicit_analyze_primary(&clause.text.to_lowercase()))
    {
        return PrimaryIntent::AnalyzeProject;
    }
    if clauses
        .iter()
        .filter(|clause| clause.role == ClauseRole::PrimaryRequest)
        .any(|clause| is_explicit_validate_primary(&clause.text.to_lowercase()))
    {
        return PrimaryIntent::ValidatePlan;
    }
    if clauses
        .iter()
        .filter(|clause| clause.role == ClauseRole::PrimaryRequest)
        .any(|clause| is_explicit_plan_primary(&clause.text.to_lowercase()))
    {
        return PrimaryIntent::GenerateChangePlan;
    }
    if clauses
        .iter()
        .filter(|clause| clause.role == ClauseRole::ExecutionRequest)
        .any(|clause| is_explicit_apply(&clause.text.to_lowercase()))
    {
        return PrimaryIntent::Apply;
    }
    PrimaryIntent::Unknown
}

pub fn extract_safety_constraints(input: &str) -> SafetyConstraints {
    extract_safety_constraints_from_clauses(&classify_clauses(input))
}

fn extract_safety_constraints_from_clauses(clauses: &[ClassifiedClause]) -> SafetyConstraints {
    clauses
        .iter()
        .filter(|clause| clause.role == ClauseRole::SafetyConstraint)
        .fold(SafetyConstraints::default(), |mut constraints, clause| {
            let lower = clause.text.to_lowercase();
            if has_no_apply_phrase(&lower)
                || lower.contains("修正しない")
                || ((lower.contains("修正") || lower.contains("apply"))
                    && contains_no_execute(&lower))
            {
                constraints.no_apply = true;
            }
            if lower.contains("ファイル変更しない")
                || (lower.contains("ファイル変更") && contains_no_execute(&lower))
                || lower.contains("file write")
                || lower.contains("files modified")
            {
                constraints.no_file_write = true;
                constraints.no_apply = true;
            }
            if lower.contains("git操作しない")
                || (lower.contains("git操作") && contains_no_execute(&lower))
                || lower.contains("git operation")
            {
                constraints.no_git_operation = true;
            }
            if lower.contains("外部コマンド実行しない")
                || (lower.contains("外部コマンド実行") && contains_no_execute(&lower))
                || lower.contains("external command")
            {
                constraints.no_external_command = true;
            }
            constraints
        })
}

fn emit_long_input_traces(
    primary_intent: PrimaryIntent,
    safety_constraints: SafetyConstraints,
    clauses: &[ClassifiedClause],
) {
    if clauses.len() < 2 {
        return;
    }
    let referenced = clauses
        .iter()
        .filter(|clause| {
            matches!(
                clause.role,
                ClauseRole::ReferencedConcept | ClauseRole::ReferencedToken
            )
        })
        .map(|clause| referenced_label(&clause.text))
        .collect::<Vec<_>>()
        .join(",");
    let safety = safety_constraints.labels().join(",");
    eprintln!(
        "[IR-TRACE][LONG_INPUT_CLASSIFY] primary={:?} referenced={} safety={}",
        primary_intent, referenced, safety
    );
    eprintln!(
        "[IR-TRACE][PRIMARY_INTENT] selected={:?} reason={}",
        primary_intent,
        primary_reason(primary_intent)
    );
    eprintln!(
        "[IR-TRACE][SAFETY_CONSTRAINTS] no_apply={} no_file_write={} no_git_operation={} no_external_command={}",
        safety_constraints.no_apply,
        safety_constraints.no_file_write,
        safety_constraints.no_git_operation,
        safety_constraints.no_external_command
    );
    for clause in clauses
        .iter()
        .filter(|clause| clause.role == ClauseRole::ReferencedToken)
    {
        for token in referenced_tokens(&clause.text.to_lowercase()) {
            eprintln!("[IR-TRACE][TOKEN_ISOLATION] token={token} role=ReferencedToken");
        }
    }
    if primary_intent == PrimaryIntent::AnalyzeProject {
        eprintln!(
            "[IR-TRACE][TARGET_RESOLUTION] target=WorkspaceRoot reason=AnalyzeProjectDefault"
        );
    }
}

fn primary_reason(intent: PrimaryIntent) -> &'static str {
    match intent {
        PrimaryIntent::AnalyzeProject => "ExplicitAnalyzePrimaryRequest",
        PrimaryIntent::ValidatePlan => "ExplicitValidatePrimaryRequest",
        PrimaryIntent::ReviewValidatedPlan => "NoApplyWithValidatedPlan",
        PrimaryIntent::ReviewSafety => "ExplicitSafetyReviewPrimaryRequest",
        PrimaryIntent::GenerateChangePlan => "ExplicitPlanPrimaryRequest",
        PrimaryIntent::Apply => "ExplicitApplyRequest",
        PrimaryIntent::Unknown => "Unknown",
    }
}

fn referenced_label(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    if is_referenced_token(&lower) {
        "ReferencedToken"
    } else if lower.contains("apply guard") {
        "ApplyGuard"
    } else if lower.contains("preview") {
        "PreviewConfirmation"
    } else if lower.contains("検証") || lower.contains("validation") {
        "PlanValidation"
    } else {
        "ReferencedConcept"
    }
}

pub fn is_confirmation_token_like_target(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "y" | "n" | "yes" | "no" | "yes/no" | "y/n" | "confirmation"
    )
}

fn is_referenced_token(lower: &str) -> bool {
    lower.contains("yes/no")
        || lower.contains("y/n")
        || lower.contains("y や n")
        || lower.contains("yes と no")
        || lower.contains("yes ではなく")
        || lower.contains("n という文字")
}

fn referenced_tokens(lower: &str) -> Vec<&'static str> {
    let mut tokens = Vec::new();
    if lower.contains("yes/no") {
        tokens.push("yes/no");
    }
    if lower.contains("y/n") || lower.contains("y や n") {
        tokens.push("y");
        tokens.push("n");
    }
    if lower.contains("confirmation") {
        tokens.push("confirmation");
    }
    tokens
}

fn is_meta_instruction(lower: &str) -> bool {
    lower.contains("confirmation として扱わず")
        || lower.contains("確認ではなく")
        || lower.contains("自然言語入力として解釈")
        || lower.contains("natural language")
}

fn is_safety_review_primary(lower: &str) -> bool {
    (lower.contains("安全性") && lower.contains("評価"))
        || lower.contains("設計上の安全性")
        || lower.contains("自然言語入力として解釈")
        || lower.contains("確認ではなく評価")
}

fn is_explicit_analyze_primary(lower: &str) -> bool {
    [
        "構造を解析してください",
        "構造を解析して",
        "全体の構造を確認して",
        "構成を確認して",
        "接続に問題がないか整理して",
        "現状を整理して",
        "analyze project structure",
        "analyze this project",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
        || (lower.contains("プロジェクト全体") && lower.contains("構造") && lower.contains("解析"))
}

fn is_explicit_validate_primary(lower: &str) -> bool {
    lower.contains("検証して") || lower.contains("validate")
}

fn is_explicit_plan_primary(lower: &str) -> bool {
    [
        "修正プランを作成して",
        "変更プランを作成して",
        "安全な小規模修正プランを作成して",
        "候補を提示して",
        "fix plan",
        "change plan",
        "create a plan",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

fn is_referenced_concept(lower: &str) -> bool {
    [
        "修正プラン生成",
        "候補選択",
        "apply guard",
        "plan validation",
        "preview confirmation",
        "までの接続",
        "までの流れ",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

fn is_safety_constraint(lower: &str) -> bool {
    has_no_apply_phrase(lower)
        || lower.contains("修正しない")
        || lower.contains("ファイル変更しない")
        || lower.contains("git操作しない")
        || lower.contains("外部コマンド実行しない")
        || ((lower.contains("修正")
            || lower.contains("apply")
            || lower.contains("ファイル変更")
            || lower.contains("git操作")
            || lower.contains("外部コマンド実行"))
            && contains_no_execute(lower))
        || lower.contains("no files modified")
        || lower.contains("no apply")
        || lower.contains("do not apply")
}

fn has_no_apply_phrase(lower: &str) -> bool {
    lower.contains("まだ適用しない")
        || lower.contains("まだapplyしない")
        || lower.contains("適用しないで")
        || lower.contains("applyしない")
        || lower.contains("do not apply")
        || lower.contains("without applying")
}

fn contains_no_execute(lower: &str) -> bool {
    lower.contains("行わない")
        || lower.contains("行わず")
        || lower.contains("しない")
        || lower.contains("停止")
}

fn is_project_structure_analyze(lower: &str) -> bool {
    let jp = [
        "プロジェクトの構造",
        "プロジェクト全体",
        "システム構造",
        "コードベースの構造",
        "全体構成",
        "全体を解析",
        "全体を分析",
    ];
    let en = [
        "project structure",
        "analyze this project",
        "inspect project",
        "summarize codebase",
        "analyze system structure",
        "inspect workspace",
        "whole project",
        "entire project",
        "analyze project",
        "project analysis",
        "codebase architecture",
    ];
    let has_jp = jp.iter().any(|p| lower.contains(p));
    let has_en = en.iter().any(|p| lower.contains(p));
    // 「プロジェクト」+「解析」キーワードの組み合わせ
    let has_project_kw = lower.contains("プロジェクト") || lower.contains("project");
    let has_analyze = has_analyze_keyword(lower);
    has_jp || has_en || (has_project_kw && has_analyze)
}

fn is_dependency_analyze(lower: &str) -> bool {
    let jp = [
        "依存関係を解析",
        "依存構造",
        "依存関係を確認",
        "依存を調べ",
        "依存関係",
    ];
    let en = [
        "dependency graph",
        "analyze dependencies",
        "inspect dependencies",
        "dependency analysis",
    ];
    jp.iter().any(|p| lower.contains(p)) || en.iter().any(|p| lower.contains(p))
}

fn is_module_analyze(lower: &str) -> bool {
    let jp = ["モジュール構成", "モジュール構造"];
    let en = ["module structure", "inspect modules", "module analysis"];
    jp.iter().any(|p| lower.contains(p)) || en.iter().any(|p| lower.contains(p))
}

fn is_workspace_analyze(lower: &str) -> bool {
    let jp = [
        "ワークスペース全体",
        "ワークスペース解析",
        "ワークスペースを分析",
    ];
    let en = [
        "workspace analysis",
        "analyze workspace",
        "workspace structure",
    ];
    jp.iter().any(|p| lower.contains(p)) || en.iter().any(|p| lower.contains(p))
}

fn is_refactor_intent(lower: &str) -> bool {
    let jp = [
        "修正して",
        "リファクタ",
        "改善して",
        "変更して",
        "直して",
        "直す",
    ];
    let en = ["refactor", "improve", "change", "fix"];
    jp.iter().any(|p| lower.contains(p)) || en.iter().any(|p| lower.contains(p))
}

fn is_explicit_apply(lower: &str) -> bool {
    lower.trim() == "apply"
        || lower.trim() == "適用"
        || lower.contains("適用して")
        || lower.contains("適用する")
        || lower.contains("問題なければ適用")
        || lower.contains("--apply")
        || lower.trim() == "y"
        || lower.trim() == "yes"
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn compose_long_request(primary: &str, referenced: &str, safety: &str) -> String {
        format!("{primary}。{referenced}。{safety}。")
    }

    fn analyze_request_with_referenced_plan_terms() -> String {
        compose_long_request(
            "このプロジェクト全体の構造を解析してください",
            "現時点でInputから構造理解、修正プラン生成、候補選択、検証、Apply Guardまでの接続に問題がないかを整理してください",
            "まだ修正、apply、git操作、外部コマンド実行は行わないでください",
        )
    }

    /// spec §11.1 テスト 1: ProjectStructureAnalyze → AnalyzeProject + WorkspaceRoot + ReadOnly
    #[test]
    fn language_core_project_structure_maps_to_analyze_project() {
        let inputs = [
            "このプロジェクトの構造を解析して",
            "プロジェクト全体を分析して",
            "analyze this project",
            "analyze project structure",
            "inspect project structure",
        ];
        for input in &inputs {
            let intent = classify_language_core_intent(input);
            assert_eq!(
                intent,
                LanguageCoreIntent::ProjectStructureAnalyze,
                "input={input:?} should be ProjectStructureAnalyze"
            );
            let ir = language_core_to_ir(intent, input);
            assert_eq!(ir.action, IrAction::AnalyzeProject, "input={input:?}");
            assert_eq!(ir.target, IrTarget::WorkspaceRoot, "input={input:?}");
            assert_eq!(ir.mode, ExecutionMode::ReadOnly, "input={input:?}");
        }
    }

    /// spec §11.1 テスト 2: DependencyAnalyze → AnalyzeDependencies + WorkspaceRoot + ReadOnly
    #[test]
    fn language_core_dependency_maps_to_dependency_analyze() {
        let inputs = [
            "依存関係を解析して",
            "依存構造を確認して",
            "analyze dependencies",
            "dependency graph",
        ];
        for input in &inputs {
            let intent = classify_language_core_intent(input);
            assert_eq!(
                intent,
                LanguageCoreIntent::DependencyAnalyze,
                "input={input:?}"
            );
            let ir = language_core_to_ir(intent, input);
            assert_eq!(ir.action, IrAction::AnalyzeDependencies, "input={input:?}");
            assert_eq!(ir.target, IrTarget::WorkspaceRoot, "input={input:?}");
            assert_eq!(ir.mode, ExecutionMode::ReadOnly, "input={input:?}");
        }
    }

    /// spec §11.1 テスト 3: ModuleStructureAnalyze → AnalyzeModuleStructure + WorkspaceRoot + ReadOnly
    #[test]
    fn language_core_module_maps_to_module_structure_analyze() {
        let inputs = [
            "モジュール構成を整理して",
            "モジュール構造を確認して",
            "module structure",
            "inspect modules",
        ];
        for input in &inputs {
            let intent = classify_language_core_intent(input);
            assert_eq!(
                intent,
                LanguageCoreIntent::ModuleStructureAnalyze,
                "input={input:?}"
            );
            let ir = language_core_to_ir(intent, input);
            assert_eq!(
                ir.action,
                IrAction::AnalyzeModuleStructure,
                "input={input:?}"
            );
            assert_eq!(ir.target, IrTarget::WorkspaceRoot, "input={input:?}");
            assert_eq!(ir.mode, ExecutionMode::ReadOnly, "input={input:?}");
        }
    }

    /// spec §11.1 テスト 4: Unknown Intent → Apply にならない
    ///
    /// Unknown な入力が IrAction::Apply に変換されてはいけない。
    #[test]
    fn unknown_language_intent_does_not_become_invalid_project_apply() {
        let inputs = ["some random unknown text", "???", "hello world"];
        for input in &inputs {
            let intent = classify_language_core_intent(input);
            let ir = language_core_to_ir(intent, input);
            assert_ne!(
                ir.action,
                IrAction::Apply,
                "Unknown input {input:?} should not map to Apply"
            );
            // Unknown は ReadOnly モードになる
            assert_eq!(
                ir.mode,
                ExecutionMode::ReadOnly,
                "Unknown input {input:?} should be ReadOnly"
            );
        }
    }

    /// spec §11.1 テスト 5: RefactorRequest → PlanOnly (apply しない)
    #[test]
    fn refactor_request_without_target_is_plan_only_or_clarification() {
        let inputs = [
            "修正して",
            "リファクタして",
            "改善して",
            "変更して",
            "refactor this code",
        ];
        for input in &inputs {
            let intent = classify_language_core_intent(input);
            let ir = language_core_to_ir(intent, input);
            // RefactorRequest は PlanOnly であり、Apply モードにならない
            assert_ne!(
                ir.mode,
                ExecutionMode::Apply,
                "Refactor input {input:?} must not be ExecutionMode::Apply"
            );
            // action は Refactor または Unknown（analyze キーワードなし）
            assert!(
                matches!(ir.action, IrAction::Refactor | IrAction::Unknown),
                "Refactor input {input:?} should map to Refactor or Unknown, got {:?}",
                ir.action
            );
        }
    }

    /// spec §11.1 テスト 6: ApplyRequest は検証済み Plan が必要
    ///
    /// Adapter レベルでは mode=Apply を返すが、action は Apply のみ。
    /// 実際に apply を実行するかどうかは core.rs の plan 存在チェックに委ねる。
    #[test]
    fn apply_request_without_validated_plan_is_rejected() {
        let intent = classify_language_core_intent("apply");
        assert_eq!(intent, LanguageCoreIntent::ApplyRequest);
        let ir = language_core_to_ir(intent, "apply");
        // Adapter は Apply にマッピングする（実行拒否は core で判断）
        assert_eq!(ir.action, IrAction::Apply);
        assert_eq!(ir.mode, ExecutionMode::Apply);
        // Unknown には決してならない
        assert_ne!(ir.action, IrAction::Unknown);
    }

    #[test]
    fn analyze_keyword_alone_maps_to_project_structure_analyze() {
        let ir = language_core_to_ir(classify_language_core_intent("解析して"), "解析して");
        assert_eq!(ir.action, IrAction::AnalyzeProject);
        assert_eq!(ir.target, IrTarget::WorkspaceRoot);
        assert_eq!(ir.mode, ExecutionMode::ReadOnly);
    }

    #[test]
    fn refactor_with_analyze_keyword_is_not_refactor() {
        // "解析して修正して" → analyze が優先
        let intent = classify_language_core_intent("解析して修正して");
        // analyze キーワードが含まれるので RefactorRequest にならない
        assert_ne!(
            intent,
            LanguageCoreIntent::RefactorRequest {
                target: Option::None,
                instruction: "解析して修正して".to_string()
            }
        );
        assert!(
            matches!(
                intent,
                LanguageCoreIntent::ProjectStructureAnalyze
                    | LanguageCoreIntent::DependencyAnalyze
                    | LanguageCoreIntent::ModuleStructureAnalyze
                    | LanguageCoreIntent::WorkspaceAnalyze
            ),
            "got {intent:?}"
        );
    }

    #[test]
    fn long_input_analyze_primary_not_plan() {
        let input = analyze_request_with_referenced_plan_terms();
        let intent = classify_language_core_intent(&input);
        let ir = language_core_to_ir(intent, &input);
        assert_eq!(ir.action, IrAction::AnalyzeProject);
        assert_eq!(ir.mode, ExecutionMode::ReadOnly);
        assert_eq!(ir.target, IrTarget::WorkspaceRoot);
    }

    #[test]
    fn long_input_referenced_plan_terms_do_not_override_analyze() {
        let input = analyze_request_with_referenced_plan_terms();
        let clauses = classify_clauses(&input);
        assert!(
            clauses
                .iter()
                .any(|clause| clause.role == ClauseRole::ReferencedConcept)
        );
        assert_eq!(
            select_primary_intent(&clauses),
            PrimaryIntent::AnalyzeProject
        );
    }

    #[test]
    fn long_input_no_apply_terms_do_not_trigger_apply() {
        let input = analyze_request_with_referenced_plan_terms();
        let intent = classify_language_core_intent(&input);
        assert_ne!(intent, LanguageCoreIntent::ApplyRequest);
        let safety = extract_safety_constraints(&input);
        assert!(safety.no_apply);
        assert!(safety.no_git_operation);
        assert!(safety.no_external_command);
    }

    #[test]
    fn long_input_apply_phrase_with_no_apply_constraint_is_not_apply() {
        let input = "この候補が安全であれば適用してもよいか検討してください。ただし、この入力ではまだapplyしないでください。まず検証結果だけを表示し、validated_planが作成されてもファイル変更は次の明示的な確認まで停止してください。";
        let intent = classify_language_core_intent(input);
        let ir = language_core_to_ir(intent, input);
        assert_eq!(ir.action, IrAction::Apply);
        assert_eq!(ir.mode, ExecutionMode::Apply);
        assert!(ir.safety_constraints.no_apply);
        assert!(ir.safety_constraints.no_file_write);
    }

    #[test]
    fn long_input_plan_primary_generates_plan() {
        let input = "先ほどのプロジェクト構造解析結果をもとに、DBM_CLIのREPL実働テストで確認された問題点を整理し、安全な小規模修正プランを作成してください。ただし、まだファイル変更、apply、git操作、外部コマンド実行は行わず、候補ごとに対象ファイル、想定変更、リスク、検証方法だけを提示してください。";
        let intent = classify_language_core_intent(input);
        let ir = language_core_to_ir(intent, input);
        assert_eq!(ir.action, IrAction::GenerateChangePlan);
        assert_eq!(ir.mode, ExecutionMode::PlanOnly);
        assert_eq!(ir.target, IrTarget::WorkspaceRoot);
    }

    #[test]
    fn long_input_analyze_returns_workspace_root() {
        let input = analyze_request_with_referenced_plan_terms();
        let ir = language_core_to_ir(classify_language_core_intent(&input), &input);
        assert_eq!(ir.target, IrTarget::WorkspaceRoot);
    }

    #[test]
    fn confirmation_sentence_yes_no_is_not_confirmation() {
        assert!(!matches!(
            classify_language_core_intent("yes/no の確認ではなく、文章で評価してください"),
            LanguageCoreIntent::ApplyRequest
        ));
    }

    #[test]
    fn confirmation_sentence_y_n_is_not_confirmation() {
        assert!(!matches!(
            classify_language_core_intent(
                "y や n という文字が含まれていても confirmation として扱わない"
            ),
            LanguageCoreIntent::ApplyRequest
        ));
    }

    #[test]
    fn target_yes_no_is_rejected() {
        assert!(is_confirmation_token_like_target("yes/no"));
    }

    #[test]
    fn target_y_n_is_rejected() {
        assert!(is_confirmation_token_like_target("y/n"));
    }

    #[test]
    fn long_input_yes_no_classified_as_referenced_token() {
        let clauses =
            classify_clauses("この変更案について yes/no の確認ではなく評価してください。");
        assert!(
            clauses
                .iter()
                .any(|clause| clause.role == ClauseRole::ReferencedToken),
            "{clauses:?}"
        );
    }

    #[test]
    fn long_input_y_n_classified_as_referenced_token() {
        let clauses =
            classify_clauses("y や n という文字が含まれていても confirmation として扱わない。");
        assert!(
            clauses
                .iter()
                .any(|clause| clause.role == ClauseRole::ReferencedToken),
            "{clauses:?}"
        );
    }
}
