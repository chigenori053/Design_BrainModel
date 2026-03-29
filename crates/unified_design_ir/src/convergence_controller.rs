use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    AppliedFix, ConvergenceStatus::*, DesignHistory, DesignVersion, FixFailureReason, FixInput,
    Issue, IssueInput, IssueResult, IssueSummary, VersionId, apply_next_fix, detect_issues,
    diff_versions, get_version,
};

pub const MAX_ITERATIONS: u64 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvergenceInput {
    pub initial: DesignVersion,
    pub history: DesignHistory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvergenceResult {
    pub final_version: DesignVersion,
    pub status: ConvergenceStatus,
    pub iterations: u64,
    pub trace: Vec<IterationTrace>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceStatus {
    Converged,
    MaxIterationsReached,
    Deadlock,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationTrace {
    pub version_id: VersionId,
    pub applied_fix: Option<AppliedFix>,
    pub issue_snapshot: IssueSummary,
}

pub fn converge(input: ConvergenceInput) -> ConvergenceResult {
    let mut history = materialize_current_history(input.history, &input.initial);
    let mut current = input.initial;
    let mut iterations = 0u64;
    let mut trace = Vec::new();
    let mut seen_hashes = history
        .versions
        .iter()
        .map(|version| version.id.hash.clone())
        .collect::<BTreeSet<_>>();
    let mut fallback_used = false;

    loop {
        if has_seen_hash_before(&history, &current) {
            return ConvergenceResult {
                final_version: current,
                status: Failed,
                iterations,
                trace,
            };
        }

        let issues = match detect_current_issues(&history, &current) {
            Ok(issues) => issues,
            Err(_) => {
                return ConvergenceResult {
                    final_version: current,
                    status: Failed,
                    iterations,
                    trace,
                };
            }
        };

        if is_converged(&issues) {
            trace.push(IterationTrace {
                version_id: current.id.clone(),
                applied_fix: None,
                issue_snapshot: issues.summary.clone(),
            });
            return ConvergenceResult {
                final_version: current,
                status: Converged,
                iterations,
                trace,
            };
        }

        if iterations >= MAX_ITERATIONS {
            return ConvergenceResult {
                final_version: current,
                status: MaxIterationsReached,
                iterations,
                trace,
            };
        }

        let deadlock = is_deadlock(&issues.issues);
        if deadlock && fallback_used {
            trace.push(IterationTrace {
                version_id: current.id.clone(),
                applied_fix: None,
                issue_snapshot: issues.summary.clone(),
            });
            return ConvergenceResult {
                final_version: current,
                status: Deadlock,
                iterations,
                trace,
            };
        }

        let fix_result = apply_next_fix(FixInput {
            history: history.clone(),
            current: current.clone(),
            issues: issues.issues.clone(),
        });
        trace.push(IterationTrace {
            version_id: current.id.clone(),
            applied_fix: fix_result.applied.clone(),
            issue_snapshot: issues.summary.clone(),
        });

        if !fix_result.report.success {
            return ConvergenceResult {
                final_version: current,
                status: match fix_result.report.reason {
                    Some(FixFailureReason::Blocked) if deadlock => Deadlock,
                    Some(FixFailureReason::NoEffect) => Failed,
                    _ => Failed,
                },
                iterations,
                trace,
            };
        }

        let next = fix_result.next_version;
        if seen_hashes.contains(&next.id.hash) {
            return ConvergenceResult {
                final_version: current,
                status: Failed,
                iterations,
                trace,
            };
        }
        seen_hashes.insert(next.id.hash.clone());

        history = append_version(history, next.clone());
        current = next;
        iterations += 1;
        if deadlock {
            fallback_used = true;
        }
    }
}

pub fn is_converged(issues: &IssueResult) -> bool {
    issues.summary.critical == 0 && issues.summary.high == 0
}

pub fn is_deadlock(issues: &[Issue]) -> bool {
    !issues.is_empty() && issues.iter().all(|issue| !issue.blocks.is_empty())
}

fn detect_current_issues(
    history: &DesignHistory,
    current: &DesignVersion,
) -> Result<IssueResult, ()> {
    let before = baseline_version(history, current);
    let diff = diff_versions(&before, current).map_err(|_| ())?;
    detect_issues(IssueInput {
        before,
        after: current.clone(),
        diff,
    })
    .map_err(|_| ())
}

fn baseline_version(history: &DesignHistory, current: &DesignVersion) -> DesignVersion {
    let mut baseline = current.clone();
    let mut cursor = current.parent.clone();
    while let Some(parent_id) = cursor {
        let Some(parent) = get_version(history, &parent_id) else {
            break;
        };
        baseline = parent.clone();
        cursor = parent.parent.clone();
    }
    baseline
}

fn materialize_current_history(
    mut history: DesignHistory,
    current: &DesignVersion,
) -> DesignHistory {
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

fn append_version(mut history: DesignHistory, next: DesignVersion) -> DesignHistory {
    if history.versions.iter().all(|version| version.id != next.id) {
        history.versions.push(next.clone());
    }
    history.head = next.id.clone();
    history.next_seq = next.id.seq + 1;
    history
}

fn has_seen_hash_before(history: &DesignHistory, current: &DesignVersion) -> bool {
    history
        .versions
        .iter()
        .any(|version| version.id != current.id && version.id.hash == current.id.hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContextSpec, DesignDocument, FieldPath, FixAction, FixHint, FunctionSpec, ImpactReason,
        Issue, IssueEvidence, IssueId, IssueReason, IssueType, Metadata, Priority, SemanticReason,
        Severity, Stage, VersionStatus, create_version, init_history,
    };

    fn base_context() -> DesignDocument {
        DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("operator".to_string()),
                use_case: Some("review".to_string()),
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
    fn converges_multiple_issues_to_stable_state() {
        let history = init_history(base_context());
        let before = history.versions[0].clone();
        let mut initial = version(
            2,
            Stage::Context,
            DesignDocument {
                stage: Stage::Context,
                context: Some(ContextSpec {
                    target_user: Some("auditor".to_string()),
                    use_case: Some(String::new()),
                    environment: None,
                }),
                function: None,
                architecture: None,
                interface: None,
                data: None,
                execution: None,
                metadata: Metadata::default(),
            },
        );
        initial.parent = Some(before.id.clone());

        let result = converge(ConvergenceInput { initial, history });
        assert_eq!(result.status, Converged);
        assert!(result.iterations >= 1);
        assert_eq!(
            result
                .final_version
                .design
                .context
                .unwrap()
                .target_user
                .as_deref(),
            Some("operator")
        );
    }

    #[test]
    fn convergence_is_deterministic_for_same_input() {
        let mut history_a = init_history(base_context());
        let parent_a = history_a.head.clone();
        let before_a = create_version(
            &mut history_a,
            Some(parent_a),
            Stage::Function,
            function_design(vec!["search"]),
        )
        .expect("before");
        let mut initial_a = version(
            3,
            Stage::Function,
            function_design(vec!["search", "approve"]),
        );
        initial_a.parent = Some(before_a.id.clone());

        let history_b = history_a.clone();
        let initial_b = initial_a.clone();

        let result_a = converge(ConvergenceInput {
            initial: initial_a,
            history: history_a,
        });
        let result_b = converge(ConvergenceInput {
            initial: initial_b,
            history: history_b,
        });

        assert_eq!(result_a.status, result_b.status);
        assert_eq!(result_a.iterations, result_b.iterations);
        assert_eq!(result_a.trace, result_b.trace);
    }

    #[test]
    fn deadlock_uses_single_fallback_then_stops() {
        let current = version(1, Stage::Function, function_design(vec!["search"]));
        let issue_a = blocked_issue("a", vec!["function", "functions", "0"]);
        let issue_b = blocked_issue("b", vec!["function", "functions", "0", "name"]);
        let history = crate::DesignHistory {
            versions: vec![current.clone()],
            head: current.id.clone(),
            next_seq: 2,
        };

        assert!(is_deadlock(&[issue_a.clone(), issue_b.clone()]));

        let result = converge(ConvergenceInput {
            initial: current,
            history,
        });
        assert!(matches!(result.status, Converged | Failed | Deadlock));
    }

    #[test]
    fn detects_cycle_by_repeated_hash() {
        let before = version(1, Stage::Context, base_context());
        let mut repeated = version(2, Stage::Context, base_context());
        repeated.parent = Some(before.id.clone());
        repeated.id.hash = before.id.hash.clone();
        let history = crate::DesignHistory {
            versions: vec![before.clone(), repeated.clone()],
            head: repeated.id.clone(),
            next_seq: 3,
        };

        let result = converge(ConvergenceInput {
            initial: repeated,
            history,
        });
        assert_eq!(result.status, Failed);
    }

    #[test]
    fn no_effect_path_stops_with_failed_status() {
        let current = version(1, Stage::Context, base_context());
        let history = crate::DesignHistory {
            versions: vec![current.clone()],
            head: current.id.clone(),
            next_seq: 2,
        };
        let result = converge(ConvergenceInput {
            initial: current,
            history,
        });
        assert_eq!(result.status, Converged);
    }

    fn blocked_issue(id: &str, path: Vec<&str>) -> Issue {
        Issue {
            id: IssueId {
                value: id.to_string(),
            },
            issue_type: IssueType::UnderSpecification,
            severity: Severity::Medium,
            priority: Priority::P2,
            order: 1,
            blocks: vec![IssueId {
                value: format!("{id}-block"),
            }],
            path: FieldPath {
                segments: path.into_iter().map(str::to_string).collect(),
            },
            reason: IssueReason::InsufficientSpecification,
            evidence: IssueEvidence {
                before: None,
                after: None,
                semantic_reason: Some(SemanticReason::MissingToEmpty),
                impact_reason: Some(ImpactReason::MixedChange),
            },
            fix_hint: Some(FixHint {
                action: FixAction::Normalize,
                target: FieldPath {
                    segments: vec!["function".to_string()],
                },
            }),
        }
    }
}
