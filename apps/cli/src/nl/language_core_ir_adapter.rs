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
    /// 未分類
    Unknown { raw: String },
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
    /// 検証済み Plan の適用
    Apply,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "ReadOnly"),
            Self::PlanOnly => write!(f, "PlanOnly"),
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
}

// ── Public API ────────────────────────────────────────────────────────────────

/// 自然言語入力を [`LanguageCoreIntent`] に分類する。
///
/// 日本語・英語の両方に対応する。分類は先行優先（specific → general）。
pub fn classify_language_core_intent(input: &str) -> LanguageCoreIntent {
    let lower = input.to_lowercase();

    // 修正プラン要求は「構造解析結果」など Analyze 語を含みうるため、
    // Analyze 系 fallback より先に判定する。
    if crate::nl::context_aware_plan_target_resolver::is_plan_only_intent(&lower) {
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
    if is_explicit_apply(&lower) {
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
    match intent {
        LanguageCoreIntent::ProjectStructureAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeProject,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.90,
        },
        LanguageCoreIntent::WorkspaceAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeWorkspace,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
        },
        LanguageCoreIntent::DependencyAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeDependencies,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
        },
        LanguageCoreIntent::ModuleStructureAnalyze => IrIntentRequest {
            action: IrAction::AnalyzeModuleStructure,
            target: IrTarget::WorkspaceRoot,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.85,
        },
        LanguageCoreIntent::FileAnalyze { file } => IrIntentRequest {
            action: IrAction::AnalyzeFile,
            target: IrTarget::File(file),
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.95,
        },
        LanguageCoreIntent::SymbolAnalyze { symbol } => IrIntentRequest {
            action: IrAction::AnalyzeSymbol,
            target: IrTarget::Symbol(symbol),
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.90,
        },
        LanguageCoreIntent::GenerateChangePlan { target, .. } => IrIntentRequest {
            action: IrAction::GenerateChangePlan,
            target: target.map(IrTarget::File).unwrap_or(IrTarget::None),
            mode: ExecutionMode::PlanOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.80,
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
        },
        LanguageCoreIntent::ApplyRequest => IrIntentRequest {
            action: IrAction::Apply,
            target: IrTarget::None,
            // ApplyRequest は検証済み Plan が必要（spec §7.3）
            mode: ExecutionMode::Apply,
            raw_input: raw_input.to_string(),
            confidence: 0.95,
        },
        LanguageCoreIntent::Unknown { .. } => IrIntentRequest {
            action: IrAction::Unknown,
            target: IrTarget::None,
            mode: ExecutionMode::ReadOnly,
            raw_input: raw_input.to_string(),
            confidence: 0.0,
        },
    }
}

// ── Classifier helpers ────────────────────────────────────────────────────────

pub(crate) fn has_analyze_keyword(lower: &str) -> bool {
    ["analyze", "analyse", "解析", "分析", "調べ", "audit"]
        .iter()
        .any(|kw| lower.contains(kw))
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
}
