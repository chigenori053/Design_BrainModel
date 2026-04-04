use std::path::Path;

use serde::Serialize;

use crate::coding::{CodingOptions, generate_code_change_set};
use crate::refactor::PatchScope;

use super::{
    ApplyResult, RefactorPlan, RefactorPreview, ValidationResult, file_move_change_set,
    preview::render_preview,
    rollback::{rollback_apply, snapshot_workspace},
    validate_refactor,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorRuntimeOptions {
    pub auto_commit: bool,
    pub no_build: bool,
    pub backup: bool,
    pub format: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefactorApplyReport {
    pub root: String,
    pub plan: RefactorPlan,
    pub preview: RefactorPreview,
    pub validation: ValidationResult,
    pub apply: ApplyResult,
}

pub fn build_apply_report(
    plan: &RefactorPlan,
    options: &RefactorRuntimeOptions,
) -> Result<RefactorApplyReport, String> {
    let preview = render_preview(plan);
    let validation = validate_refactor(plan)?;
    let apply = apply_refactor(plan, options, &validation)?;
    Ok(RefactorApplyReport {
        root: plan.root.display().to_string(),
        plan: plan.clone(),
        preview,
        validation,
        apply,
    })
}

pub fn apply_refactor(
    plan: &RefactorPlan,
    options: &RefactorRuntimeOptions,
    validation: &ValidationResult,
) -> Result<ApplyResult, String> {
    if !validation.valid {
        return Ok(ApplyResult {
            applied: false,
            build_ok: false,
            rolled_back: false,
            changed_files: Vec::new(),
            commit_id: None,
        });
    }

    let snapshot = snapshot_workspace(&plan.root, &plan.affected_files)?;
    let change_set = match &plan.target {
        super::RefactorTarget::FileMove(path) => file_move_change_set(&plan.root, path)?,
        _ => generate_code_change_set(&plan.root, &plan.patches)?,
    };
    let execution = crate::coding::execute_code_change_set(
        &plan.root,
        &change_set,
        &CodingOptions {
            apply: true,
            check: true,
            no_build: options.no_build,
            backup: options.backup,
            format: options.format,
            safe_mode: true,
            auto_commit: options.auto_commit,
            confirm_commit: false,
            auto_push: false,
            confirm_push: false,
            auto_pr: false,
            confirm_pr: false,
            pr_base: "main".to_string(),
            patch_scope: PatchScope::WorkspaceWide,
            explicit_target: None,
        },
        None,
    )?;

    if !execution.applied {
        let _ = rollback_apply(&snapshot);
        return Ok(ApplyResult {
            applied: false,
            build_ok: execution.build_ok,
            rolled_back: true,
            changed_files: Vec::new(),
            commit_id: execution.commit_id,
        });
    }

    let post_validation = validate_post_apply(&plan.root, plan)?;
    if !post_validation.valid {
        rollback_apply(&snapshot)?;
        return Ok(ApplyResult {
            applied: false,
            build_ok: false,
            rolled_back: true,
            changed_files: Vec::new(),
            commit_id: None,
        });
    }

    Ok(ApplyResult {
        applied: true,
        build_ok: execution.build_ok,
        rolled_back: execution.rolled_back,
        changed_files: plan.affected_files.clone(),
        commit_id: execution.commit_id,
    })
}

fn validate_post_apply(root: &Path, plan: &RefactorPlan) -> Result<ValidationResult, String> {
    let validation = validate_refactor(plan)?;
    for file in &plan.affected_files {
        let path = if file.is_absolute() {
            file.clone()
        } else {
            root.join(file)
        };
        if !path.exists() && !matches!(plan.target, super::RefactorTarget::FileMove(_)) {
            return Ok(ValidationResult {
                valid: false,
                cycle_removed: validation.cycle_removed,
                no_new_layer_violation: validation.no_new_layer_violation,
                buildable: false,
                public_api_preserved: validation.public_api_preserved,
                issues: vec![format!(
                    "expected changed file is missing: {}",
                    path.display()
                )],
            });
        }
    }
    Ok(validation)
}
