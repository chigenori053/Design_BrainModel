use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CanonicalDesign, DesignVersion, Stage, normalize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffResult {
    pub changes: Vec<FieldChange>,
    pub summary: DiffSummary,
    pub semantic: SemanticDiff,
    pub impact: Impact,
    pub impact_reason: ImpactReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    pub path: FieldPath,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub change_type: ChangeType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Removed,
    Modified,
    Moved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FieldPath {
    pub segments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub net_complexity: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub is_equivalent: bool,
    pub reason: SemanticReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Impact {
    Improved,
    Regressed,
    Neutral,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticReason {
    ExactMatch,
    OrderDifference,
    ValueMismatch,
    MissingToEmpty,
    EmptyToMissing,
    TypeMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactReason {
    ReducedComplexity,
    IncreasedComplexity,
    OrderOnlyChange,
    NoMeaningfulChange,
    CompletenessGain,
    MixedChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffError {
    StageMismatch,
    InvalidCanonical,
}

pub fn diff(before: &CanonicalDesign, after: &CanonicalDesign) -> Result<DiffResult, DiffError> {
    validate_stage_match(before, after)?;
    let changes = diff_structural(before, after)?;
    let summary = summarize(&changes);
    let semantic = diff_semantic(before, after)?;
    let (impact, impact_reason) = compute_impact(&summary, &semantic);
    Ok(DiffResult {
        changes,
        summary,
        semantic,
        impact,
        impact_reason,
    })
}

pub fn diff_versions(
    before: &DesignVersion,
    after: &DesignVersion,
) -> Result<DiffResult, DiffError> {
    if before.stage != after.stage {
        return Err(DiffError::StageMismatch);
    }
    let before_canonical = normalize(&before.design);
    let after_canonical = normalize(&after.design);
    diff(&before_canonical, &after_canonical)
}

pub fn diff_structural(
    before: &CanonicalDesign,
    after: &CanonicalDesign,
) -> Result<Vec<FieldChange>, DiffError> {
    validate_stage_match(before, after)?;
    let mut changes = Vec::new();
    diff_value(FieldPath::default(), before, after, &mut changes)?;
    Ok(changes)
}

pub fn diff_semantic(
    before: &CanonicalDesign,
    after: &CanonicalDesign,
) -> Result<SemanticDiff, DiffError> {
    validate_stage_match(before, after)?;
    semantic_diff(None, before, after)
}

pub fn compute_impact(summary: &DiffSummary, semantic: &SemanticDiff) -> (Impact, ImpactReason) {
    if semantic.is_equivalent {
        return match semantic.reason {
            SemanticReason::OrderDifference => (Impact::Neutral, ImpactReason::OrderOnlyChange),
            _ => (Impact::Neutral, ImpactReason::NoMeaningfulChange),
        };
    }

    if summary.removed > summary.added {
        (Impact::Improved, ImpactReason::ReducedComplexity)
    } else if summary.added > summary.removed || summary.modified > 0 {
        (Impact::Regressed, ImpactReason::IncreasedComplexity)
    } else {
        (Impact::Neutral, ImpactReason::MixedChange)
    }
}

fn validate_stage_match(before: &Value, after: &Value) -> Result<Stage, DiffError> {
    let before_stage = extract_stage(before)?;
    let after_stage = extract_stage(after)?;
    if before_stage == after_stage {
        Ok(before_stage)
    } else {
        Err(DiffError::StageMismatch)
    }
}

fn extract_stage(value: &Value) -> Result<Stage, DiffError> {
    let object = value.as_object().ok_or(DiffError::InvalidCanonical)?;
    let stage = object
        .get("stage")
        .and_then(Value::as_str)
        .ok_or(DiffError::InvalidCanonical)?;
    parse_stage(stage).ok_or(DiffError::InvalidCanonical)
}

fn parse_stage(stage: &str) -> Option<Stage> {
    match stage {
        "Context" => Some(Stage::Context),
        "Function" => Some(Stage::Function),
        "Architecture" => Some(Stage::Architecture),
        "Interface" => Some(Stage::Interface),
        "Data" => Some(Stage::Data),
        "Execution" => Some(Stage::Execution),
        _ => None,
    }
}

fn diff_value(
    path: FieldPath,
    before: &Value,
    after: &Value,
    changes: &mut Vec<FieldChange>,
) -> Result<(), DiffError> {
    match (before, after) {
        (Value::Object(before_obj), Value::Object(after_obj)) => {
            let keys = before_obj
                .keys()
                .chain(after_obj.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                let next_path = path.child(key.clone());
                match (before_obj.get(&key), after_obj.get(&key)) {
                    (None, Some(after_value)) => changes.push(FieldChange {
                        path: next_path,
                        before: None,
                        after: Some(after_value.clone()),
                        change_type: ChangeType::Added,
                    }),
                    (Some(before_value), None) => changes.push(FieldChange {
                        path: next_path,
                        before: Some(before_value.clone()),
                        after: None,
                        change_type: ChangeType::Removed,
                    }),
                    (Some(before_value), Some(after_value)) => {
                        diff_value(next_path, before_value, after_value, changes)?;
                    }
                    (None, None) => {}
                }
            }
            Ok(())
        }
        (Value::Array(before_items), Value::Array(after_items)) => {
            diff_array(path, before_items, after_items, changes)
        }
        _ if before == after => Ok(()),
        _ => {
            changes.push(FieldChange {
                path,
                before: Some(before.clone()),
                after: Some(after.clone()),
                change_type: ChangeType::Modified,
            });
            Ok(())
        }
    }
}

fn diff_array(
    path: FieldPath,
    before: &[Value],
    after: &[Value],
    changes: &mut Vec<FieldChange>,
) -> Result<(), DiffError> {
    if is_order_insensitive(path.last().unwrap_or_default()) {
        let before_map = keyed_values(before);
        let after_map = keyed_values(after);
        let keys = before_map
            .keys()
            .chain(after_map.keys())
            .cloned()
            .collect::<BTreeSet<_>>();

        for key in keys {
            let next_path = path.child(key.clone());
            match (before_map.get(&key), after_map.get(&key)) {
                (None, Some(after_value)) => changes.push(FieldChange {
                    path: next_path,
                    before: None,
                    after: Some((*after_value).clone()),
                    change_type: ChangeType::Added,
                }),
                (Some(before_value), None) => changes.push(FieldChange {
                    path: next_path,
                    before: Some((*before_value).clone()),
                    after: None,
                    change_type: ChangeType::Removed,
                }),
                (Some(before_value), Some(after_value)) => {
                    diff_value(next_path, before_value, after_value, changes)?;
                }
                (None, None) => {}
            }
        }
        Ok(())
    } else {
        if before.len() == after.len() && normalized_multiset(before) == normalized_multiset(after)
        {
            let combined_counts = combined_array_label_counts(before, after);
            for index in 0..before.len() {
                if before[index] != after[index] {
                    let next_path =
                        path.child(array_path_segment(&after[index], index, &combined_counts));
                    changes.push(FieldChange {
                        path: next_path,
                        before: Some(before[index].clone()),
                        after: Some(after[index].clone()),
                        change_type: ChangeType::Moved,
                    });
                }
            }
            return Ok(());
        }

        let combined_counts = combined_array_label_counts(before, after);
        let max_len = before.len().max(after.len());
        for index in 0..max_len {
            let next_path = path.child(array_path_segment(
                after
                    .get(index)
                    .or_else(|| before.get(index))
                    .expect("index within bounds"),
                index,
                &combined_counts,
            ));
            match (before.get(index), after.get(index)) {
                (None, Some(after_value)) => changes.push(FieldChange {
                    path: next_path,
                    before: None,
                    after: Some(after_value.clone()),
                    change_type: ChangeType::Added,
                }),
                (Some(before_value), None) => changes.push(FieldChange {
                    path: next_path,
                    before: Some(before_value.clone()),
                    after: None,
                    change_type: ChangeType::Removed,
                }),
                (Some(before_value), Some(after_value)) => {
                    diff_value(next_path, before_value, after_value, changes)?;
                }
                (None, None) => {}
            }
        }
        Ok(())
    }
}

fn semantic_diff(
    field_name: Option<&str>,
    before: &Value,
    after: &Value,
) -> Result<SemanticDiff, DiffError> {
    match (before, after) {
        (Value::Object(before_obj), Value::Object(after_obj)) => {
            let keys = before_obj
                .keys()
                .chain(after_obj.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            let mut propagated_reason = SemanticReason::ExactMatch;
            for key in keys {
                match (before_obj.get(&key), after_obj.get(&key)) {
                    (Some(before_value), Some(after_value)) => {
                        let nested = semantic_diff(Some(&key), before_value, after_value)?;
                        if !nested.is_equivalent {
                            return Ok(nested);
                        }
                        if nested.reason != SemanticReason::ExactMatch {
                            propagated_reason = nested.reason;
                        }
                    }
                    (None, None) => {}
                    (None, Some(after_value)) if is_semantically_empty(after_value) => {
                        return Ok(SemanticDiff {
                            is_equivalent: false,
                            reason: SemanticReason::MissingToEmpty,
                        });
                    }
                    (Some(before_value), None) if is_semantically_empty(before_value) => {
                        return Ok(SemanticDiff {
                            is_equivalent: false,
                            reason: SemanticReason::EmptyToMissing,
                        });
                    }
                    _ => {
                        return Ok(SemanticDiff {
                            is_equivalent: false,
                            reason: SemanticReason::ValueMismatch,
                        });
                    }
                }
            }
            Ok(SemanticDiff {
                is_equivalent: true,
                reason: propagated_reason,
            })
        }
        (Value::Array(before_items), Value::Array(after_items)) => {
            if is_order_insensitive(field_name.unwrap_or_default()) {
                if before_items == after_items {
                    Ok(SemanticDiff {
                        is_equivalent: true,
                        reason: SemanticReason::ExactMatch,
                    })
                } else if normalized_multiset(before_items) == normalized_multiset(after_items) {
                    Ok(SemanticDiff {
                        is_equivalent: true,
                        reason: SemanticReason::OrderDifference,
                    })
                } else {
                    Ok(SemanticDiff {
                        is_equivalent: false,
                        reason: SemanticReason::ValueMismatch,
                    })
                }
            } else {
                if before_items.len() != after_items.len() {
                    return Ok(SemanticDiff {
                        is_equivalent: false,
                        reason: SemanticReason::ValueMismatch,
                    });
                }
                for (before_item, after_item) in before_items.iter().zip(after_items.iter()) {
                    let nested = semantic_diff(None, before_item, after_item)?;
                    if !nested.is_equivalent {
                        return Ok(nested);
                    }
                    if nested.reason != SemanticReason::ExactMatch {
                        return Ok(nested);
                    }
                }
                Ok(SemanticDiff {
                    is_equivalent: true,
                    reason: SemanticReason::ExactMatch,
                })
            }
        }
        (Value::Null, _) | (_, Value::Null) => Ok(SemanticDiff {
            is_equivalent: false,
            reason: SemanticReason::ValueMismatch,
        }),
        _ if before == after => Ok(SemanticDiff {
            is_equivalent: true,
            reason: SemanticReason::ExactMatch,
        }),
        (Value::Array(_), _)
        | (_, Value::Array(_))
        | (Value::Object(_), _)
        | (_, Value::Object(_)) => Ok(SemanticDiff {
            is_equivalent: false,
            reason: SemanticReason::TypeMismatch,
        }),
        _ => Ok(SemanticDiff {
            is_equivalent: false,
            reason: SemanticReason::ValueMismatch,
        }),
    }
}

fn summarize(changes: &[FieldChange]) -> DiffSummary {
    let mut summary = DiffSummary::default();
    for change in changes {
        match change.change_type {
            ChangeType::Added => summary.added += 1,
            ChangeType::Removed => summary.removed += 1,
            ChangeType::Modified => summary.modified += 1,
            ChangeType::Moved => summary.modified += 1,
        }
    }
    summary.net_complexity = summary.added as i32 - summary.removed as i32;
    summary
}

fn keyed_values(values: &[Value]) -> BTreeMap<String, &Value> {
    let mut counts = BTreeMap::new();
    let mut keyed = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        let base = array_base_key(value, index);
        let entry = counts.entry(base.clone()).or_insert(0usize);
        let key = if *entry == 0 {
            base.clone()
        } else {
            format!("{base}#{entry}")
        };
        *entry += 1;
        keyed.insert(key, value);
    }
    keyed
}

fn array_base_key(value: &Value, index: usize) -> String {
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

fn combined_array_label_counts(before: &[Value], after: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for (index, value) in before.iter().enumerate() {
        *counts.entry(array_base_key(value, index)).or_insert(0) += 1;
    }
    for (index, value) in after.iter().enumerate() {
        *counts.entry(array_base_key(value, index)).or_insert(0) += 1;
    }
    counts
}

fn array_path_segment(value: &Value, index: usize, counts: &BTreeMap<String, usize>) -> String {
    let base = array_base_key(value, index);
    if counts.get(&base).copied().unwrap_or_default() == 1
        && !matches!(value, Value::Number(_) | Value::Bool(_) | Value::Null)
    {
        base
    } else {
        index.to_string()
    }
}

fn normalized_multiset(values: &[Value]) -> BTreeMap<String, usize> {
    let mut multiset = BTreeMap::new();
    for value in values {
        let key = serde_json::to_string(value).unwrap_or_default();
        *multiset.entry(key).or_insert(0) += 1;
    }
    multiset
}

fn is_order_insensitive(field_name: &str) -> bool {
    matches!(field_name, "requirements" | "tags")
}

fn is_semantically_empty(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.is_empty(),
        Value::Object(entries) => entries.is_empty() || entries.values().all(is_semantically_empty),
        Value::String(text) => text.is_empty(),
        _ => false,
    }
}

impl FieldPath {
    fn child(&self, segment: String) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment);
        Self { segments }
    }

    fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContextSpec, DesignDocument, ExecutionSpec, FunctionSpec, Metadata, VersionId,
        VersionStatus, create_version, init_history,
    };

    fn function_design(functions: Vec<&str>, tags: Vec<&str>) -> DesignDocument {
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
            metadata: Metadata {
                tags: tags.into_iter().map(str::to_string).collect(),
                annotations: Vec::new(),
            },
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

    #[test]
    fn detects_added_removed_and_modified_fields() {
        let before = normalize(&function_design(vec!["search"], vec!["core"]));
        let after = normalize(&function_design(vec!["search", "approve"], vec!["edge"]));
        let result = diff(&before, &after).expect("diff");

        assert!(result.summary.added >= 1 || result.summary.modified >= 1);
        assert!(result.changes.iter().any(|change| {
            change.path.segments == vec!["function", "functions", "1"]
                || change.path.segments == vec!["function", "functions", "approve"]
        }));
        assert!(result.changes.iter().any(|change| change.path.segments
            == vec!["metadata", "tags", "core"]
            || change.path.segments == vec!["metadata", "tags", "edge"]));
    }

    #[test]
    fn order_insensitive_arrays_are_semantically_equivalent() {
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
        let semantic = diff_semantic(&before, &after).expect("semantic diff");
        let result = diff(&before, &after).expect("diff");

        assert!(semantic.is_equivalent);
        assert_eq!(semantic.reason, SemanticReason::OrderDifference);
        assert!(result.changes.is_empty());
        assert_eq!(result.impact, Impact::Neutral);
        assert_eq!(result.impact_reason, ImpactReason::OrderOnlyChange);
    }

    #[test]
    fn order_sensitive_arrays_produce_structural_diff() {
        let before = normalize(&execution_design(vec!["validate", "persist"]));
        let after = normalize(&execution_design(vec!["persist", "validate"]));
        let result = diff(&before, &after).expect("diff");

        assert_eq!(result.summary.modified, 2);
        assert!(
            result
                .changes
                .iter()
                .all(|change| change.change_type == ChangeType::Moved)
        );
        assert!(!result.semantic.is_equivalent);
    }

    #[test]
    fn stage_mismatch_is_rejected() {
        let before = normalize(&function_design(vec!["search"], vec![]));
        let after = normalize(&execution_design(vec!["validate"]));

        assert_eq!(diff(&before, &after), Err(DiffError::StageMismatch));
    }

    #[test]
    fn empty_vs_missing_is_detected() {
        let before = normalize(&function_design(vec![], vec![]));
        let after = normalize(&DesignDocument {
            stage: Stage::Function,
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        });
        let result = diff(&before, &after).expect("diff");

        assert!(
            result
                .changes
                .iter()
                .any(|change| change.path.segments == vec!["function"])
        );
        assert_eq!(result.semantic.reason, SemanticReason::EmptyToMissing);
    }

    #[test]
    fn type_difference_is_modified() {
        let before = serde_json::json!({"stage":"Context","context":{"target_user":"a"},"metadata":{"tags":[],"annotations":[]}});
        let after = serde_json::json!({"stage":"Context","context":"a","metadata":{"tags":[],"annotations":[]}});
        let result = diff(&before, &after).expect("diff");

        assert_eq!(result.summary.modified, 1);
        assert_eq!(result.changes[0].change_type, ChangeType::Modified);
        assert_eq!(result.semantic.reason, SemanticReason::TypeMismatch);
    }

    #[test]
    fn deep_nested_paths_are_reported() {
        let before = normalize(&DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("operator".to_string()),
                use_case: Some("review".to_string()),
                environment: Some("prod".to_string()),
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        });
        let after = normalize(&DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("auditor".to_string()),
                use_case: Some("review".to_string()),
                environment: Some("prod".to_string()),
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        });
        let result = diff(&before, &after).expect("diff");

        assert!(
            result
                .changes
                .iter()
                .any(|change| change.path.segments == vec!["context", "target_user"])
        );
    }

    #[test]
    fn diff_versions_checks_stage_consistency() {
        let base = DesignVersion {
            id: VersionId {
                seq: 1,
                hash: "a".to_string(),
            },
            parent: None,
            stage: Stage::Function,
            status: VersionStatus::Draft,
            design: function_design(vec!["search"], vec![]),
            created_at: 0,
            is_duplicate: false,
        };
        let mismatched = DesignVersion {
            stage: Stage::Execution,
            design: execution_design(vec!["validate"]),
            ..base.clone()
        };

        assert_eq!(
            diff_versions(&base, &mismatched),
            Err(DiffError::StageMismatch)
        );
    }

    #[test]
    fn compute_impact_returns_neutral_for_semantic_equivalence() {
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
        let result = diff(&before, &after).expect("diff");

        assert_eq!(result.impact, Impact::Neutral);
        assert_eq!(result.impact_reason, ImpactReason::OrderOnlyChange);
    }

    #[test]
    fn diff_versions_returns_result_for_valid_versions() {
        let initial = function_design(vec!["search"], vec![]);
        let mut history = init_history(DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("operator".to_string()),
                use_case: None,
                environment: None,
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        });
        let parent = history.head.clone();
        let before =
            create_version(&mut history, Some(parent), Stage::Function, initial).expect("before");
        let parent = history.head.clone();
        let after = create_version(
            &mut history,
            Some(parent),
            Stage::Function,
            function_design(vec!["search", "approve"], vec![]),
        )
        .expect("after");

        let result = diff_versions(&before, &after).expect("diff versions");
        assert!(!result.changes.is_empty());
    }

    #[test]
    fn duplicate_named_array_paths_fall_back_to_indexes() {
        let before = serde_json::json!({
            "stage": "Architecture",
            "architecture": {
                "modules": [
                    { "name": "core", "role": "domain" },
                    { "name": "core", "role": "api" }
                ]
            },
            "metadata": { "tags": [], "annotations": [] }
        });
        let after = serde_json::json!({
            "stage": "Architecture",
            "architecture": {
                "modules": [
                    { "name": "core", "role": "service" },
                    { "name": "core", "role": "api" }
                ]
            },
            "metadata": { "tags": [], "annotations": [] }
        });

        let result = diff(&before, &after).expect("diff");
        let paths = result
            .changes
            .iter()
            .map(|change| change.path.segments.join("."))
            .collect::<Vec<_>>();

        assert!(
            paths
                .iter()
                .any(|path| path == "architecture.modules.0.role")
        );
        assert_eq!(paths.len(), paths.iter().collect::<BTreeSet<_>>().len());
    }

    #[test]
    fn missing_to_empty_has_distinct_semantic_reason() {
        let before = normalize(&DesignDocument {
            stage: Stage::Function,
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        });
        let after = normalize(&function_design(vec![], vec![]));

        let result = diff(&before, &after).expect("diff");
        assert_eq!(result.semantic.reason, SemanticReason::MissingToEmpty);
    }

    #[test]
    fn reduced_complexity_is_improved() {
        let before = normalize(&execution_design(vec!["validate", "persist", "notify"]));
        let after = normalize(&execution_design(vec!["validate"]));

        let result = diff(&before, &after).expect("diff");
        assert_eq!(result.impact, Impact::Improved);
        assert_eq!(result.impact_reason, ImpactReason::ReducedComplexity);
    }

    #[test]
    fn diff_identity_is_empty() {
        let design = normalize(&function_design(vec!["search"], vec!["core"]));
        let result = diff(&design, &design).expect("diff");

        assert!(result.changes.is_empty());
        assert_eq!(result.semantic.reason, SemanticReason::ExactMatch);
    }
}
