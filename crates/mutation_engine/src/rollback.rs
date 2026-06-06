use std::path::{Path, PathBuf};

use crate::error::MutationError;
use crate::executor::{restore, same_snapshot, timestamp};
use crate::model::{AuditResult, FileSnapshot, MutationAudit};
use crate::store::MutationAuditStore;

pub struct MutationRollbackEngine {
    workspace: PathBuf,
    store: MutationAuditStore,
}

impl MutationRollbackEngine {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
            store: MutationAuditStore::new(workspace),
        }
    }

    pub fn rollback(&self, id: &str, actor: &str) -> Result<(), MutationError> {
        let apply = self.store.load_apply(id)?;
        same_snapshot(&self.workspace, &apply.after)?;
        restore(&self.workspace, &apply.before)?;
        self.audit(id, actor, format!("rollback mutation {id}"))
    }

    pub fn rollback_last(&self, actor: &str) -> Result<String, MutationError> {
        let id = self
            .store
            .list_ids()?
            .into_iter()
            .next_back()
            .ok_or_else(|| MutationError::MissingRecord("last".to_string()))?;
        self.rollback(&id, actor)?;
        Ok(id)
    }

    pub fn rollback_snapshot(
        &self,
        id: &str,
        actor: &str,
        snapshot: &[FileSnapshot],
    ) -> Result<(), MutationError> {
        restore(&self.workspace, snapshot)?;
        self.audit(id, actor, format!("rollback snapshot for {id}"))
    }

    fn audit(&self, id: &str, actor: &str, why: String) -> Result<(), MutationError> {
        self.store.persist_audit(&MutationAudit {
            mutation_id: id.to_string(),
            who: actor.to_string(),
            when: timestamp(),
            why,
            what: "restore workspace state".to_string(),
            result: AuditResult::RolledBack,
        })
    }
}
