use std::cell::RefCell;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::language_engine::Explanation;

const MAX_STRENGTHS: usize = 3;
const MAX_ISSUES: usize = 5;
const EVAL_VERSION: &str = "v1.0";

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
pub struct StructuredExplanationResult {
    pub mode: RealizationMode,
    pub srt_hash: String,
    pub cache_key: String,
    pub srt: StructuredReasoningTrace,
    pub output: RealizedExplanation,
}

#[derive(Default)]
pub struct StructuredReasoningEngine {
    cache: RefCell<BTreeMap<String, RealizedExplanation>>,
}

impl StructuredReasoningEngine {
    pub fn build_srt(&self, input: &StructuredReasoningInput) -> StructuredReasoningTrace {
        let normalized = normalize_input(input);
        let input_digest = digest_hex(&normalized);
        let mut strengths = Vec::new();
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let evidence = preferred_evidence(&normalized.evidence_spans, &normalized.source_text);

        if let Some(obj) = &normalized.selected_objective {
            strengths.push(SrtStrength {
                axis: ReasoningAxis::ProblemDefinition,
                evidence_span: obj.clone(),
                confidence: clamp01(1.0 - normalized.ambiguity_score * 0.5),
            });
            strengths.push(SrtStrength {
                axis: ReasoningAxis::ValueProposition,
                evidence_span: obj.clone(),
                confidence: clamp01(0.6 + (1.0 - normalized.ambiguity_score) * 0.3),
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
                confidence: 0.68,
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
                confidence: 0.62,
            });
        }

        if normalized.ambiguity_score >= 0.45 {
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
                confidence: clamp01(normalized.stability_score),
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
            s.confidence = clamp01(s.confidence);
        }

        issues.sort_by(|a, b| {
            b.severity
                .total_cmp(&a.severity)
                .then(a.issue_type.cmp(&b.issue_type))
                .then(a.axis.cmp(&b.axis))
        });
        issues.truncate(MAX_ISSUES);
        for issue in &mut issues {
            issue.severity = clamp01(issue.severity);
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
        let srt = self.build_srt(input);
        let srt_hash = digest_hex(&srt);
        let cache_key = format!("{}:{}", srt.input_digest, srt_hash);

        if let Some(hit) = self.cache.borrow().get(&cache_key).cloned() {
            return StructuredExplanationResult {
                mode,
                srt_hash,
                cache_key,
                srt,
                output: hit,
            };
        }

        let output = match mode {
            RealizationMode::LlmControlled => llm_controlled_render(&srt),
            RealizationMode::RuleBased => rule_based_render(&srt),
        };
        self.cache
            .borrow_mut()
            .insert(cache_key.clone(), output.clone());

        StructuredExplanationResult {
            mode,
            srt_hash,
            cache_key,
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
        "cache_key": result.cache_key,
        "srt_hash": result.srt_hash,
        "srt": result.srt,
        "output": result.output,
    })
    .to_string();

    Explanation {
        summary: summary_lines.join("\n"),
        detail,
    }
}

fn llm_controlled_render(srt: &StructuredReasoningTrace) -> RealizedExplanation {
    let summary = match srt.overall_state {
        OverallState::Ready => "設計は実装可能な状態です。",
        OverallState::PartialReady => "設計は前進していますが、優先課題の明確化が必要です。",
        OverallState::Insufficient => "設計は現時点で不足が多く、再定義が必要です。",
    };
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
    let summary = if srt.overall_state == OverallState::Ready {
        "設計は実装可能な水準に達しています。"
    } else if srt.overall_state == OverallState::PartialReady {
        "設計は部分的に成立しています。"
    } else {
        "設計の成立条件が不足しています。"
    };
    RealizedExplanation {
        summary: summary.to_string(),
        key_issues,
        next_action: next_action_sentence(srt.next_priority_axis).to_string(),
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
    clamp01(base_weight(axis) * deficiency_level(issue_type) * impact_weight)
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
        stability_score: clamp01(input.stability_score),
        ambiguity_score: clamp01(input.ambiguity_score),
        evidence_spans: spans,
    }
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
