use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::MutationError;
use crate::model::{
    AuditResult, Confirmation, FileSnapshot, MutationApplyRecord, MutationAudit, MutationPlan,
    MutationPreview, MutationValidation, PatchOperation, RuntimeCheckResult,
};
use crate::store::{MutationAuditStore, atomic_write};
use crate::validator::RuntimeValidator;

pub(crate) struct ApplyContext<'a> {
    pub workspace: &'a Path,
    pub actor: &'a str,
    pub plan: &'a MutationPlan,
    pub validation: &'a MutationValidation,
    pub preview: &'a MutationPreview,
    pub confirmation: Option<&'a Confirmation>,
    pub runtime: &'a dyn RuntimeValidator,
    pub store: &'a MutationAuditStore,
}

pub(crate) fn apply(context: ApplyContext<'_>) -> Result<MutationApplyRecord, MutationError> {
    let ApplyContext {
        workspace,
        actor,
        plan,
        validation,
        preview,
        confirmation,
        runtime,
        store,
    } = context;
    if validation.mutation_id != plan.id || preview.mutation_id != plan.id {
        return Err(MutationError::InvalidState(
            "validation and preview must belong to the plan",
        ));
    }
    if !validation.passed() {
        return Err(MutationError::Rejected(
            validation
                .violations
                .iter()
                .map(|violation| violation.message.clone())
                .collect(),
        ));
    }
    if validation.confirmation_required {
        match confirmation {
            Some(confirmation) if confirmation.mutation_id == plan.id => {}
            Some(_) => return Err(MutationError::InvalidConfirmation),
            None => return Err(MutationError::ConfirmationRequired),
        }
    }

    let paths = affected_paths(plan);
    let before = snapshot(workspace, &paths)?;
    if let Err(error) = apply_operations(workspace, &plan.patches) {
        restore(workspace, &before)?;
        persist_failed_audit(store, actor, plan, error.to_string());
        return Err(error);
    }

    let verification = runtime.validate(workspace, plan);
    let failures = verification
        .iter()
        .filter(|check| !check.passed)
        .map(|check| format!("{}: {}", check.name, check.detail))
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        restore(workspace, &before)?;
        persist_failed_audit(store, actor, plan, failures.join("; "));
        return Err(MutationError::VerificationFailed(failures));
    }

    let after = snapshot(workspace, &paths)?;
    let record = MutationApplyRecord {
        mutation_id: plan.id.clone(),
        applied_at: timestamp(),
        operations: plan.patches.clone(),
        before,
        after,
        verification,
    };
    store.persist_apply(&record)?;
    store.persist_audit(&MutationAudit {
        mutation_id: plan.id.clone(),
        who: actor.to_string(),
        when: record.applied_at,
        why: plan.reason.clone(),
        what: format!("{:?} {:?}", plan.operation, plan.target),
        result: AuditResult::Applied,
    })?;
    Ok(record)
}

fn persist_failed_audit(
    store: &MutationAuditStore,
    actor: &str,
    plan: &MutationPlan,
    detail: String,
) {
    let _ = store.persist_audit(&MutationAudit {
        mutation_id: plan.id.clone(),
        who: actor.to_string(),
        when: timestamp(),
        why: plan.reason.clone(),
        what: detail,
        result: AuditResult::Failed,
    });
}

pub(crate) fn apply_operations(
    workspace: &Path,
    operations: &[PatchOperation],
) -> Result<(), MutationError> {
    for operation in operations {
        match operation {
            PatchOperation::Write { path, content } => {
                let target = resolve(workspace, path)?;
                atomic_write(&target, content.as_bytes())?;
            }
            PatchOperation::Delete { path } => {
                let target = resolve(workspace, path)?;
                if target.exists() {
                    fs::remove_file(target)?;
                }
            }
            PatchOperation::Move { from, to } => {
                let source = resolve(workspace, from)?;
                let destination = resolve(workspace, to)?;
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(source, destination)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn snapshot(
    workspace: &Path,
    paths: &[PathBuf],
) -> Result<Vec<FileSnapshot>, MutationError> {
    paths
        .iter()
        .map(|path| {
            let target = resolve(workspace, path)?;
            Ok(FileSnapshot {
                path: path.clone(),
                content: if target.exists() {
                    Some(fs::read(target)?)
                } else {
                    None
                },
            })
        })
        .collect()
}

pub(crate) fn restore(workspace: &Path, snapshots: &[FileSnapshot]) -> Result<(), MutationError> {
    for snapshot in snapshots {
        let target = resolve(workspace, &snapshot.path)?;
        match &snapshot.content {
            Some(content) => atomic_write(&target, content)?,
            None if target.exists() => fs::remove_file(target)?,
            None => {}
        }
    }
    Ok(())
}

pub(crate) fn same_snapshot(
    workspace: &Path,
    expected: &[FileSnapshot],
) -> Result<(), MutationError> {
    for snapshot in expected {
        let current = resolve(workspace, &snapshot.path)?;
        let content = if current.exists() {
            Some(fs::read(current)?)
        } else {
            None
        };
        if content != snapshot.content {
            return Err(MutationError::DriftDetected(snapshot.path.clone()));
        }
    }
    Ok(())
}

pub(crate) fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn affected_paths(plan: &MutationPlan) -> Vec<PathBuf> {
    plan.patches
        .iter()
        .flat_map(PatchOperation::affected_paths)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn resolve(workspace: &Path, relative: &Path) -> Result<PathBuf, MutationError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(MutationError::UnsafePath(relative.to_path_buf()));
    }
    Ok(workspace.join(relative))
}

pub fn failed_runtime_check(name: &str, detail: &str) -> RuntimeCheckResult {
    RuntimeCheckResult {
        name: name.to_string(),
        passed: false,
        detail: detail.to_string(),
    }
}
