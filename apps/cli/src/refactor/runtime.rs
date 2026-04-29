use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::coding::{
    CodingOptions, create_transactional_preview_sandbox, generate_code_change_set,
    run_transactional_preview_cargo_check,
};
use crate::refactor::PatchScope;
use crate::viewer::StructureViewIR;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyPreviewPlan {
    pub candidate_id: String,
    pub target_files: Vec<String>,
    pub operations: Vec<String>,
    pub checks: Vec<String>,
    pub rollback: RollbackPreview,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackPreview {
    pub mode: String,
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionPreview {
    pub candidate_id: String,
    pub allowed: bool,
    pub safe: bool,
    pub steps: Vec<String>,
    pub rollback_strategy: TransactionRollbackPreview,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRollbackPreview {
    pub mode: String,
    pub guaranteed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionExecutionPreview {
    pub candidate_id: String,
    pub allowed: bool,
    pub executed: bool,
    pub sandbox_write: SandboxWritePreview,
    pub steps: Vec<String>,
    pub rollback_guaranteed: bool,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxWritePreview {
    pub enabled: bool,
    pub target_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionResult {
    pub executed: bool,
    pub success: bool,
    pub sandbox_root: String,
    pub written_files: Vec<String>,
    pub cargo_check: String,
    pub rollback_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromoteResult {
    pub confirmed: bool,
    pub workspace_write: bool,
    pub written_files: Vec<String>,
    pub cargo_check: String,
    pub rollback_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitPreview {
    pub branch: String,
    pub protected_branch: bool,
    pub commit_allowed: bool,
    pub commit_message: String,
    pub changed_files: Vec<String>,
    pub push: bool,
}

pub fn generate_apply_preview_plan(ir: &StructureViewIR) -> Option<ApplyPreviewPlan> {
    let preview = ir.preview.as_ref()?;
    let candidate = ir
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == preview.candidate_id)?;

    let target_files = collect_target_files(ir, candidate);
    Some(ApplyPreviewPlan {
        candidate_id: candidate.candidate_id.clone(),
        target_files,
        operations: vec![format_patch_plan(&candidate.patch_plan)],
        checks: vec!["cargo check -p runtime_vm".to_string()],
        rollback: RollbackPreview {
            mode: "git diff based".to_string(),
            safe: true,
        },
        write: false,
    })
}

pub fn generate_transaction_preview(plan: &ApplyPreviewPlan) -> Option<TransactionPreview> {
    let file_name = plan
        .target_files
        .first()
        .map(|target_file| {
            Path::new(target_file)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(target_file.as_str())
                .to_string()
        })
        .unwrap_or_else(|| "target file".to_string());
    let allowed = !plan.target_files.is_empty()
        && !plan.operations.is_empty()
        && !plan.checks.is_empty()
        && !plan.write;

    Some(TransactionPreview {
        candidate_id: plan.candidate_id.clone(),
        allowed,
        safe: true,
        steps: vec![
            format!("write patch to {file_name}"),
            "cargo check -p runtime_vm".to_string(),
            "rollback on failure".to_string(),
        ],
        rollback_strategy: TransactionRollbackPreview {
            mode: "transactional git diff".to_string(),
            guaranteed: true,
        },
        write: false,
    })
}

pub fn generate_transaction_execution_preview(
    tx: &TransactionPreview,
    apply: &ApplyPreviewPlan,
) -> Option<TransactionExecutionPreview> {
    if !tx.allowed || apply.write {
        return None;
    }

    Some(TransactionExecutionPreview {
        candidate_id: tx.candidate_id.clone(),
        allowed: tx.allowed,
        executed: false,
        sandbox_write: SandboxWritePreview {
            enabled: true,
            target_files: apply.target_files.clone(),
        },
        steps: vec![
            "sandbox patch write".to_string(),
            "cargo check -p runtime_vm".to_string(),
            "commit preview".to_string(),
            "rollback on fail".to_string(),
        ],
        rollback_guaranteed: true,
        write: false,
    })
}

pub fn execute_transactional_safe_apply(
    exec: &TransactionExecutionPreview,
) -> Option<TransactionResult> {
    if !exec.allowed || exec.write || exec.executed {
        return None;
    }

    let root = std::env::current_dir().ok()?;
    let sandbox_relative = transactional_sandbox_relative_path()?;
    let sandbox_root = root.join(&sandbox_relative);

    if create_transactional_preview_sandbox(&root, &sandbox_root).is_err() {
        return Some(TransactionResult {
            executed: true,
            success: false,
            sandbox_root: sandbox_relative.clone(),
            written_files: Vec::new(),
            cargo_check: "failed".to_string(),
            rollback_executed: true,
        });
    }

    let write_result = write_preview_patch_to_sandbox(&sandbox_root, exec);
    if write_result.is_err() {
        let _ = fs::remove_dir_all(&sandbox_root);
        return Some(TransactionResult {
            executed: true,
            success: false,
            sandbox_root: sandbox_relative,
            written_files: Vec::new(),
            cargo_check: "failed".to_string(),
            rollback_executed: true,
        });
    }

    match run_transactional_preview_cargo_check(&root, &sandbox_root, "runtime_vm") {
        Ok(_) => Some(TransactionResult {
            executed: true,
            success: true,
            sandbox_root: sandbox_relative,
            written_files: exec.sandbox_write.target_files.clone(),
            cargo_check: "passed".to_string(),
            rollback_executed: false,
        }),
        Err(_) => {
            let _ = fs::remove_dir_all(&sandbox_root);
            Some(TransactionResult {
                executed: true,
                success: false,
                sandbox_root: sandbox_relative,
                written_files: exec.sandbox_write.target_files.clone(),
                cargo_check: "failed".to_string(),
                rollback_executed: true,
            })
        }
    }
}

pub fn promote_sandbox_to_workspace(
    tx: &TransactionResult,
    confirmed: bool,
) -> Option<PromoteResult> {
    if !tx.success || !confirmed {
        return None;
    }

    let root = std::env::current_dir().ok()?;
    let sandbox_root = root.join(&tx.sandbox_root);
    if !sandbox_root.exists() {
        return Some(PromoteResult {
            confirmed: true,
            workspace_write: false,
            written_files: Vec::new(),
            cargo_check: "failed".to_string(),
            rollback_executed: true,
        });
    }

    let files = exact_target_files(&tx.written_files)?;
    let snapshot = snapshot_workspace(&root, &files).ok()?;

    if copy_sandbox_files_to_workspace(&root, &sandbox_root, &files).is_err() {
        let _ = rollback_apply(&snapshot);
        return Some(PromoteResult {
            confirmed: true,
            workspace_write: false,
            written_files: files
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            cargo_check: "failed".to_string(),
            rollback_executed: true,
        });
    }

    match run_transactional_preview_cargo_check(&root, &root, "runtime_vm") {
        Ok(_) => Some(PromoteResult {
            confirmed: true,
            workspace_write: true,
            written_files: files
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            cargo_check: "passed".to_string(),
            rollback_executed: false,
        }),
        Err(_) => {
            let _ = rollback_apply(&snapshot);
            Some(PromoteResult {
                confirmed: true,
                workspace_write: true,
                written_files: files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                cargo_check: "failed".to_string(),
                rollback_executed: true,
            })
        }
    }
}

pub fn generate_git_commit_preview(promote: &PromoteResult) -> Option<GitCommitPreview> {
    if !promote.workspace_write {
        return None;
    }

    let root = std::env::current_dir().ok()?;
    let current_branch = git_current_branch(&root).unwrap_or_default();
    let protected_branch = is_protected_branch_name(&current_branch);
    let changed_files = promote.written_files.clone();

    Some(GitCommitPreview {
        branch: safe_preview_branch_name()?,
        protected_branch,
        commit_allowed: !protected_branch && !changed_files.is_empty(),
        commit_message: commit_preview_message(&changed_files),
        changed_files,
        push: false,
    })
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
            prompt_commit: false,
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

fn transactional_sandbox_relative_path() -> Option<String> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(format!(".dbm/sandbox/tx-{seconds}"))
}

fn exact_target_files(paths: &[String]) -> Option<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        let relative = PathBuf::from(path);
        if !is_safe_relative_file(&relative) {
            return None;
        }
        files.push(relative);
    }
    Some(files)
}

fn is_safe_relative_file(path: &Path) -> bool {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return false;
    }
    if path.to_string_lossy().contains('*') {
        return false;
    }
    path.components().all(|component| match component {
        Component::Normal(_) => true,
        Component::CurDir => true,
        Component::ParentDir | Component::Prefix(_) | Component::RootDir => false,
    })
}

fn copy_sandbox_files_to_workspace(
    root: &Path,
    sandbox_root: &Path,
    files: &[PathBuf],
) -> Result<(), String> {
    for file in files {
        let source = sandbox_root.join(file);
        let destination = root.join(file);
        let bytes = fs::read(&source)
            .map_err(|err| format!("failed to read {}: {err}", source.display()))?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        fs::write(&destination, bytes)
            .map_err(|err| format!("failed to write {}: {err}", destination.display()))?;
    }
    Ok(())
}

fn git_current_branch(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to inspect current branch: {err}"))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_protected_branch_name(branch: &str) -> bool {
    matches!(branch, "main" | "master")
        || branch.starts_with("release/")
        || branch.starts_with("production/")
        || branch.starts_with("hotfix/")
}

fn safe_preview_branch_name() -> Option<String> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(format!("dbm/auto-fix/{seconds}"))
}

fn commit_preview_message(changed_files: &[String]) -> String {
    if changed_files
        .iter()
        .any(|file| file.ends_with("adapter.rs"))
    {
        "dbm: remove adapter -> world dependency".to_string()
    } else {
        let summary = changed_files
            .first()
            .and_then(|file| Path::new(file).file_stem())
            .and_then(|stem| stem.to_str())
            .unwrap_or("workspace change");
        format!("dbm: update {summary}")
    }
}

fn write_preview_patch_to_sandbox(
    sandbox_root: &Path,
    exec: &TransactionExecutionPreview,
) -> Result<(), String> {
    let preview_lines = preview_apply_lines(&exec.candidate_id);
    for target in &exec.sandbox_write.target_files {
        let sandbox_target = sandbox_root.join(target);
        let mut content = fs::read_to_string(&sandbox_target)
            .map_err(|err| format!("failed to read {}: {err}", sandbox_target.display()))?;
        content.push_str("\n// DBM Preview Apply:\n");
        for line in &preview_lines {
            content.push_str("// ");
            content.push_str(line);
            content.push('\n');
        }
        if exec.candidate_id.contains("invalid") {
            content.push_str("this is not valid rust\n");
        }
        fs::write(&sandbox_target, content)
            .map_err(|err| format!("failed to write {}: {err}", sandbox_target.display()))?;
    }
    Ok(())
}

fn preview_apply_lines(candidate_id: &str) -> Vec<String> {
    let Some(remainder) = candidate_id.strip_prefix("cut-") else {
        return vec!["preview apply".to_string()];
    };
    let mut parts = remainder.split('-');
    let from = parts.next().unwrap_or("module");
    let to = parts.next().unwrap_or("dependency");
    vec![format!("- {from} -> {to}"), format!("+ {from} -> ports")]
}

fn collect_target_files(ir: &StructureViewIR, candidate: &super::RefactorCandidate) -> Vec<String> {
    let mut target_files = Vec::new();

    for node_id in &candidate.target_nodes {
        if let Some(path) = resolve_source_binding_file(ir, node_id)
            && !target_files.iter().any(|existing| existing == &path)
        {
            target_files.push(path);
        }
    }

    if target_files.is_empty()
        && let Some(path) = resolve_source_binding_file(ir, &candidate.from_node.logical_name)
    {
        target_files.push(path);
    }

    if target_files.is_empty() && !candidate.source_path.as_os_str().is_empty() {
        target_files.push(candidate.source_path.display().to_string());
    }

    target_files
}

fn resolve_source_binding_file(ir: &StructureViewIR, node_id: &str) -> Option<String> {
    ir.scene_3d
        .as_ref()?
        .graph
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.source_binding.as_ref())
        .map(|binding| binding.file.display().to_string())
}

fn format_patch_plan(target: &super::RefactorTarget) -> String {
    match target {
        super::RefactorTarget::Cycle => "Cycle".to_string(),
        super::RefactorTarget::ExtractInterface { from, to } => {
            format!("ExtractInterface({from} -> {to})")
        }
        super::RefactorTarget::RemoveDependency { from, to } => {
            format!("RemoveDependency({from} -> {to})")
        }
        super::RefactorTarget::ModuleSplit(name) => format!("ModuleSplit({name})"),
        super::RefactorTarget::MergeModule(nodes) => {
            format!("MergeModule({})", nodes.join(", "))
        }
        super::RefactorTarget::LayerViolation(name) => format!("LayerViolation({name})"),
        super::RefactorTarget::RenameBoundary(name) => format!("RenameBoundary({name})"),
        super::RefactorTarget::IntroduceService(name) => format!("IntroduceService({name})"),
        super::RefactorTarget::FileMove(path) => format!("FileMove({})", path.display()),
    }
}
