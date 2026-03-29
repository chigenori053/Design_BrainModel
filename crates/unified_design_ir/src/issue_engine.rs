use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::DesignVersion;
use crate::{
    ChangeType, DiffResult, FieldChange, FieldPath, Impact, ImpactReason, SemanticReason, Stage,
    normalize,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueInput {
    pub before: DesignVersion,
    pub after: DesignVersion,
    pub diff: DiffResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueResult {
    pub issues: Vec<Issue>,
    pub summary: IssueSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IssueId {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub id: IssueId,
    pub issue_type: IssueType,
    pub severity: Severity,
    pub priority: Priority,
    pub order: u64,
    pub blocks: Vec<IssueId>,
    pub path: FieldPath,
    pub reason: IssueReason,
    pub evidence: IssueEvidence,
    pub fix_hint: Option<FixHint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueType {
    Missing,
    Conflict,
    Redundancy,
    OverSpecification,
    UnderSpecification,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IssueReason {
    MissingRequiredField,
    ValueConflict,
    DuplicateDefinition,
    ExcessiveComplexity,
    InsufficientSpecification,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueEvidence {
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub semantic_reason: Option<SemanticReason>,
    pub impact_reason: Option<ImpactReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixHint {
    pub action: FixAction,
    pub target: FieldPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixAction {
    Add,
    Remove,
    Replace,
    Normalize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IssueSummary {
    pub total: usize,
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueError {
    InvalidDiff,
    InconsistentState,
}

#[derive(Clone, Debug)]
struct IssueCandidate {
    change: FieldChange,
    semantic_reason: SemanticReason,
    impact: Impact,
    impact_reason: ImpactReason,
}

pub const MAX_FIXES_PER_ITERATION: usize = 1;

pub fn detect_issues(input: IssueInput) -> Result<IssueResult, IssueError> {
    validate_input(&input)?;

    let after_canonical = normalize(&input.after.design);
    let candidates = generate_candidates(&input);
    let filtered = filter_candidates(candidates);
    let issues = merge_issues(
        filtered
            .into_iter()
            .filter_map(|candidate| {
                classify_issue(candidate, input.after.stage.clone(), &after_canonical)
            })
            .collect(),
    );
    let issues = assign_blocks(issues);
    let issues = assign_order_and_sort(issues);
    let summary = summarize_issues(&issues);

    Ok(IssueResult { issues, summary })
}

fn validate_input(input: &IssueInput) -> Result<(), IssueError> {
    if input.before.stage != input.after.stage {
        return Err(IssueError::InconsistentState);
    }

    if input.diff.summary.added + input.diff.summary.removed + input.diff.summary.modified
        < input.diff.changes.len()
    {
        return Err(IssueError::InvalidDiff);
    }

    Ok(())
}

fn generate_candidates(input: &IssueInput) -> Vec<IssueCandidate> {
    input
        .diff
        .changes
        .iter()
        .filter(|change| {
            !input.diff.semantic.is_equivalent
                || input.diff.impact == Impact::Regressed
                || matches!(
                    change.change_type,
                    ChangeType::Removed | ChangeType::Modified
                )
        })
        .cloned()
        .map(|change| IssueCandidate {
            change,
            semantic_reason: input.diff.semantic.reason.clone(),
            impact: input.diff.impact.clone(),
            impact_reason: input.diff.impact_reason.clone(),
        })
        .collect()
}

fn filter_candidates(candidates: Vec<IssueCandidate>) -> Vec<IssueCandidate> {
    candidates
        .into_iter()
        .filter(|candidate| !matches!(candidate.change.change_type, ChangeType::Moved))
        .filter(|candidate| !matches!(candidate.semantic_reason, SemanticReason::OrderDifference))
        .filter(|candidate| {
            !(candidate.impact == Impact::Improved
                && !matches!(
                    candidate.semantic_reason,
                    SemanticReason::ValueMismatch | SemanticReason::TypeMismatch
                ))
        })
        .collect()
}

fn classify_issue(
    candidate: IssueCandidate,
    stage: Stage,
    after_canonical: &Value,
) -> Option<Issue> {
    let path = candidate.change.path.clone();

    let (issue_type, reason) = if matches!(candidate.change.change_type, ChangeType::Removed)
        && is_required_field(&stage, &path)
    {
        (IssueType::Missing, IssueReason::MissingRequiredField)
    } else if is_under_specified(&candidate.change, &stage, &path, after_canonical) {
        (
            IssueType::UnderSpecification,
            IssueReason::InsufficientSpecification,
        )
    } else if matches!(candidate.change.change_type, ChangeType::Modified)
        && matches!(
            candidate.semantic_reason,
            SemanticReason::ValueMismatch | SemanticReason::TypeMismatch
        )
        && violates_rule(&candidate.change, &stage)
    {
        (IssueType::Conflict, IssueReason::ValueConflict)
    } else if matches!(candidate.change.change_type, ChangeType::Added)
        && is_duplicate_structure(&path, candidate.change.after.as_ref(), after_canonical)
    {
        (IssueType::Redundancy, IssueReason::DuplicateDefinition)
    } else if matches!(candidate.change.change_type, ChangeType::Added)
        && candidate.impact == Impact::Regressed
        && !is_required_field(&stage, &path)
    {
        (
            IssueType::OverSpecification,
            IssueReason::ExcessiveComplexity,
        )
    } else {
        return None;
    };

    let severity = classify_severity(&issue_type);
    let priority = classify_priority(&severity, &candidate.impact);
    let evidence = IssueEvidence {
        before: candidate.change.before.clone(),
        after: candidate.change.after.clone(),
        semantic_reason: Some(candidate.semantic_reason.clone()),
        impact_reason: Some(candidate.impact_reason.clone()),
    };
    let fix_hint = build_fix_hint(&issue_type, &path);
    let id = issue_id(&path, &reason, &candidate.change);

    Some(Issue {
        id,
        issue_type,
        severity,
        priority,
        order: 0,
        blocks: Vec::new(),
        path,
        reason,
        evidence,
        fix_hint,
    })
}

fn merge_issues(issues: Vec<Issue>) -> Vec<Issue> {
    let mut merged = BTreeMap::<(Vec<String>, IssueReason), Issue>::new();
    for issue in issues {
        let key = (issue.path.segments.clone(), issue.reason.clone());
        merged.entry(key).or_insert(issue);
    }
    merged.into_values().collect()
}

fn summarize_issues(issues: &[Issue]) -> IssueSummary {
    let mut summary = IssueSummary {
        total: issues.len(),
        ..IssueSummary::default()
    };

    for issue in issues {
        match issue.severity {
            Severity::Critical => summary.critical += 1,
            Severity::High => summary.high += 1,
            Severity::Medium => summary.medium += 1,
            Severity::Low => summary.low += 1,
        }
    }

    summary
}

fn assign_blocks(mut issues: Vec<Issue>) -> Vec<Issue> {
    let missing_issues = issues
        .iter()
        .filter(|issue| issue.issue_type == IssueType::Missing)
        .map(|issue| (issue.id.clone(), issue.path.clone()))
        .collect::<Vec<_>>();

    for issue in &mut issues {
        if issue.issue_type == IssueType::Missing {
            continue;
        }

        for (missing_id, missing_path) in &missing_issues {
            if path_is_blocked_by(&issue.path, missing_path) {
                issue.blocks.push(missing_id.clone());
            }
        }
    }

    issues
}

fn assign_order_and_sort(mut issues: Vec<Issue>) -> Vec<Issue> {
    issues.sort_by(|left, right| compare_issue_priority(left, right));
    for (index, issue) in issues.iter_mut().enumerate() {
        issue.order = index as u64 + 1;
    }
    issues.sort_by(|a, b| a.order.cmp(&b.order));
    issues
}

fn compare_issue_priority(left: &Issue, right: &Issue) -> std::cmp::Ordering {
    severity_rank(&left.severity)
        .cmp(&severity_rank(&right.severity))
        .then_with(|| issue_type_rank(&left.issue_type).cmp(&issue_type_rank(&right.issue_type)))
        .then_with(|| left.path.segments.len().cmp(&right.path.segments.len()))
        .then_with(|| left.path.segments.cmp(&right.path.segments))
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Critical => 0,
        Severity::High => 1,
        Severity::Medium => 2,
        Severity::Low => 3,
    }
}

fn issue_type_rank(issue_type: &IssueType) -> u8 {
    match issue_type {
        IssueType::Missing => 0,
        IssueType::Conflict => 1,
        IssueType::UnderSpecification => 2,
        IssueType::OverSpecification => 3,
        IssueType::Redundancy => 4,
    }
}

fn path_is_blocked_by(path: &FieldPath, blocker: &FieldPath) -> bool {
    path != blocker
        && path.segments.len() >= blocker.segments.len()
        && path
            .segments
            .iter()
            .zip(&blocker.segments)
            .all(|(a, b)| a == b)
}

fn classify_severity(issue_type: &IssueType) -> Severity {
    match issue_type {
        IssueType::Missing => Severity::Critical,
        IssueType::Conflict => Severity::High,
        IssueType::OverSpecification => Severity::Medium,
        IssueType::Redundancy => Severity::Low,
        IssueType::UnderSpecification => Severity::Medium,
    }
}

fn classify_priority(severity: &Severity, impact: &Impact) -> Priority {
    match (severity, impact) {
        (Severity::Critical, _) => Priority::P0,
        (Severity::High, Impact::Regressed) => Priority::P0,
        (Severity::High, _) => Priority::P1,
        (Severity::Medium, _) => Priority::P2,
        (Severity::Low, _) => Priority::P3,
    }
}

fn build_fix_hint(issue_type: &IssueType, path: &FieldPath) -> Option<FixHint> {
    let action = match issue_type {
        IssueType::Missing => FixAction::Add,
        IssueType::Conflict => FixAction::Replace,
        IssueType::Redundancy => FixAction::Remove,
        IssueType::OverSpecification => FixAction::Normalize,
        IssueType::UnderSpecification => FixAction::Replace,
    };

    Some(FixHint {
        action,
        target: path.clone(),
    })
}

fn issue_id(path: &FieldPath, reason: &IssueReason, change: &FieldChange) -> IssueId {
    IssueId {
        value: format!(
            "{}::{reason:?}::{:?}",
            path.segments.join("."),
            change.change_type
        ),
    }
}

fn violates_rule(change: &FieldChange, stage: &Stage) -> bool {
    is_required_field(stage, &change.path)
        || stage_root(stage) == change.path.segments.first().map(String::as_str)
}

fn is_under_specified(
    change: &FieldChange,
    stage: &Stage,
    path: &FieldPath,
    after_canonical: &Value,
) -> bool {
    if is_required_field(stage, path)
        && matches!(
            change.change_type,
            ChangeType::Modified | ChangeType::Added | ChangeType::Removed
        )
        && change.after.as_ref().is_none_or(is_semantically_empty)
    {
        return true;
    }

    let Some(required_parent) = required_parent_collection(stage, path) else {
        return false;
    };

    value_at_path(after_canonical, &required_parent).is_some_and(is_semantically_empty)
}

fn is_duplicate_structure(
    path: &FieldPath,
    after_value: Option<&Value>,
    after_canonical: &Value,
) -> bool {
    let Some(after_value) = after_value else {
        return false;
    };
    let Some(parent_path) = parent_path(path) else {
        return false;
    };
    let Some(Value::Array(values)) = value_at_path(after_canonical, &parent_path) else {
        return false;
    };

    values.iter().filter(|value| *value == after_value).count() > 1
}

fn value_at_path<'a>(value: &'a Value, path: &FieldPath) -> Option<&'a Value> {
    let mut current = value;
    for segment in &path.segments {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => {
                if let Ok(index) = segment.parse::<usize>() {
                    items.get(index)?
                } else {
                    resolve_array_segment(items, segment)?
                }
            }
            _ => return None,
        };
    }
    Some(current)
}

fn resolve_array_segment<'a>(items: &'a [Value], segment: &str) -> Option<&'a Value> {
    let (base, ordinal) = split_segment_ordinal(segment);
    let mut seen = 0usize;
    for (index, item) in items.iter().enumerate() {
        if array_segment_key(item, index) == base {
            if seen == ordinal {
                return Some(item);
            }
            seen += 1;
        }
    }
    None
}

fn split_segment_ordinal(segment: &str) -> (&str, usize) {
    if let Some((base, suffix)) = segment.rsplit_once('#') {
        if let Ok(index) = suffix.parse::<usize>() {
            return (base, index);
        }
    }
    (segment, 0)
}

fn array_segment_key(value: &Value, index: usize) -> String {
    match value {
        Value::Object(map) => map
            .get("name")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| index.to_string()),
        Value::String(text) => text.clone(),
        _ => index.to_string(),
    }
}

fn parent_path(path: &FieldPath) -> Option<FieldPath> {
    if path.segments.is_empty() {
        return None;
    }
    let mut segments = path.segments.clone();
    segments.pop();
    Some(FieldPath { segments })
}

fn required_parent_collection(stage: &Stage, path: &FieldPath) -> Option<FieldPath> {
    let segments = path.segments.iter().map(String::as_str).collect::<Vec<_>>();
    match segments.as_slice() {
        ["function", "functions", ..] if *stage == Stage::Function => Some(FieldPath {
            segments: vec!["function".to_string(), "functions".to_string()],
        }),
        ["execution", "steps", ..] if *stage == Stage::Execution => Some(FieldPath {
            segments: vec!["execution".to_string(), "steps".to_string()],
        }),
        _ => None,
    }
}

fn is_required_field(stage: &Stage, path: &FieldPath) -> bool {
    let segments = path.segments.iter().map(String::as_str).collect::<Vec<_>>();
    match segments.as_slice() {
        [root] => Some(*root) == stage_root(stage),
        ["context", "target_user"] | ["context", "use_case"] => *stage == Stage::Context,
        ["function", "functions"] => *stage == Stage::Function,
        ["execution", "steps"] => *stage == Stage::Execution,
        _ => false,
    }
}

fn stage_root(stage: &Stage) -> Option<&'static str> {
    match stage {
        Stage::Context => Some("context"),
        Stage::Function => Some("function"),
        Stage::Architecture => Some("architecture"),
        Stage::Interface => Some("interface"),
        Stage::Data => Some("data"),
        Stage::Execution => Some("execution"),
    }
}

fn is_semantically_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(entries) => entries.is_empty() || entries.values().all(is_semantically_empty),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContextSpec, DesignDocument, DiffSummary, ExecutionSpec, FunctionSpec, Metadata,
        SemanticDiff, VersionId, VersionStatus, diff,
    };

    fn version(id: u64, stage: Stage, design: DesignDocument) -> DesignVersion {
        DesignVersion {
            id: VersionId {
                seq: id,
                hash: id.to_string(),
            },
            parent: None,
            stage,
            status: VersionStatus::Draft,
            design,
            created_at: 0,
            is_duplicate: false,
        }
    }

    fn context_design(target_user: Option<&str>, use_case: Option<&str>) -> DesignDocument {
        DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: target_user.map(str::to_string),
                use_case: use_case.map(str::to_string),
                environment: None,
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        }
    }

    fn function_design(functions: Vec<&str>) -> DesignDocument {
        DesignDocument {
            stage: Stage::Function,
            context: None,
            function: Some(FunctionSpec {
                functions: functions.into_iter().map(str::to_string).collect(),
            }),
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        }
    }

    fn execution_design(steps: Vec<&str>) -> DesignDocument {
        DesignDocument {
            stage: Stage::Execution,
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: Some(ExecutionSpec {
                steps: steps.into_iter().map(str::to_string).collect(),
            }),
            metadata: Metadata::default(),
        }
    }

    fn detect(before: DesignVersion, after: DesignVersion) -> IssueResult {
        let diff = diff(&normalize(&before.design), &normalize(&after.design)).expect("diff");
        detect_issues(IssueInput {
            before,
            after,
            diff,
        })
        .expect("issues")
    }

    #[test]
    fn detects_missing_required_field() {
        let before = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let after = version(
            2,
            Stage::Context,
            DesignDocument {
                stage: Stage::Context,
                context: None,
                function: None,
                architecture: None,
                interface: None,
                data: None,
                execution: None,
                metadata: Metadata::default(),
            },
        );

        let result = detect(before, after);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_type, IssueType::Missing);
        assert_eq!(result.issues[0].severity, Severity::Critical);
        assert_eq!(result.issues[0].priority, Priority::P0);
    }

    #[test]
    fn detects_conflict_from_value_mismatch() {
        let before = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let after = version(
            2,
            Stage::Context,
            context_design(Some("auditor"), Some("review")),
        );

        let result = detect(before, after);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_type, IssueType::Conflict);
        assert_eq!(result.issues[0].reason, IssueReason::ValueConflict);
        assert_eq!(result.issues[0].priority, Priority::P0);
    }

    #[test]
    fn detects_over_specification_for_non_required_addition() {
        let before = version(1, Stage::Function, function_design(vec!["search"]));
        let after = version(
            2,
            Stage::Function,
            function_design(vec!["search", "approve"]),
        );

        let result = detect(before, after);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_type, IssueType::OverSpecification);
        assert_eq!(result.issues[0].severity, Severity::Medium);
        assert_eq!(result.issues[0].priority, Priority::P2);
    }

    #[test]
    fn detects_redundancy_for_duplicate_structure() {
        let before = version(1, Stage::Function, function_design(vec!["search"]));
        let after = version(
            2,
            Stage::Function,
            function_design(vec!["search", "search"]),
        );

        let result = detect(before, after);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_type, IssueType::Redundancy);
        assert_eq!(result.issues[0].severity, Severity::Low);
        assert_eq!(result.issues[0].priority, Priority::P3);
    }

    #[test]
    fn detects_under_specification_for_empty_required_field() {
        let before = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let after = version(
            2,
            Stage::Context,
            context_design(Some("operator"), Some("")),
        );

        let result = detect(before, after);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_type, IssueType::UnderSpecification);
        assert_eq!(result.issues[0].severity, Severity::Medium);
    }

    #[test]
    fn filters_noise_for_order_difference_only() {
        let before = serde_json::json!({
            "stage": "Function",
            "function": { "functions": ["search"] },
            "metadata": { "tags": ["beta", "core"], "annotations": [] }
        });
        let after = serde_json::json!({
            "stage": "Function",
            "function": { "functions": ["search"] },
            "metadata": { "tags": ["core", "beta"], "annotations": [] }
        });
        let diff = diff(&before, &after).expect("diff");
        let before_version = version(1, Stage::Function, function_design(vec!["search"]));
        let after_version = version(2, Stage::Function, function_design(vec!["search"]));

        let result = detect_issues(IssueInput {
            before: before_version,
            after: after_version,
            diff,
        })
        .expect("issues");

        assert!(result.issues.is_empty());
    }

    #[test]
    fn detects_missing_steps_as_under_specification() {
        let before = version(1, Stage::Execution, execution_design(vec!["validate"]));
        let after = version(2, Stage::Execution, execution_design(vec![]));

        let result = detect(before, after);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_type, IssueType::UnderSpecification);
        assert!(result.issues[0].fix_hint.is_some());
    }

    #[test]
    fn sorts_critical_issues_first_and_assigns_stable_order() {
        let before = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let after = version(
            2,
            Stage::Context,
            context_design(Some("auditor"), Some("review")),
        );
        let diff = DiffResult {
            changes: vec![
                FieldChange {
                    path: FieldPath {
                        segments: vec!["context".to_string()],
                    },
                    before: Some(serde_json::json!({"target_user":"operator","use_case":"review"})),
                    after: None,
                    change_type: ChangeType::Removed,
                },
                FieldChange {
                    path: FieldPath {
                        segments: vec!["context".to_string(), "target_user".to_string()],
                    },
                    before: Some(Value::String("operator".to_string())),
                    after: Some(Value::String("auditor".to_string())),
                    change_type: ChangeType::Modified,
                },
            ],
            summary: DiffSummary {
                added: 0,
                removed: 1,
                modified: 1,
                net_complexity: -1,
            },
            semantic: SemanticDiff {
                is_equivalent: false,
                reason: SemanticReason::ValueMismatch,
            },
            impact: Impact::Regressed,
            impact_reason: ImpactReason::IncreasedComplexity,
        };

        let result = detect_issues(IssueInput {
            before: before.clone(),
            after: after.clone(),
            diff: diff.clone(),
        })
        .expect("issues");
        let repeated = detect_issues(IssueInput {
            before,
            after,
            diff,
        })
        .expect("issues");

        assert_eq!(result.issues[0].issue_type, IssueType::Missing);
        assert_eq!(result.issues[0].order, 1);
        assert_eq!(result.issues, repeated.issues);
    }

    #[test]
    fn missing_issue_blocks_descendant_issues() {
        let before = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let after = version(
            2,
            Stage::Context,
            context_design(Some("auditor"), Some("review")),
        );
        let diff = DiffResult {
            changes: vec![
                FieldChange {
                    path: FieldPath {
                        segments: vec!["context".to_string()],
                    },
                    before: Some(serde_json::json!({"target_user":"operator","use_case":"review"})),
                    after: None,
                    change_type: ChangeType::Removed,
                },
                FieldChange {
                    path: FieldPath {
                        segments: vec!["context".to_string(), "target_user".to_string()],
                    },
                    before: Some(Value::String("operator".to_string())),
                    after: Some(Value::String("auditor".to_string())),
                    change_type: ChangeType::Modified,
                },
            ],
            summary: DiffSummary {
                added: 0,
                removed: 1,
                modified: 1,
                net_complexity: -1,
            },
            semantic: SemanticDiff {
                is_equivalent: false,
                reason: SemanticReason::ValueMismatch,
            },
            impact: Impact::Regressed,
            impact_reason: ImpactReason::IncreasedComplexity,
        };

        let result = detect_issues(IssueInput {
            before,
            after,
            diff,
        })
        .expect("issues");

        assert_eq!(result.issues.len(), 2);
        let missing = result
            .issues
            .iter()
            .find(|issue| issue.issue_type == IssueType::Missing)
            .expect("missing issue");
        let conflict = result
            .issues
            .iter()
            .find(|issue| issue.issue_type == IssueType::Conflict)
            .expect("conflict issue");

        assert_eq!(conflict.blocks, vec![missing.id.clone()]);
    }

    #[test]
    fn summary_counts_are_correct_after_sorting() {
        let before = version(1, Stage::Function, function_design(vec!["search"]));
        let after = version(
            2,
            Stage::Function,
            function_design(vec!["search", "search"]),
        );

        let result = detect(before, after);
        assert_eq!(result.summary.total, 1);
        assert_eq!(result.summary.low, 1);
        assert_eq!(result.summary.critical, 0);
        assert_eq!(result.summary.high, 0);
        assert_eq!(result.summary.medium, 0);
    }
}
