use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::nl::types::{ExecutionPlan, Operation, ValidationPolicy};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub violations: Vec<Violation>,
    pub confidence: f32,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            violations: Vec::new(),
            confidence: 1.0,
        }
    }

    pub fn invalid(violations: Vec<Violation>) -> Self {
        Self {
            is_valid: false,
            confidence: 0.0,
            violations,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub code: String,
    pub message: String,
}

impl Violation {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub fn validate_before_execution(plan: &ExecutionPlan) -> ValidationResult {
    let mut violations = Vec::new();
    validate_deterministic_plan(plan, &mut violations);
    validate_target(plan, &mut violations);
    validate_rollback_policy(plan, &mut violations);

    if violations.is_empty() || plan.validation_policy == ValidationPolicy::Advisory {
        ValidationResult {
            is_valid: true,
            violations,
            confidence: if plan.validation_policy == ValidationPolicy::Advisory {
                0.8
            } else {
                1.0
            },
        }
    } else {
        ValidationResult::invalid(violations)
    }
}

pub fn validate_after_execution(plan: &ExecutionPlan, output: &str) -> ValidationResult {
    let mut violations = Vec::new();
    if output.contains("ERROR:") || output.contains("[ERROR]") {
        violations.push(Violation::new(
            "execution_error",
            "execution produced an error and must be rolled back by the caller",
        ));
    }
    validate_deterministic_plan(plan, &mut violations);

    if violations.is_empty() {
        ValidationResult::valid()
    } else {
        ValidationResult::invalid(violations)
    }
}

fn validate_deterministic_plan(plan: &ExecutionPlan, violations: &mut Vec<Violation>) {
    if !plan.execution_metadata.deterministic {
        violations.push(Violation::new(
            "nondeterministic_plan",
            "ExecutionPlan must be deterministic before execution",
        ));
    }
    if has_duplicate_flags(&plan.args.flags) {
        violations.push(Violation::new(
            "duplicate_flags",
            "Plan flags must be unique to preserve deterministic ordering",
        ));
    }
    if let Operation::Composite(ops) = &plan.operation {
        if ops.is_empty() {
            violations.push(Violation::new(
                "empty_composite",
                "Composite operation requires at least one child operation",
            ));
        }
        for op in ops {
            if matches!(op, Operation::Composite(_)) {
                violations.push(Violation::new(
                    "nested_composite",
                    "Nested composite operations are rejected for deterministic execution",
                ));
            }
        }
    }
}

fn validate_target(plan: &ExecutionPlan, violations: &mut Vec<Violation>) {
    match &plan.operation {
        Operation::Analyze
        | Operation::Refactor
        | Operation::Validate
        | Operation::Repair
        | Operation::Reload => {
            let Some(target) = &plan.target else {
                if matches!(plan.operation, Operation::Reload) {
                    return;
                }
                violations.push(Violation::new(
                    "missing_target",
                    "operation requires an explicit target",
                ));
                return;
            };
            if target.as_os_str().is_empty() || target == Path::new("/") {
                violations.push(Violation::new(
                    "unsafe_target",
                    "target must not be empty or filesystem root",
                ));
            }
        }
        Operation::Composite(_) | Operation::Apply | Operation::Rollback | Operation::NoOp => {}
    }
}

fn validate_rollback_policy(plan: &ExecutionPlan, violations: &mut Vec<Violation>) {
    if matches!(plan.operation, Operation::Refactor | Operation::Apply)
        && !plan.execution_metadata.rollback_required
    {
        violations.push(Violation::new(
            "rollback_not_required",
            "mutating operations must require rollback support",
        ));
    }
}

fn has_duplicate_flags(flags: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    flags.iter().any(|flag| !seen.insert(flag))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::nl::types::{ExecutionMetadata, PlanSource};

    #[test]
    fn blocks_mutating_plan_without_rollback() {
        let mut plan = ExecutionPlan::new(
            Operation::Refactor,
            Some(PathBuf::from("src/lib.rs")),
            PlanSource::System,
        );
        plan.execution_metadata = ExecutionMetadata {
            rollback_required: false,
            ..ExecutionMetadata::default()
        };

        let result = validate_before_execution(&plan);

        assert!(!result.is_valid);
        assert!(
            result
                .violations
                .iter()
                .any(|violation| violation.code == "rollback_not_required")
        );
    }

    #[test]
    fn validate_operation_accepts_explicit_target() {
        let plan = ExecutionPlan::new(
            Operation::Validate,
            Some(PathBuf::from(".")),
            PlanSource::System,
        );

        assert!(validate_before_execution(&plan).is_valid);
    }
}
