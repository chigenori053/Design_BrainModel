use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::model::{
    AnalyzeContext, MutationOperation, MutationPlan, MutationValidation, PatchOperation,
    RuntimeCheckResult, ValidationLayer, ValidationStatus, ValidationViolation,
};

pub trait RuntimeValidator {
    fn validate(&self, workspace: &Path, plan: &MutationPlan) -> Vec<RuntimeCheckResult>;
}

#[derive(Debug, Default)]
pub struct NoopRuntimeValidator;

impl RuntimeValidator for NoopRuntimeValidator {
    fn validate(&self, _workspace: &Path, _plan: &MutationPlan) -> Vec<RuntimeCheckResult> {
        Vec::new()
    }
}

pub struct MutationValidator<'a> {
    runtime: &'a dyn RuntimeValidator,
}

impl<'a> MutationValidator<'a> {
    pub fn new(runtime: &'a dyn RuntimeValidator) -> Self {
        Self { runtime }
    }

    pub fn validate(
        &self,
        workspace: &Path,
        plan: &MutationPlan,
        analyze: &AnalyzeContext,
    ) -> MutationValidation {
        let mut violations = Vec::new();
        validate_paths(plan, &mut violations);
        validate_security(plan, &mut violations);
        validate_structure(plan, analyze, &mut violations);
        validate_semantics(plan, analyze, &mut violations);
        let runtime_checks = self.runtime.validate(workspace, plan);
        for check in runtime_checks.iter().filter(|check| !check.passed) {
            violations.push(ValidationViolation {
                layer: ValidationLayer::Runtime,
                code: "runtime_check_failed".to_string(),
                message: format!("{}: {}", check.name, check.detail),
            });
        }
        let confirmation_required = requires_confirmation(plan);
        MutationValidation {
            mutation_id: plan.id.clone(),
            status: if violations.is_empty() {
                ValidationStatus::Passed
            } else {
                ValidationStatus::Rejected
            },
            violations,
            runtime_checks,
            confirmation_required,
        }
    }
}

pub fn requires_confirmation(plan: &MutationPlan) -> bool {
    matches!(
        plan.operation,
        MutationOperation::Delete | MutationOperation::Refactor
    ) || plan.patches.len() > 5
        || plan
            .patches
            .iter()
            .any(|patch| matches!(patch, PatchOperation::Delete { .. }))
        || affected_modules(plan) > 1
}

fn validate_paths(plan: &MutationPlan, violations: &mut Vec<ValidationViolation>) {
    for path in plan.patches.iter().flat_map(PatchOperation::affected_paths) {
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            violations.push(ValidationViolation {
                layer: ValidationLayer::Security,
                code: "path_outside_workspace".to_string(),
                message: format!("unsafe mutation path: {}", path.display()),
            });
        }
        if path.as_os_str().is_empty() || path == Path::new(".") {
            violations.push(ValidationViolation {
                layer: ValidationLayer::Security,
                code: "repository_delete".to_string(),
                message: "repository root cannot be mutated".to_string(),
            });
        }
        if path == Path::new(".git") || path.starts_with(".git/") {
            violations.push(ValidationViolation {
                layer: ValidationLayer::Security,
                code: "git_metadata_mutation".to_string(),
                message: "git metadata cannot be mutated".to_string(),
            });
        }
    }
}

fn validate_security(plan: &MutationPlan, violations: &mut Vec<ValidationViolation>) {
    for patch in &plan.patches {
        let PatchOperation::Write { content, .. } = patch else {
            continue;
        };
        for forbidden in [
            "rm -rf",
            "git push --force",
            "git push -f",
            "repository delete",
        ] {
            if content.contains(forbidden) {
                violations.push(ValidationViolation {
                    layer: ValidationLayer::Security,
                    code: "forbidden_operation".to_string(),
                    message: format!("forbidden operation detected: {forbidden}"),
                });
            }
        }
    }
}

fn validate_structure(
    plan: &MutationPlan,
    analyze: &AnalyzeContext,
    violations: &mut Vec<ValidationViolation>,
) {
    let projected_dependencies = plan
        .projected_dependencies
        .as_deref()
        .unwrap_or(&analyze.dependencies);
    if has_cycle(&analyze.nodes, projected_dependencies) {
        violations.push(ValidationViolation {
            layer: ValidationLayer::Structural,
            code: "circular_dependency".to_string(),
            message: "projected structure contains a circular dependency".to_string(),
        });
    }

    for boundary in &analyze.responsibility_boundaries {
        for dependency in projected_dependencies
            .iter()
            .filter(|edge| edge.from == boundary.owner)
        {
            if !boundary.allowed_dependencies.contains(&dependency.to) {
                violations.push(ValidationViolation {
                    layer: ValidationLayer::Structural,
                    code: "responsibility_boundary_violation".to_string(),
                    message: format!("{} may not depend on {}", boundary.owner, dependency.to),
                });
            }
        }
    }

    if !analyze.design_intent.is_empty()
        && !plan.design_intent.is_empty()
        && !plan
            .design_intent
            .iter()
            .any(|intent| analyze.design_intent.contains(intent))
    {
        violations.push(ValidationViolation {
            layer: ValidationLayer::Structural,
            code: "design_intent_mismatch".to_string(),
            message: "mutation does not match analyzed design intent".to_string(),
        });
    }
}

fn validate_semantics(
    plan: &MutationPlan,
    analyze: &AnalyzeContext,
    violations: &mut Vec<ValidationViolation>,
) {
    if plan.reason.trim().is_empty() || plan.expected_effect.trim().is_empty() {
        violations.push(ValidationViolation {
            layer: ValidationLayer::Semantic,
            code: "missing_rationale".to_string(),
            message: "reason and expected effect are required".to_string(),
        });
    }
    if plan.patches.is_empty() {
        violations.push(ValidationViolation {
            layer: ValidationLayer::Semantic,
            code: "empty_mutation".to_string(),
            message: "mutation must contain at least one patch operation".to_string(),
        });
    }
    if matches!(
        plan.operation,
        MutationOperation::Delete | MutationOperation::Rename
    ) && analyze
        .public_api_symbols
        .iter()
        .any(|symbol| plan.reason.contains(symbol))
    {
        violations.push(ValidationViolation {
            layer: ValidationLayer::Semantic,
            code: "public_api_change".to_string(),
            message: "public API change requires a compatibility plan".to_string(),
        });
    }
}

fn has_cycle(nodes: &[String], dependencies: &[crate::model::DependencyEdge]) -> bool {
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();
    for edge in dependencies {
        graph.entry(&edge.from).or_default().push(&edge.to);
    }
    nodes.iter().any(|node| {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        visit(node, &graph, &mut visiting, &mut visited)
    })
}

fn visit<'a>(
    node: &'a str,
    graph: &BTreeMap<&'a str, Vec<&'a str>>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> bool {
    if visiting.contains(node) {
        return true;
    }
    if !visited.insert(node) {
        return false;
    }
    visiting.insert(node);
    if graph
        .get(node)
        .into_iter()
        .flatten()
        .any(|next| visit(next, graph, visiting, visited))
    {
        return true;
    }
    visiting.remove(node);
    false
}

fn affected_modules(plan: &MutationPlan) -> usize {
    plan.patches
        .iter()
        .flat_map(PatchOperation::affected_paths)
        .filter_map(|path| path.components().next())
        .map(|component| component.as_os_str().to_owned())
        .collect::<BTreeSet<_>>()
        .len()
}
