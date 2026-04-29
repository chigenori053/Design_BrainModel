use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ContextSpec, DesignDocument, DesignHistory, DesignVersion, ExecutionSpec, FieldPath, FixAction,
    Issue, IssueId, IssueInput, IssueType, Severity, Stage, create_version, detect_issues,
    diff_versions, get_version,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixInput {
    pub history: DesignHistory,
    pub current: DesignVersion,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixResult {
    pub applied: Option<AppliedFix>,
    pub next_version: DesignVersion,
    pub report: FixReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedFix {
    pub issue_id: IssueId,
    pub action: FixAction,
    pub path: FieldPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixReport {
    pub success: bool,
    pub reason: Option<FixFailureReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixFailureReason {
    Blocked,
    InvalidPath,
    TypeMismatch,
    ConstraintViolation,
    NoEffect,
}

pub fn apply_next_fix(input: FixInput) -> FixResult {
    let selected = select_issue(&input.issues);
    let Some(issue) = selected else {
        return failure_result(input.current, None, FixFailureReason::Blocked);
    };

    let action = action_for_issue(issue);
    let applied = AppliedFix {
        issue_id: issue.id.clone(),
        action: action.clone(),
        path: issue.path.clone(),
    };

    if let Err(reason) = pre_check(&input.current.design, issue, &action) {
        return failure_result(input.current, Some(applied), reason);
    }

    let mut next_design = input.current.design.clone();
    if let Err(reason) = apply_fix(&mut next_design, issue, &action) {
        return failure_result(input.current, Some(applied), reason);
    }

    if next_design == input.current.design {
        return failure_result(input.current, Some(applied), FixFailureReason::NoEffect);
    }

    let mut history = materialize_current_head(input.history, &input.current);
    let baseline = input
        .current
        .parent
        .as_ref()
        .and_then(|id| get_version(&history, id))
        .cloned()
        .unwrap_or_else(|| input.current.clone());
    let next_version = match create_version(
        &mut history,
        Some(input.current.id.clone()),
        input.current.stage.clone(),
        next_design,
    ) {
        Ok(version) => version,
        Err(_) => {
            return failure_result(
                input.current,
                Some(applied),
                FixFailureReason::ConstraintViolation,
            );
        }
    };

    if let Err(reason) = post_check(&baseline, &next_version, issue, &input.issues) {
        return failure_result(input.current, Some(applied), reason);
    }

    FixResult {
        applied: Some(applied),
        next_version,
        report: FixReport {
            success: true,
            reason: None,
        },
    }
}

fn materialize_current_head(mut history: DesignHistory, current: &DesignVersion) -> DesignHistory {
    if history.head == current.id {
        return history;
    }

    if history
        .versions
        .iter()
        .all(|version| version.id != current.id)
    {
        history.versions.push(current.clone());
    }
    history.head = current.id.clone();
    history.next_seq = history
        .versions
        .iter()
        .map(|version| version.id.seq)
        .max()
        .unwrap_or(current.id.seq)
        + 1;
    history
}

fn select_issue(issues: &[Issue]) -> Option<&Issue> {
    issues
        .iter()
        .find(|issue| issue.blocks.is_empty())
        .or_else(|| issues.first())
}

fn action_for_issue(issue: &Issue) -> FixAction {
    match issue.issue_type {
        IssueType::Missing => FixAction::Add,
        IssueType::Conflict => FixAction::Replace,
        IssueType::Redundancy => FixAction::Remove,
        IssueType::OverSpecification => FixAction::Remove,
        IssueType::UnderSpecification => FixAction::Normalize,
    }
}

fn pre_check(
    design: &DesignDocument,
    issue: &Issue,
    action: &FixAction,
) -> Result<(), FixFailureReason> {
    if design.stage != issue_stage_root(issue).0 {
        return Err(FixFailureReason::ConstraintViolation);
    }

    match action {
        FixAction::Add => pre_check_add(design, &issue.path),
        FixAction::Remove => pre_check_remove(design, issue),
        FixAction::Replace | FixAction::Normalize => {
            pre_check_replace_or_normalize(design, &issue.path)
        }
    }
}

fn pre_check_add(design: &DesignDocument, path: &FieldPath) -> Result<(), FixFailureReason> {
    let segments = path_segments(path);
    match segments.as_slice() {
        [root] if *root == "context" && design.stage == Stage::Context => Ok(()),
        [root] if *root == "function" && design.stage == Stage::Function => Ok(()),
        [root] if *root == "execution" && design.stage == Stage::Execution => Ok(()),
        ["context", "target_user"] | ["context", "use_case"] if design.stage == Stage::Context => {
            Ok(())
        }
        ["function", "functions", ..] | ["function", "functions"]
            if design.stage == Stage::Function =>
        {
            Ok(())
        }
        ["execution", "steps", ..] | ["execution", "steps"] if design.stage == Stage::Execution => {
            Ok(())
        }
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn pre_check_remove(design: &DesignDocument, issue: &Issue) -> Result<(), FixFailureReason> {
    if matches!(issue.issue_type, IssueType::Missing) {
        return Err(FixFailureReason::ConstraintViolation);
    }

    let segments = path_segments(&issue.path);
    match segments.as_slice() {
        ["function", "functions", ..]
            if design.stage == Stage::Function && design.function.is_some() =>
        {
            Ok(())
        }
        ["execution", "steps", ..]
            if design.stage == Stage::Execution && design.execution.is_some() =>
        {
            Ok(())
        }
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn pre_check_replace_or_normalize(
    design: &DesignDocument,
    path: &FieldPath,
) -> Result<(), FixFailureReason> {
    let segments = path_segments(path);
    match segments.as_slice() {
        ["context", "target_user"] | ["context", "use_case"] if design.stage == Stage::Context => {
            Ok(())
        }
        ["context"] if design.stage == Stage::Context => Ok(()),
        ["function", "functions"] | ["function", "functions", ..]
            if design.stage == Stage::Function =>
        {
            Ok(())
        }
        ["execution", "steps"] | ["execution", "steps", ..] if design.stage == Stage::Execution => {
            Ok(())
        }
        ["function"] if design.stage == Stage::Function => Ok(()),
        ["execution"] if design.stage == Stage::Execution => Ok(()),
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn apply_fix(
    design: &mut DesignDocument,
    issue: &Issue,
    action: &FixAction,
) -> Result<(), FixFailureReason> {
    match action {
        FixAction::Add => apply_add(design, issue),
        FixAction::Remove => apply_remove(design, issue),
        FixAction::Replace => apply_replace(design, issue),
        FixAction::Normalize => apply_normalize(design, issue),
    }
}

fn apply_add(design: &mut DesignDocument, issue: &Issue) -> Result<(), FixFailureReason> {
    let segments = path_segments(&issue.path);
    match segments.as_slice() {
        [root] if *root == "context" && design.stage == Stage::Context => {
            design.context = Some(default_context());
            Ok(())
        }
        [root] if *root == "function" && design.stage == Stage::Function => {
            design.function = Some(default_function());
            Ok(())
        }
        [root] if *root == "execution" && design.stage == Stage::Execution => {
            design.execution = Some(default_execution());
            Ok(())
        }
        ["context", field] if design.stage == Stage::Context => {
            let context = design.context.get_or_insert_with(default_context);
            let value = default_scalar_for_path(&issue.path, issue);
            match *field {
                "target_user" => context.target_user = Some(value),
                "use_case" => context.use_case = Some(value),
                _ => return Err(FixFailureReason::InvalidPath),
            }
            Ok(())
        }
        ["function", "functions"] | ["function", "functions", ..]
            if design.stage == Stage::Function =>
        {
            let functions = &mut design
                .function
                .get_or_insert_with(default_function)
                .functions;
            if functions.is_empty() {
                functions.push(default_function_name());
            }
            Ok(())
        }
        ["execution", "steps"] | ["execution", "steps", ..] if design.stage == Stage::Execution => {
            let steps = &mut design.execution.get_or_insert_with(default_execution).steps;
            if steps.is_empty() {
                steps.push(default_step_name());
            }
            Ok(())
        }
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn apply_remove(design: &mut DesignDocument, issue: &Issue) -> Result<(), FixFailureReason> {
    let segments = path_segments(&issue.path);
    match segments.as_slice() {
        ["function", "functions", key] if design.stage == Stage::Function => {
            let Some(function) = design.function.as_mut() else {
                return Err(FixFailureReason::InvalidPath);
            };
            remove_vec_item(&mut function.functions, key)
        }
        ["execution", "steps", key] if design.stage == Stage::Execution => {
            let Some(execution) = design.execution.as_mut() else {
                return Err(FixFailureReason::InvalidPath);
            };
            remove_vec_item(&mut execution.steps, key)
        }
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn apply_replace(design: &mut DesignDocument, issue: &Issue) -> Result<(), FixFailureReason> {
    let segments = path_segments(&issue.path);
    match segments.as_slice() {
        ["context", field] if design.stage == Stage::Context => {
            let Some(context) = design.context.as_mut() else {
                return Err(FixFailureReason::InvalidPath);
            };
            let replacement = replacement_value(issue)
                .unwrap_or_else(|| default_scalar_for_path(&issue.path, issue));
            match *field {
                "target_user" => context.target_user = Some(replacement),
                "use_case" => context.use_case = Some(replacement),
                _ => return Err(FixFailureReason::InvalidPath),
            }
            Ok(())
        }
        ["context"] if design.stage == Stage::Context => {
            design.context = Some(default_context());
            Ok(())
        }
        ["function", "functions"] | ["function", "functions", ..]
            if design.stage == Stage::Function =>
        {
            let functions = &mut design
                .function
                .get_or_insert_with(default_function)
                .functions;
            if functions.is_empty() {
                functions.push(default_function_name());
            } else {
                functions.sort();
                functions.dedup();
            }
            Ok(())
        }
        ["execution", "steps"] | ["execution", "steps", ..] if design.stage == Stage::Execution => {
            let steps = &mut design.execution.get_or_insert_with(default_execution).steps;
            if steps.is_empty() {
                steps.push(default_step_name());
            }
            Ok(())
        }
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn apply_normalize(design: &mut DesignDocument, issue: &Issue) -> Result<(), FixFailureReason> {
    let segments = path_segments(&issue.path);
    match segments.as_slice() {
        ["context"] | ["context", ..] if design.stage == Stage::Context => {
            let context = design.context.get_or_insert_with(default_context);
            if context
                .target_user
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            {
                context.target_user = Some("unspecified-user".to_string());
            }
            if context.use_case.as_deref().unwrap_or_default().is_empty() {
                context.use_case = Some("unspecified-use-case".to_string());
            }
            Ok(())
        }
        ["function"] | ["function", "functions"] | ["function", "functions", ..]
            if design.stage == Stage::Function =>
        {
            let functions = &mut design
                .function
                .get_or_insert_with(default_function)
                .functions;
            functions.retain(|value| !value.is_empty());
            if functions.is_empty() {
                functions.push(default_function_name());
            }
            Ok(())
        }
        ["execution"] | ["execution", "steps"] | ["execution", "steps", ..]
            if design.stage == Stage::Execution =>
        {
            let steps = &mut design.execution.get_or_insert_with(default_execution).steps;
            steps.retain(|value| !value.is_empty());
            if steps.is_empty() {
                steps.push(default_step_name());
            }
            Ok(())
        }
        _ => Err(FixFailureReason::InvalidPath),
    }
}

fn post_check(
    current: &DesignVersion,
    next: &DesignVersion,
    applied_issue: &Issue,
    previous_issues: &[Issue],
) -> Result<(), FixFailureReason> {
    let diff = diff_versions(current, next).map_err(|_| FixFailureReason::ConstraintViolation)?;
    let next_issues = detect_issues(IssueInput {
        before: current.clone(),
        after: next.clone(),
        diff,
    })
    .map_err(|_| FixFailureReason::ConstraintViolation)?;

    if next_issues
        .issues
        .iter()
        .any(|next_issue| same_issue(next_issue, applied_issue))
    {
        return Err(FixFailureReason::NoEffect);
    }

    let previous_critical = previous_issues
        .iter()
        .filter(|issue| issue.severity == Severity::Critical)
        .count();
    let next_critical = next_issues
        .issues
        .iter()
        .filter(|issue| issue.severity == Severity::Critical)
        .count();

    if next_critical > previous_critical {
        return Err(FixFailureReason::ConstraintViolation);
    }

    Ok(())
}

fn same_issue(left: &Issue, right: &Issue) -> bool {
    left.path == right.path && left.reason == right.reason
}

fn failure_result(
    current: DesignVersion,
    applied: Option<AppliedFix>,
    reason: FixFailureReason,
) -> FixResult {
    FixResult {
        applied,
        next_version: current,
        report: FixReport {
            success: false,
            reason: Some(reason),
        },
    }
}

fn issue_stage_root(issue: &Issue) -> (Stage, &'static str) {
    let stage = match issue.path.segments.first().map(String::as_str) {
        Some("context") => Stage::Context,
        Some("function") => Stage::Function,
        Some("execution") => Stage::Execution,
        Some("architecture") => Stage::Architecture,
        Some("interface") => Stage::Interface,
        Some("data") => Stage::Data,
        _ => Stage::Context,
    };
    let root = match stage {
        Stage::Context => "context",
        Stage::Function => "function",
        Stage::Architecture => "architecture",
        Stage::Interface => "interface",
        Stage::Data => "data",
        Stage::Execution => "execution",
    };
    (stage, root)
}

fn default_context() -> ContextSpec {
    ContextSpec {
        target_user: Some("unspecified-user".to_string()),
        use_case: Some("unspecified-use-case".to_string()),
        environment: Some("default-environment".to_string()),
    }
}

fn default_function() -> crate::FunctionSpec {
    crate::FunctionSpec {
        functions: vec![default_function_name()],
    }
}

fn default_execution() -> ExecutionSpec {
    ExecutionSpec {
        steps: vec![default_step_name()],
    }
}

fn default_function_name() -> String {
    "default-function".to_string()
}

fn default_step_name() -> String {
    "default-step".to_string()
}

fn default_scalar_for_path(path: &FieldPath, issue: &Issue) -> String {
    replacement_value(issue)
        .or_else(|| match path_segments(path).as_slice() {
            ["context", "target_user"] => Some("unspecified-user".to_string()),
            ["context", "use_case"] => Some("unspecified-use-case".to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "normalized-value".to_string())
}

fn replacement_value(issue: &Issue) -> Option<String> {
    let before = issue.evidence.before.as_ref();
    let after = issue.evidence.after.as_ref();

    match path_segments(&issue.path).as_slice() {
        ["context", "target_user"] | ["context", "use_case"] => preferred_string(before, after),
        _ => preferred_string(before, after),
    }
}

fn path_segments(path: &FieldPath) -> Vec<&str> {
    path.segments.iter().map(String::as_str).collect()
}

fn preferred_string(before: Option<&Value>, after: Option<&Value>) -> Option<String> {
    if let Some(Value::String(text)) = before {
        return Some(text.clone());
    }
    if let Some(Value::String(text)) = after {
        return Some(text.clone());
    }
    None
}

fn remove_vec_item(values: &mut Vec<String>, key: &str) -> Result<(), FixFailureReason> {
    if let Ok(index) = key.parse::<usize>() {
        if index >= values.len() {
            return Err(FixFailureReason::InvalidPath);
        }
        values.remove(index);
        return Ok(());
    }

    let (base, ordinal) = split_segment_ordinal(key);
    let mut seen = 0usize;
    for index in 0..values.len() {
        if values[index] == base {
            if seen == ordinal {
                values.remove(index);
                return Ok(());
            }
            seen += 1;
        }
    }

    Err(FixFailureReason::InvalidPath)
}

fn split_segment_ordinal(segment: &str) -> (&str, usize) {
    if let Some((base, suffix)) = segment.rsplit_once('#')
        && let Ok(index) = suffix.parse::<usize>()
    {
        return (base, index);
    }
    (segment, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FixHint, Metadata, Priority, VersionId, VersionStatus, detect_issues, diff, init_history,
    };

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
            function: Some(crate::FunctionSpec {
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

    fn detect_issue_set(
        history: DesignHistory,
        before: DesignVersion,
        mut current: DesignVersion,
    ) -> FixInput {
        if current.parent.is_none() {
            current.parent = Some(before.id.clone());
        }
        let diff = diff(
            &crate::normalize(&before.design),
            &crate::normalize(&current.design),
        )
        .expect("diff");
        let issues = detect_issues(IssueInput {
            before: before.clone(),
            after: current.clone(),
            diff,
        })
        .expect("issues");
        FixInput {
            history,
            current,
            issues: issues.issues,
        }
    }

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

    #[test]
    fn applies_missing_fix_by_adding_default_value() {
        let before_design = context_design(Some("operator"), Some("review"));
        let current_design = DesignDocument {
            stage: Stage::Context,
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let history = init_history(before_design.clone());
        let before = history.versions[0].clone();
        let current = version(2, Stage::Context, current_design);
        let result = apply_next_fix(detect_issue_set(history, before, current));

        assert!(result.report.success);
        assert_eq!(
            result.applied.as_ref().map(|fix| &fix.action),
            Some(&FixAction::Add)
        );
        assert!(result.next_version.design.context.is_some());
    }

    #[test]
    fn applies_conflict_fix_with_replace_rule() {
        let before_design = context_design(Some("operator"), Some("review"));
        let current_design = context_design(Some("auditor"), Some("review"));
        let history = init_history(before_design.clone());
        let before = history.versions[0].clone();
        let current = version(2, Stage::Context, current_design);
        let result = apply_next_fix(detect_issue_set(history, before, current));

        assert!(result.report.success);
        assert_eq!(
            result
                .next_version
                .design
                .context
                .unwrap()
                .target_user
                .as_deref(),
            Some("operator")
        );
    }

    #[test]
    fn applies_redundancy_fix_with_remove() {
        let base = DesignDocument {
            stage: Stage::Context,
            context: Some(default_context()),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let mut history = init_history(base);
        let parent = history.head.clone();
        let before = create_version(
            &mut history,
            Some(parent),
            Stage::Function,
            function_design(vec!["search"]),
        )
        .expect("before");
        let current = version(
            3,
            Stage::Function,
            function_design(vec!["search", "search"]),
        );
        let result = apply_next_fix(detect_issue_set(history, before, current));

        assert!(result.report.success);
        assert_eq!(
            result.next_version.design.function.unwrap().functions,
            vec!["search"]
        );
    }

    #[test]
    fn applies_over_specification_fix_with_remove() {
        let base = DesignDocument {
            stage: Stage::Context,
            context: Some(default_context()),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let mut history = init_history(base);
        let parent = history.head.clone();
        let before = create_version(
            &mut history,
            Some(parent),
            Stage::Function,
            function_design(vec!["search"]),
        )
        .expect("before");
        let current = version(
            3,
            Stage::Function,
            function_design(vec!["search", "approve"]),
        );
        let result = apply_next_fix(detect_issue_set(history, before, current));

        assert!(result.report.success);
        assert_eq!(
            result.applied.as_ref().map(|fix| &fix.action),
            Some(&FixAction::Remove)
        );
        assert_eq!(
            result.next_version.design.function.unwrap().functions,
            vec!["search"]
        );
    }

    #[test]
    fn applies_under_specification_fix_with_normalize() {
        let base = DesignDocument {
            stage: Stage::Context,
            context: Some(default_context()),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let mut history = init_history(base);
        let parent = history.head.clone();
        let before = create_version(
            &mut history,
            Some(parent),
            Stage::Execution,
            execution_design(vec!["validate"]),
        )
        .expect("before");
        let current = version(3, Stage::Execution, execution_design(vec![]));
        let result = apply_next_fix(detect_issue_set(history, before, current));

        assert!(result.report.success);
        assert_eq!(
            result.next_version.design.execution.unwrap().steps,
            vec!["default-step"]
        );
    }

    #[test]
    fn rejects_invalid_path() {
        let current = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let history = init_history(current.design.clone());
        let issue = Issue {
            id: IssueId {
                value: "bad".to_string(),
            },
            issue_type: IssueType::Conflict,
            severity: Severity::High,
            priority: Priority::P1,
            order: 1,
            blocks: Vec::new(),
            path: FieldPath {
                segments: vec!["function".to_string(), "missing".to_string()],
            },
            reason: crate::IssueReason::ValueConflict,
            evidence: crate::IssueEvidence {
                before: None,
                after: None,
                semantic_reason: None,
                impact_reason: None,
            },
            fix_hint: Some(FixHint {
                action: FixAction::Replace,
                target: FieldPath {
                    segments: vec!["function".to_string(), "missing".to_string()],
                },
            }),
        };
        let result = apply_next_fix(FixInput {
            history,
            current: current.clone(),
            issues: vec![issue],
        });

        assert!(!result.report.success);
        assert_eq!(
            result.report.reason,
            Some(FixFailureReason::ConstraintViolation)
        );
        assert_eq!(result.next_version, current);
    }

    #[test]
    fn rolls_back_when_fix_has_no_effect() {
        let current = version(
            1,
            Stage::Context,
            context_design(Some("operator"), Some("review")),
        );
        let history = init_history(current.design.clone());
        let issue = Issue {
            id: IssueId {
                value: "noop".to_string(),
            },
            issue_type: IssueType::Conflict,
            severity: Severity::High,
            priority: Priority::P1,
            order: 1,
            blocks: Vec::new(),
            path: FieldPath {
                segments: vec!["context".to_string(), "target_user".to_string()],
            },
            reason: crate::IssueReason::ValueConflict,
            evidence: crate::IssueEvidence {
                before: Some(Value::String("operator".to_string())),
                after: Some(Value::String("operator".to_string())),
                semantic_reason: Some(crate::SemanticReason::ExactMatch),
                impact_reason: Some(crate::ImpactReason::NoMeaningfulChange),
            },
            fix_hint: None,
        };
        let result = apply_next_fix(FixInput {
            history,
            current: current.clone(),
            issues: vec![issue],
        });

        assert!(!result.report.success);
        assert_eq!(result.report.reason, Some(FixFailureReason::NoEffect));
        assert_eq!(result.next_version, current);
    }

    #[test]
    fn converges_deterministically_over_multiple_iterations() {
        let base = DesignDocument {
            stage: Stage::Context,
            context: Some(default_context()),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let mut history = init_history(base);
        let parent = history.head.clone();
        let before = create_version(
            &mut history,
            Some(parent),
            Stage::Function,
            function_design(vec!["search"]),
        )
        .expect("before");
        let mut current = version(
            3,
            Stage::Function,
            function_design(vec!["search", "search", "approve"]),
        );
        current.parent = Some(before.id.clone());

        let mut safety = 0;
        loop {
            let diff = diff(
                &crate::normalize(&before.design),
                &crate::normalize(&current.design),
            )
            .expect("diff");
            let issues = detect_issues(IssueInput {
                before: before.clone(),
                after: current.clone(),
                diff,
            })
            .expect("issues");
            if issues.issues.is_empty() {
                break;
            }
            let result = apply_next_fix(FixInput {
                history,
                current: current.clone(),
                issues: issues.issues,
            });
            assert!(result.report.success);
            history = DesignHistory {
                versions: vec![before.clone(), result.next_version.clone()],
                head: result.next_version.id.clone(),
                next_seq: result.next_version.id.seq + 1,
            };
            current = result.next_version;
            safety += 1;
            assert!(safety <= 4);
        }

        assert_eq!(current.design.function.unwrap().functions, vec!["search"]);
    }

    #[test]
    fn same_input_produces_same_fix_result() {
        let before_design = context_design(Some("operator"), Some("review"));
        let current_design = context_design(Some("auditor"), Some("review"));

        let history_a = init_history(before_design.clone());
        let before_a = history_a.versions[0].clone();
        let current_a = version(2, Stage::Context, current_design.clone());
        let result_a = apply_next_fix(detect_issue_set(history_a, before_a, current_a));

        let history_b = init_history(before_design);
        let before_b = history_b.versions[0].clone();
        let current_b = version(2, Stage::Context, current_design);
        let result_b = apply_next_fix(detect_issue_set(history_b, before_b, current_b));

        assert_eq!(result_a.applied, result_b.applied);
        assert_eq!(result_a.next_version.design, result_b.next_version.design);
    }
}
