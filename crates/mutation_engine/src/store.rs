use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::MutationError;
use crate::model::{
    MutationApplyRecord, MutationAudit, MutationAuditLog, MutationPlan, MutationPreview,
    MutationReplayRecord,
};

#[derive(Debug, Clone)]
pub struct MutationAuditStore {
    root: PathBuf,
}

impl MutationAuditStore {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        let workspace = core_types::WorkspaceRoot::discover_from(workspace.as_ref());
        Self {
            root: workspace.join(".dbm").join("mutations"),
        }
    }

    pub fn persist_plan(&self, plan: &MutationPlan) -> Result<(), MutationError> {
        self.write(&plan.id, "mutation_plan.json", plan)
    }

    pub fn persist_preview(&self, preview: &MutationPreview) -> Result<(), MutationError> {
        self.write(&preview.mutation_id, "mutation_preview.json", preview)
    }

    pub fn persist_apply(&self, apply: &MutationApplyRecord) -> Result<(), MutationError> {
        self.write(&apply.mutation_id, "mutation_apply.json", apply)?;
        self.write(
            &apply.mutation_id,
            "mutation_replay.json",
            &MutationReplayRecord {
                mutation_id: apply.mutation_id.clone(),
                timestamp: apply.applied_at,
                operations: apply.operations.clone(),
            },
        )
    }

    pub fn persist_audit(&self, audit: &MutationAudit) -> Result<(), MutationError> {
        let path = self
            .root
            .join(&audit.mutation_id)
            .join("mutation_audit.json");
        let mut log = if path.exists() {
            let bytes = fs::read(&path)?;
            serde_json::from_slice::<MutationAuditLog>(&bytes).or_else(|_| {
                serde_json::from_slice::<MutationAudit>(&bytes).map(|event| MutationAuditLog {
                    events: vec![event],
                })
            })?
        } else {
            MutationAuditLog::default()
        };
        log.events.push(audit.clone());
        self.write(&audit.mutation_id, "mutation_audit.json", &log)
    }

    pub fn load_plan(&self, id: &str) -> Result<MutationPlan, MutationError> {
        self.read(id, "mutation_plan.json")
    }

    pub fn load_apply(&self, id: &str) -> Result<MutationApplyRecord, MutationError> {
        self.read(id, "mutation_apply.json")
    }

    pub fn load_replay(&self, id: &str) -> Result<MutationReplayRecord, MutationError> {
        self.read(id, "mutation_replay.json")
    }

    pub fn list_ids(&self) -> Result<Vec<String>, MutationError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut ids = fs::read_dir(&self.root)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>();
        ids.sort();
        Ok(ids)
    }

    fn write<T: Serialize>(&self, id: &str, name: &str, value: &T) -> Result<(), MutationError> {
        let directory = self.root.join(id);
        fs::create_dir_all(&directory)?;
        let bytes = serde_json::to_vec_pretty(value)?;
        atomic_write(&directory.join(name), &bytes)
    }

    fn read<T: DeserializeOwned>(&self, id: &str, name: &str) -> Result<T, MutationError> {
        let path = self.root.join(id).join(name);
        if !path.exists() {
            return Err(MutationError::MissingRecord(id.to_string()));
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), MutationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("dbm-tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    use crate::{MutationOperation, MutationPlan, MutationTarget};

    #[test]
    fn subdirectory_input_still_uses_workspace_mutation_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        core_types::WorkspaceRoot::ensure_layout(dir.path()).expect("layout");
        let nested = dir.path().join("apps/cli/src");
        fs::create_dir_all(&nested).expect("nested");
        let store = MutationAuditStore::new(&nested);
        let plan = MutationPlan {
            id: "mutation-1".to_string(),
            target: MutationTarget::File(PathBuf::from("src/lib.rs")),
            operation: MutationOperation::Refactor,
            reason: "test".to_string(),
            expected_effect: "test".to_string(),
            patches: Vec::new(),
            design_intent: Vec::new(),
            projected_dependencies: None,
        };

        store.persist_plan(&plan).expect("persist");

        assert!(
            dir.path()
                .join(".dbm/mutations/mutation-1/mutation_plan.json")
                .exists()
        );
        assert!(!nested.join(".dbm").exists());
    }
}
