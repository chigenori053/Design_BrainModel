use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::language_engine::Explanation;

const MAX_STRENGTHS: usize = 3;
const MAX_ISSUES: usize = 5;
const EVAL_VERSION: &str = "v1.0";
const AMBIGUOUS_TERMS: [&str; 7] = [
    "大規模",
    "柔軟",
    "最適",
    "効率的",
    "高性能",
    "ユーザーフレンドリー",
    "拡張可能",
];
const QUANT_MARKERS: [&str; 5] = ["以上", "以下", "程度", "可能", "対応"];
const SUBJECTLESS_PATTERNS: [&str; 2] = ["する予定", "を改善"];
const CONDITIONLESS_PATTERNS: [&str; 2] = ["スケール可能", "拡張可能"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealizationMode {
    #[serde(rename = "LLM_CONTROLLED")]
    LlmControlled,
    #[serde(rename = "RULE_BASED")]
    RuleBased,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OverallState {
    #[serde(rename = "PARTIAL_READY")]
    PartialReady,
    #[serde(rename = "READY")]
    Ready,
    #[serde(rename = "INSUFFICIENT")]
    Insufficient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReasoningAxis {
    #[serde(rename = "PROBLEM_DEFINITION")]
    ProblemDefinition,
    #[serde(rename = "TARGET_USER")]
    TargetUser,
    #[serde(rename = "VALUE_PROPOSITION")]
    ValueProposition,
    #[serde(rename = "SUCCESS_METRIC")]
    SuccessMetric,
    #[serde(rename = "SCOPE_BOUNDARY")]
    ScopeBoundary,
    #[serde(rename = "CONSTRAINT")]
    Constraint,
    #[serde(rename = "TECHNICAL_STRATEGY")]
    TechnicalStrategy,
    #[serde(rename = "RISK_ASSUMPTION")]
    RiskAssumption,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AxisCategory {
    #[serde(rename = "CORE")]
    Core,
    #[serde(rename = "EXECUTION")]
    Execution,
    #[serde(rename = "STABILITY")]
    Stability,
}

impl ReasoningAxis {
    fn as_str(self) -> &'static str {
        match self {
            ReasoningAxis::ProblemDefinition => "PROBLEM_DEFINITION",
            ReasoningAxis::TargetUser => "TARGET_USER",
            ReasoningAxis::ValueProposition => "VALUE_PROPOSITION",
            ReasoningAxis::SuccessMetric => "SUCCESS_METRIC",
            ReasoningAxis::ScopeBoundary => "SCOPE_BOUNDARY",
            ReasoningAxis::Constraint => "CONSTRAINT",
            ReasoningAxis::TechnicalStrategy => "TECHNICAL_STRATEGY",
            ReasoningAxis::RiskAssumption => "RISK_ASSUMPTION",
        }
    }

    fn category(self) -> AxisCategory {
        match self {
            ReasoningAxis::ProblemDefinition
            | ReasoningAxis::TargetUser
            | ReasoningAxis::ValueProposition => AxisCategory::Core,
            ReasoningAxis::SuccessMetric
            | ReasoningAxis::TechnicalStrategy
            | ReasoningAxis::Constraint => AxisCategory::Execution,
            ReasoningAxis::ScopeBoundary | ReasoningAxis::RiskAssumption => AxisCategory::Stability,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IssueType {
    #[serde(rename = "MISSING")]
    Missing,
    #[serde(rename = "AMBIGUOUS")]
    Ambiguous,
    #[serde(rename = "WEAK")]
    Weak,
    #[serde(rename = "MINOR")]
    Minor,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredReasoningInput {
    pub source_text: String,
    pub selected_objective: Option<String>,
    pub requirement_count: usize,
    pub stability_score: f64,
    pub ambiguity_score: f64,
    pub evidence_spans: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SrtStrength {
    pub axis: ReasoningAxis,
    pub evidence_span: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SrtIssue {
    #[serde(rename = "type")]
    pub issue_type: IssueType,
    pub axis: ReasoningAxis,
    pub span: Option<String>,
    pub reason: Option<String>,
    pub severity: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SrtConsistencyWarning {
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredReasoningTrace {
    pub evaluation_version: String,
    pub input_digest: String,
    pub overall_state: OverallState,
    pub strengths: Vec<SrtStrength>,
    pub issues: Vec<SrtIssue>,
    pub consistency_warnings: Vec<SrtConsistencyWarning>,
    pub next_priority_axis: ReasoningAxis,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizedExplanation {
    pub summary: String,
    pub key_issues: Vec<String>,
    pub next_action: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model_name: String,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
    pub system_prompt_version: String,
    pub validation_rule_version: String,
    pub seed: Option<u64>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "internal".to_string(),
            model_name: "deterministic-v1".to_string(),
            temperature: 0.0,
            top_p: 1.0,
            max_tokens: 256,
            system_prompt_version: "v1".to_string(),
            validation_rule_version: "v1.0".to_string(),
            seed: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationError {
    TooManySentences,
    ContainsForbiddenNumber,
    AxisOutOfScope(String),
    UnwantedProposalOutsideNextAction,
}

impl ValidationError {
    fn as_str(&self) -> String {
        match self {
            ValidationError::TooManySentences => "TooManySentences".to_string(),
            ValidationError::ContainsForbiddenNumber => "ContainsForbiddenNumber".to_string(),
            ValidationError::AxisOutOfScope(axis) => format!("AxisOutOfScope:{axis}"),
            ValidationError::UnwantedProposalOutsideNextAction => {
                "UnwantedProposalOutsideNextAction".to_string()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredExplanationResult {
    pub mode: RealizationMode,
    pub model_version: String,
    pub srt_hash: String,
    pub cache_key: String,
    pub llm_cache_key: String,
    pub fallback_reason: Option<String>,
    pub cache_hit: bool,
    pub srt: StructuredReasoningTrace,
    pub output: RealizedExplanation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AmbiguitySpan {
    span: String,
    reason: &'static str,
    axis: ReasoningAxis,
}

#[derive(Clone, Debug, PartialEq)]
struct AmbiguityAnalysis {
    score: f64,
    ambiguous_word_count: usize,
    quantitative_missing_count: usize,
    subject_missing_count: usize,
    spans: Vec<AmbiguitySpan>,
}

#[derive(Default)]
pub struct StructuredReasoningEngine {
    cache: RefCell<BTreeMap<String, RealizedExplanation>>,
    llm_render_calls: RefCell<u64>,
}

impl StructuredReasoningEngine {
    pub fn llm_call_count(&self) -> u64 {
        *self.llm_render_calls.borrow()
    }

    pub fn reset_llm_call_count(&self) {
        *self.llm_render_calls.borrow_mut() = 0;
    }

    pub fn cache_len(&self) -> usize {
        self.cache.borrow().len()
    }

    pub fn build_srt(&self, input: &StructuredReasoningInput) -> StructuredReasoningTrace {
        let normalized = normalize_input(input);
        let input_digest = digest_hex(&normalized);
        let ambiguity = analyze_ambiguity(&normalized.source_text);
        let mut strengths = Vec::new();
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let evidence = preferred_evidence(&normalized.evidence_spans, &normalized.source_text);

        if let Some(obj) = &normalized.selected_objective {
            strengths.push(SrtStrength {
                axis: ReasoningAxis::ProblemDefinition,
                evidence_span: obj.clone(),
                confidence: round6(clamp01(1.0 - ambiguity.score * 0.5)),
            });
            strengths.push(SrtStrength {
                axis: ReasoningAxis::ValueProposition,
                evidence_span: obj.clone(),
                confidence: round6(clamp01(0.6 + (1.0 - ambiguity.score) * 0.3)),
            });
        } else {
            issues.push(SrtIssue {
                issue_type: IssueType::Missing,
                axis: ReasoningAxis::ProblemDefinition,
                span: None,
                reason: Some("設計目標の記述が不足".to_string()),
                severity: severity_for(
                    ReasoningAxis::ProblemDefinition,
                    IssueType::Missing,
                    impact_weight(ReasoningAxis::ProblemDefinition),
                ),
            });
            issues.push(SrtIssue {
                issue_type: IssueType::Missing,
                axis: ReasoningAxis::ValueProposition,
                span: None,
                reason: Some("提供価値の差別化が不足".to_string()),
                severity: severity_for(
                    ReasoningAxis::ValueProposition,
                    IssueType::Missing,
                    impact_weight(ReasoningAxis::ValueProposition),
                ),
            });
        }

        if normalized.requirement_count == 0 {
            issues.push(SrtIssue {
                issue_type: IssueType::Missing,
                axis: ReasoningAxis::SuccessMetric,
                span: None,
                reason: Some("成功条件が定義されていない".to_string()),
                severity: severity_for(
                    ReasoningAxis::SuccessMetric,
                    IssueType::Missing,
                    impact_weight(ReasoningAxis::SuccessMetric),
                ),
            });
        } else {
            strengths.push(SrtStrength {
                axis: ReasoningAxis::SuccessMetric,
                evidence_span: evidence.clone(),
                confidence: round6(0.68),
            });
        }

        if normalized.source_text.len() < 24 {
            issues.push(SrtIssue {
                issue_type: IssueType::Ambiguous,
                axis: ReasoningAxis::TargetUser,
                span: Some(evidence.clone()),
                reason: Some("対象ユーザー属性が不足".to_string()),
                severity: severity_for(
                    ReasoningAxis::TargetUser,
                    IssueType::Ambiguous,
                    impact_weight(ReasoningAxis::TargetUser),
                ),
            });
        } else {
            strengths.push(SrtStrength {
                axis: ReasoningAxis::TargetUser,
                evidence_span: evidence.clone(),
                confidence: round6(0.62),
            });
        }

        if ambiguity.score >= 0.45 {
            for hit in &ambiguity.spans {
                issues.push(SrtIssue {
                    issue_type: IssueType::Ambiguous,
                    axis: hit.axis,
                    span: Some(hit.span.clone()),
                    reason: Some(hit.reason.to_string()),
                    severity: severity_for(hit.axis, IssueType::Ambiguous, impact_weight(hit.axis)),
                });
            }
            if ambiguity.spans.is_empty() {
                issues.push(SrtIssue {
                    issue_type: IssueType::Ambiguous,
                    axis: ReasoningAxis::ScopeBoundary,
                    span: Some(evidence.clone()),
                    reason: Some("定量条件未指定".to_string()),
                    severity: severity_for(
                        ReasoningAxis::ScopeBoundary,
                        IssueType::Ambiguous,
                        impact_weight(ReasoningAxis::ScopeBoundary),
                    ),
                });
            }
        }

        if normalized.stability_score < 0.55 {
            issues.push(SrtIssue {
                issue_type: IssueType::Weak,
                axis: ReasoningAxis::TechnicalStrategy,
                span: None,
                reason: Some("技術方針の根拠が弱い".to_string()),
                severity: severity_for(
                    ReasoningAxis::TechnicalStrategy,
                    IssueType::Weak,
                    impact_weight(ReasoningAxis::TechnicalStrategy),
                ),
            });
            issues.push(SrtIssue {
                issue_type: IssueType::Weak,
                axis: ReasoningAxis::RiskAssumption,
                span: None,
                reason: Some("不確実性の明示が不足".to_string()),
                severity: severity_for(
                    ReasoningAxis::RiskAssumption,
                    IssueType::Weak,
                    impact_weight(ReasoningAxis::RiskAssumption),
                ),
            });
            warnings.push(SrtConsistencyWarning {
                description: "スケール要件に対するインフラ未定義".to_string(),
            });
        } else {
            strengths.push(SrtStrength {
                axis: ReasoningAxis::TechnicalStrategy,
                evidence_span: evidence,
                confidence: round6(clamp01(normalized.stability_score)),
            });
        }
        if normalized.requirement_count < 2 {
            issues.push(SrtIssue {
                issue_type: IssueType::Minor,
                axis: ReasoningAxis::Constraint,
                span: None,
                reason: Some("制約条件が十分に列挙されていない".to_string()),
                severity: severity_for(
                    ReasoningAxis::Constraint,
                    IssueType::Minor,
                    impact_weight(ReasoningAxis::Constraint),
                ),
            });
        }

        strengths.sort_by(|a, b| {
            b.confidence
                .total_cmp(&a.confidence)
                .then(a.axis.category().cmp(&b.axis.category()))
                .then(a.axis.cmp(&b.axis))
        });
        strengths.truncate(MAX_STRENGTHS);
        for s in &mut strengths {
            s.confidence = round6(clamp01(s.confidence));
        }

        issues.sort_by(|a, b| {
            b.severity
                .total_cmp(&a.severity)
                .then(a.issue_type.cmp(&b.issue_type))
                .then(a.axis.cmp(&b.axis))
        });
        issues.truncate(MAX_ISSUES);
        for issue in &mut issues {
            issue.severity = round6(clamp01(issue.severity));
        }

        let max_severity = issues.first().map(|i| i.severity).unwrap_or(0.0);
        let overall_state = if max_severity < 0.25 {
            OverallState::Ready
        } else if max_severity < 0.6 {
            OverallState::PartialReady
        } else {
            OverallState::Insufficient
        };
        let next_priority_axis = issues
            .first()
            .map(|i| i.axis)
            .unwrap_or(ReasoningAxis::SuccessMetric);

        StructuredReasoningTrace {
            evaluation_version: EVAL_VERSION.to_string(),
            input_digest,
            overall_state,
            strengths,
            issues,
            consistency_warnings: warnings,
            next_priority_axis,
        }
    }

    pub fn realize(
        &self,
        input: &StructuredReasoningInput,
        mode: RealizationMode,
    ) -> StructuredExplanationResult {
        self.realize_with_model_config(input, mode, &ModelConfig::default())
    }

    pub fn realize_with_model_config(
        &self,
        input: &StructuredReasoningInput,
        mode: RealizationMode,
        config: &ModelConfig,
    ) -> StructuredExplanationResult {
        let srt = self.build_srt(input);
        let srt_hash = canonical_srt_hash(&srt);
        let cache_key = format!("{}:{}", srt.input_digest, srt_hash);
        let model_version = model_version(config);
        let llm_cache_key = llm_cache_key(&model_version, &cache_key);
        let lookup_key = match mode {
            RealizationMode::LlmControlled => llm_cache_key.clone(),
            RealizationMode::RuleBased => cache_key.clone(),
        };

        if let Some(hit) = self.cache.borrow().get(&lookup_key).cloned() {
            return StructuredExplanationResult {
                mode,
                model_version,
                srt_hash,
                cache_key,
                llm_cache_key,
                fallback_reason: None,
                cache_hit: true,
                srt,
                output: hit,
            };
        }

        let (output, realized_mode, fallback_reason) = match mode {
            RealizationMode::LlmControlled => {
                *self.llm_render_calls.borrow_mut() += 1;
                let llm_output = normalize_realized_explanation_for_output(llm_controlled_render(&srt));
                let text_for_validation = render_validation_text(&llm_output);
                match validate_llm_output(&text_for_validation, &srt) {
                    Ok(()) => (llm_output, RealizationMode::LlmControlled, None),
                    Err(reason) => (
                        normalize_realized_explanation_for_output(rule_based_render(&srt)),
                        RealizationMode::RuleBased,
                        Some(reason.as_str()),
                    ),
                }
            }
            RealizationMode::RuleBased => (
                normalize_realized_explanation_for_output(rule_based_render(&srt)),
                RealizationMode::RuleBased,
                None,
            ),
        };
        self.cache
            .borrow_mut()
            .insert(lookup_key, output.clone());

        StructuredExplanationResult {
            mode: realized_mode,
            model_version,
            srt_hash,
            cache_key,
            llm_cache_key,
            fallback_reason,
            cache_hit: false,
            srt,
            output,
        }
    }
}

pub fn parse_realization_mode_from_env() -> RealizationMode {
    match std::env::var("DESIGN_EXPLAIN_MODE") {
        Ok(v) if v.eq_ignore_ascii_case("RULE_BASED") => RealizationMode::RuleBased,
        _ => RealizationMode::LlmControlled,
    }
}

pub fn format_explanation(result: &StructuredExplanationResult) -> Explanation {
    let mut summary_lines = vec![result.output.summary.clone()];
    for issue in &result.output.key_issues {
        summary_lines.push(format!("- {issue}"));
    }
    summary_lines.push(format!("次のアクション: {}", result.output.next_action));

    let detail = serde_json::json!({
        "mode": match result.mode {
            RealizationMode::LlmControlled => "LLM_CONTROLLED",
            RealizationMode::RuleBased => "RULE_BASED",
        },
        "model_version": result.model_version,
        "cache_key": result.cache_key,
        "llm_cache_key": result.llm_cache_key,
        "srt_hash": result.srt_hash,
        "fallback_reason": result.fallback_reason,
        "overall_state": result.srt.overall_state,
        "next_priority_axis": result.srt.next_priority_axis,
        "issues": result.srt.issues.iter().map(|i| serde_json::json!({
            "type": i.issue_type,
            "axis": i.axis,
            "span": i.span,
            "reason": i.reason,
        })).collect::<Vec<_>>(),
        "output": result.output,
    })
    .to_string();

    Explanation {
        summary: summary_lines.join("\n"),
        detail,
    }
}

fn llm_controlled_render(srt: &StructuredReasoningTrace) -> RealizedExplanation {
    let summary = state_summary_sentence(srt.overall_state);
    let mut key_issues = srt
        .issues
        .iter()
        .take(2)
        .map(issue_sentence)
        .collect::<Vec<_>>();
    if key_issues.is_empty() {
        key_issues.push("重大な課題は検出されていません。".to_string());
    }
    RealizedExplanation {
        summary: summary.to_string(),
        key_issues,
        next_action: next_action_sentence(srt.next_priority_axis).to_string(),
    }
}

fn rule_based_render(srt: &StructuredReasoningTrace) -> RealizedExplanation {
    let mut key_issues = srt
        .issues
        .iter()
        .take(3)
        .map(issue_sentence)
        .collect::<Vec<_>>();
    if key_issues.is_empty() {
        key_issues.push("課題は検出されませんでした。".to_string());
    }
    let summary = state_summary_sentence(srt.overall_state);
    RealizedExplanation {
        summary: summary.to_string(),
        key_issues,
        next_action: next_action_sentence(srt.next_priority_axis).to_string(),
    }
}

fn state_summary_sentence(state: OverallState) -> &'static str {
    match state {
        OverallState::Ready => "設計は実装可能な水準に達しています。",
        OverallState::PartialReady => {
            "設計は概ね整理されています。以下の点を明確にすると安定します。"
        }
        OverallState::Insufficient => {
            "設計の基礎要素が不足しています。優先項目から整理すると安定します。"
        }
    }
}

fn issue_sentence(issue: &SrtIssue) -> String {
    match issue.issue_type {
        IssueType::Missing => format!("「{}」が未定義です。", issue.axis.as_str()),
        IssueType::Ambiguous => {
            let span = issue
                .span
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("該当箇所");
            format!("「{span}」は具体化が必要です。")
        }
        IssueType::Weak => format!(
            "「{}」の根拠が弱いため補強が必要です。",
            issue.axis.as_str()
        ),
        IssueType::Minor => format!("「{}」の補足情報を追加してください。", issue.axis.as_str()),
    }
}

fn next_action_sentence(axis: ReasoningAxis) -> &'static str {
    match axis {
        ReasoningAxis::ProblemDefinition => "解決課題と現状との差分を具体化してください。",
        ReasoningAxis::TargetUser => "対象ユーザーの属性と利用場面を明示してください。",
        ReasoningAxis::ValueProposition => "提供価値と既存との差別化を明文化してください。",
        ReasoningAxis::SuccessMetric => "成功指標を観測可能な条件で追加してください。",
        ReasoningAxis::ScopeBoundary => "含む範囲と含まない範囲を境界として定義してください。",
        ReasoningAxis::Constraint => "技術・予算・期間・法規制の制約を明記してください。",
        ReasoningAxis::TechnicalStrategy => {
            "技術選定理由とアーキテクチャ方針を明確化してください。"
        }
        ReasoningAxis::RiskAssumption => "主要な不確実性と外部依存を列挙してください。",
    }
}

fn base_weight(axis: ReasoningAxis) -> f64 {
    match axis {
        ReasoningAxis::ProblemDefinition => 1.0,
        ReasoningAxis::TargetUser => 0.9,
        ReasoningAxis::ValueProposition => 1.0,
        ReasoningAxis::SuccessMetric => 0.85,
        ReasoningAxis::ScopeBoundary => 0.8,
        ReasoningAxis::Constraint => 0.9,
        ReasoningAxis::TechnicalStrategy => 0.75,
        ReasoningAxis::RiskAssumption => 0.7,
    }
}

fn deficiency_level(issue_type: IssueType) -> f64 {
    match issue_type {
        IssueType::Missing => 1.0,
        IssueType::Ambiguous => 0.7,
        IssueType::Weak => 0.5,
        IssueType::Minor => 0.3,
    }
}

fn impact_weight(axis: ReasoningAxis) -> f64 {
    let mut weight = 1.0;
    if dependency_count(axis) >= 3 {
        weight += 0.2;
    }
    if propagates_to_other_axes(axis) {
        weight += 0.15;
    }
    weight
}

fn dependency_count(axis: ReasoningAxis) -> usize {
    match axis {
        ReasoningAxis::ProblemDefinition => 4,
        ReasoningAxis::TargetUser => 3,
        ReasoningAxis::ValueProposition => 4,
        ReasoningAxis::SuccessMetric => 3,
        ReasoningAxis::ScopeBoundary => 2,
        ReasoningAxis::Constraint => 3,
        ReasoningAxis::TechnicalStrategy => 2,
        ReasoningAxis::RiskAssumption => 2,
    }
}

fn propagates_to_other_axes(axis: ReasoningAxis) -> bool {
    matches!(
        axis,
        ReasoningAxis::ProblemDefinition
            | ReasoningAxis::TargetUser
            | ReasoningAxis::ValueProposition
            | ReasoningAxis::SuccessMetric
            | ReasoningAxis::Constraint
    )
}

fn severity_for(axis: ReasoningAxis, issue_type: IssueType, impact_weight: f64) -> f64 {
    round6(clamp01(
        base_weight(axis) * deficiency_level(issue_type) * impact_weight,
    ))
}

fn preferred_evidence(spans: &[String], source_text: &str) -> String {
    if let Some(first) = spans.iter().find(|s| !s.trim().is_empty()) {
        return first.clone();
    }
    if let Some(first_line) = source_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        return first_line.to_string();
    }
    "入力全体".to_string()
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn normalize_string(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_input(input: &StructuredReasoningInput) -> StructuredReasoningInput {
    let mut spans = input
        .evidence_spans
        .iter()
        .map(|s| normalize_string(s))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    spans.sort();
    spans.dedup();
    StructuredReasoningInput {
        source_text: normalize_string(&input.source_text),
        selected_objective: input
            .selected_objective
            .as_ref()
            .map(|s| normalize_string(s)),
        requirement_count: input.requirement_count,
        stability_score: round6(clamp01(input.stability_score)),
        ambiguity_score: round6(clamp01(input.ambiguity_score)),
        evidence_spans: spans,
    }
}

fn analyze_ambiguity(source_text: &str) -> AmbiguityAnalysis {
    let segments = split_segments(source_text);
    let mut ambiguous_word_count = 0usize;
    let mut quantitative_missing_count = 0usize;
    let mut subject_missing_count = 0usize;
    let mut spans = Vec::new();

    for seg in segments {
        let has_number = seg.chars().any(|c| c.is_ascii_digit());
        for term in AMBIGUOUS_TERMS {
            if seg.contains(term) {
                ambiguous_word_count += 1;
                spans.push(AmbiguitySpan {
                    span: seg.clone(),
                    reason: "曖昧語を含む",
                    axis: ReasoningAxis::ValueProposition,
                });
            }
        }
        if QUANT_MARKERS.iter().any(|m| seg.contains(m)) && !has_number {
            quantitative_missing_count += 1;
            spans.push(AmbiguitySpan {
                span: seg.clone(),
                reason: "定量条件未指定",
                axis: ReasoningAxis::SuccessMetric,
            });
        }
        if SUBJECTLESS_PATTERNS.iter().any(|p| seg.contains(p)) {
            subject_missing_count += 1;
            spans.push(AmbiguitySpan {
                span: seg.clone(),
                reason: "主体未定義",
                axis: ReasoningAxis::TargetUser,
            });
        }
        if CONDITIONLESS_PATTERNS.iter().any(|p| seg.contains(p)) && !has_number {
            spans.push(AmbiguitySpan {
                span: seg.clone(),
                reason: "条件未定義",
                axis: ReasoningAxis::ScopeBoundary,
            });
        }
    }

    spans.sort_by(|a, b| a.span.cmp(&b.span).then(a.reason.cmp(b.reason)));
    spans.dedup_by(|a, b| a.span == b.span && a.reason == b.reason && a.axis == b.axis);

    let score = round6(clamp01(
        ambiguous_word_count as f64 * 0.1
            + quantitative_missing_count as f64 * 0.2
            + subject_missing_count as f64 * 0.15,
    ));
    AmbiguityAnalysis {
        score,
        ambiguous_word_count,
        quantitative_missing_count,
        subject_missing_count,
        spans,
    }
}

fn split_segments(source_text: &str) -> Vec<String> {
    source_text
        .split(['。', '、', ',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn digest_hex<T: Serialize>(value: &T) -> String {
    let payload = serde_json::to_vec(value).unwrap_or_default();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in payload {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn model_version(config: &ModelConfig) -> String {
    format!(
        "{}:{}:{:.3}:{:.3}:{}:{}:{}:{:?}",
        config.provider,
        config.model_name,
        config.temperature,
        config.top_p,
        config.max_tokens,
        config.system_prompt_version,
        config.validation_rule_version,
        config.seed
    )
}

pub fn llm_cache_key(model_version: &str, cache_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model_version.as_bytes());
    hasher.update(cache_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn render_validation_text(output: &RealizedExplanation) -> String {
    format!(
        "summary:\n{}\nkey_issues:\n{}\nnext_action:\n{}",
        output.summary,
        output.key_issues.join("\n"),
        output.next_action
    )
}

pub fn validate_sentence_count(text: &str) -> Result<(), ValidationError> {
    let sentence_count = text
        .split(['.', '。', '!', '?'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .count();
    if sentence_count > 5 {
        return Err(ValidationError::TooManySentences);
    }
    Ok(())
}

fn validate_no_forbidden_numbers(text: &str) -> Result<(), ValidationError> {
    let re = Regex::new(r"(?i)\b(one|two|three|four|five)\b|\p{N}").unwrap();
    if re.is_match(text) {
        return Err(ValidationError::ContainsForbiddenNumber);
    }
    Ok(())
}

fn validate_axis_scope(text: &str, srt: &StructuredReasoningTrace) -> Result<(), ValidationError> {
    let allowed = srt.issues.iter().map(|i| i.axis).collect::<BTreeSet<_>>();
    let axes = [
        ReasoningAxis::ProblemDefinition,
        ReasoningAxis::TargetUser,
        ReasoningAxis::ValueProposition,
        ReasoningAxis::SuccessMetric,
        ReasoningAxis::ScopeBoundary,
        ReasoningAxis::Constraint,
        ReasoningAxis::TechnicalStrategy,
        ReasoningAxis::RiskAssumption,
    ];
    for axis in axes {
        let token = axis.as_str();
        if text.contains(token) && !allowed.contains(&axis) {
            return Err(ValidationError::AxisOutOfScope(token.to_string()));
        }
    }
    Ok(())
}

fn validate_no_unwanted_proposal_outside_next_action(text: &str) -> Result<(), ValidationError> {
    let lower = text.to_lowercase();
    let marker = "\nnext_action:\n";
    let outside = if let Some(idx) = lower.find(marker) {
        &lower[..idx]
    } else {
        lower.as_str()
    };
    let patterns = ["should", "must", "採用すべき", "導入すべき"];
    if patterns.iter().any(|p| outside.contains(p)) {
        return Err(ValidationError::UnwantedProposalOutsideNextAction);
    }
    Ok(())
}

pub fn validate_llm_output(
    text: &str,
    srt: &StructuredReasoningTrace,
) -> Result<(), ValidationError> {
    validate_sentence_count(text)?;
    validate_no_forbidden_numbers(text)?;
    validate_axis_scope(text, srt)?;
    validate_no_unwanted_proposal_outside_next_action(text)?;
    Ok(())
}

pub fn canonical_srt_hash(srt: &StructuredReasoningTrace) -> String {
    let mut cloned = srt.clone();
    for strength in &mut cloned.strengths {
        strength.confidence = round6(strength.confidence);
    }
    for issue in &mut cloned.issues {
        issue.severity = round6(issue.severity);
    }
    digest_hex(&cloned)
}

pub fn normalize_realized_explanation_for_output(output: RealizedExplanation) -> RealizedExplanation {
    RealizedExplanation {
        summary: normalize_summary_text(&output.summary),
        key_issues: output
            .key_issues
            .iter()
            .map(|s| normalize_issue_text(s))
            .collect(),
        next_action: normalize_tone_text(&output.next_action),
    }
}

pub fn normalize_summary_text(text: &str) -> String {
    let toned = normalize_tone_text(text);
    let sentence_limited = limit_sentences(&toned, 2);
    truncate_chars(&sentence_limited, 100)
}

fn normalize_issue_text(text: &str) -> String {
    let toned = normalize_tone_text(text);
    let sentence_limited = limit_sentences(&toned, 2);
    truncate_chars(&sentence_limited, 80)
}

fn normalize_tone_text(text: &str) -> String {
    let mut out = text.replace('!', "。").replace('！', "。");
    for token in ["現時点では", "いくつかの", "非常に", "絶対に", "重要です", "必須です"] {
        out = out.replace(token, "");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn limit_sentences(text: &str, max_sentences: usize) -> String {
    if max_sentences == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut sentence_count = 0usize;
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '。' | '.' | '?' | '!' | '？') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentence_count += 1;
                if sentence_count <= max_sentences {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(trimmed);
                }
            }
            current.clear();
            if sentence_count >= max_sentences {
                break;
            }
        }
    }
    if sentence_count < max_sentences {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(trimmed);
        }
    }
    out
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    chars.into_iter().take(max_chars).collect()
}
