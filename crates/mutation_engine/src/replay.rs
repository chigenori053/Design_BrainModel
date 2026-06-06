use std::path::Path;

use crate::error::MutationError;
use crate::executor::{apply_operations, restore, same_snapshot, timestamp};
use crate::model::{AuditResult, MutationAudit, MutationReplayRecord};
use crate::store::MutationAuditStore;

pub struct MutationReplayEngine {
    workspace: std::path::PathBuf,
    store: MutationAuditStore,
}

impl MutationReplayEngine {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
            store: MutationAuditStore::new(workspace),
        }
    }

    pub fn replay(&self, id: &str, actor: &str) -> Result<MutationReplayRecord, MutationError> {
        let apply = self.store.load_apply(id)?;
        same_snapshot(&self.workspace, &apply.before)?;
        let replay = self.store.load_replay(id)?;
        if let Err(error) = apply_operations(&self.workspace, &replay.operations) {
            restore(&self.workspace, &apply.before)?;
            return Err(error);
        }
        if let Err(error) = same_snapshot(&self.workspace, &apply.after) {
            restore(&self.workspace, &apply.before)?;
            return Err(error);
        }
        self.store.persist_audit(&MutationAudit {
            mutation_id: id.to_string(),
            who: actor.to_string(),
            when: timestamp(),
            why: "replay persisted mutation".to_string(),
            what: format!("{} patch operations", replay.operations.len()),
            result: AuditResult::Replayed,
        })?;
        Ok(replay)
    }

    pub fn replay_range(
        &self,
        start: &str,
        end: &str,
        actor: &str,
    ) -> Result<Vec<MutationReplayRecord>, MutationError> {
        self.store
            .list_ids()?
            .into_iter()
            .filter(|id| id.as_str() >= start && id.as_str() <= end)
            .map(|id| self.replay(&id, actor))
            .collect()
    }

    pub fn replay_all(&self, actor: &str) -> Result<Vec<MutationReplayRecord>, MutationError> {
        self.store
            .list_ids()?
            .into_iter()
            .map(|id| self.replay(&id, actor))
            .collect()
    }
}
