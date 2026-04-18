use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::service::dto::{ActionKind, IRState, SessionAppliedDiff};

const CHECKPOINT_INTERVAL: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IRSessionRecord {
    pub session_id: String,
    pub created_at: u64,
    pub workspace_root: PathBuf,
    pub current_target: Option<PathBuf>,
    pub last_checkpoint_step: usize,
    pub last_step_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IRTransitionRecord {
    pub transition_id: String,
    pub session_id: String,
    pub step_index: usize,
    pub action_kind: ActionKind,
    pub input_text: String,
    pub from_state_hash: String,
    pub to_state_hash: String,
    pub timestamp: u64,
    pub target: Option<PathBuf>,
    pub next_actions: Vec<ActionKind>,
    pub serialized_post_ir_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IRStateSnapshotRecord {
    pub snapshot_id: String,
    pub session_id: String,
    pub step_index: usize,
    pub serialized_ir_state: String,
    pub state_hash: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IRTransactionArtifactRecord {
    pub transaction_id: String,
    pub step_index: usize,
    pub diff_ref: Option<SessionAppliedDiff>,
    pub build_ok: Option<bool>,
    pub validation_ok: Option<bool>,
    pub rollback_checkpoint: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IRPersistenceArtifact {
    pub diff_ref: Option<SessionAppliedDiff>,
    pub build_ok: Option<bool>,
    pub validation_ok: Option<bool>,
    pub rollback_checkpoint: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayTimelineEntry {
    pub step: usize,
    pub action: String,
    pub target: Option<String>,
    pub state_hash: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryOutcome {
    pub state: IRState,
    pub recovered_step: usize,
    pub fallback_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainValidation {
    pub valid: bool,
    pub latest_valid_step: usize,
    pub latest_checkpoint_step: usize,
    pub fallback_state: IRState,
}

#[derive(Debug, Clone)]
pub struct IRPersistenceStore {
    workspace_root: PathBuf,
}

impl IRPersistenceStore {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    pub fn recover_or_create(&self) -> Result<RecoveryOutcome, String> {
        fs::create_dir_all(self.sessions_dir()).map_err(|err| err.to_string())?;
        let session_id = match self.workspace_index().get(&self.workspace_key()) {
            Some(session_id) => session_id.clone(),
            None => {
                let state = self.initial_state(new_session_id(), self.workspace_root.clone());
                let session = IRSessionRecord {
                    session_id: state.session_id.clone(),
                    created_at: now_ts(),
                    workspace_root: self.workspace_root.clone(),
                    current_target: state.current_target.clone(),
                    last_checkpoint_step: 0,
                    last_step_index: 0,
                };
                self.write_json(&self.session_record_path(&state.session_id), &session)?;
                self.append_jsonl(
                    &self.snapshots_path(&state.session_id),
                    &IRStateSnapshotRecord {
                        snapshot_id: format!("snap:{}:0", state.session_id),
                        session_id: state.session_id.clone(),
                        step_index: 0,
                        serialized_ir_state: serialize_state(&state)?,
                        state_hash: state_hash(&state)?,
                        timestamp: now_ts(),
                    },
                )?;
                self.update_workspace_index(&state.session_id)?;
                return Ok(RecoveryOutcome {
                    state,
                    recovered_step: 0,
                    fallback_used: false,
                });
            }
        };

        self.load_latest(&session_id)
    }

    pub fn load_latest(&self, session_id: &str) -> Result<RecoveryOutcome, String> {
        let session = self.read_session(session_id)?;
        let validation = self.validate_hash_chain(session_id)?;
        let checkpoint_step = if validation.valid {
            session.last_checkpoint_step
        } else {
            validation.latest_checkpoint_step
        };
        let latest_checkpoint = self
            .load_checkpoint(session_id, checkpoint_step)
            .or_else(|_| self.load_checkpoint(session_id, validation.latest_checkpoint_step))?;
        Ok(RecoveryOutcome {
            fallback_used: latest_checkpoint.step_index != session.last_checkpoint_step
                || !validation.valid,
            recovered_step: latest_checkpoint.step_index,
            state: latest_checkpoint.state,
        })
    }

    pub fn load_checkpoint(
        &self,
        session_id: &str,
        step: usize,
    ) -> Result<LoadedCheckpoint, String> {
        let snapshots =
            self.read_jsonl::<IRStateSnapshotRecord>(&self.snapshots_path(session_id))?;
        let Some(snapshot) = snapshots
            .into_iter()
            .filter(|snapshot| snapshot.step_index <= step)
            .max_by_key(|snapshot| snapshot.step_index)
        else {
            return Err(format!(
                "checkpoint not found for session {session_id} step {step}"
            ));
        };
        let state = deserialize_state(&snapshot.serialized_ir_state)?;
        if state_hash(&state)? != snapshot.state_hash {
            return Err(format!(
                "checkpoint hash mismatch for session {session_id} step {}",
                snapshot.step_index
            ));
        }
        Ok(LoadedCheckpoint {
            step_index: snapshot.step_index,
            state,
        })
    }

    pub fn record_transition(
        &self,
        before: &IRState,
        after: &IRState,
        action_kind: ActionKind,
        input_text: impl Into<String>,
        artifact: IRPersistenceArtifact,
    ) -> Result<(), String> {
        let session_id = non_empty_session_id(before, after)?;
        let mut session = self.read_session(&session_id)?;
        let step_index = session.last_step_index + 1;
        let serialized_post_ir_state = serialize_state(after)?;
        let from_state_hash = state_hash(before)?;
        let to_state_hash = state_hash(after)?;
        let transition = IRTransitionRecord {
            transition_id: format!("tr:{session_id}:{step_index}"),
            session_id: session_id.clone(),
            step_index,
            action_kind,
            input_text: input_text.into(),
            from_state_hash,
            to_state_hash: to_state_hash.clone(),
            timestamp: now_ts(),
            target: after.current_target.clone(),
            next_actions: after.next_allowed_actions.clone(),
            serialized_post_ir_state,
        };
        self.append_jsonl(&self.transitions_path(&session_id), &transition)?;

        if let Some(active) = after.active_transaction.as_ref() {
            let artifact_record = IRTransactionArtifactRecord {
                transaction_id: active.transaction_id.clone(),
                step_index,
                diff_ref: artifact.diff_ref,
                build_ok: artifact.build_ok.or(active.latest_build_ok),
                validation_ok: artifact.validation_ok.or(active.latest_build_ok),
                rollback_checkpoint: artifact
                    .rollback_checkpoint
                    .or(Some(session.last_checkpoint_step)),
            };
            self.append_jsonl(&self.artifacts_path(&session_id), &artifact_record)?;
        }

        if self.should_checkpoint(step_index, transition.action_kind) {
            let snapshot = IRStateSnapshotRecord {
                snapshot_id: format!("snap:{session_id}:{step_index}"),
                session_id: session_id.clone(),
                step_index,
                serialized_ir_state: serialize_state(after)?,
                state_hash: to_state_hash,
                timestamp: now_ts(),
            };
            self.append_jsonl(&self.snapshots_path(&session_id), &snapshot)?;
            session.last_checkpoint_step = step_index;
        }

        session.last_step_index = step_index;
        session.current_target = after.current_target.clone();
        self.write_json(&self.session_record_path(&session_id), &session)?;
        self.update_workspace_index(&session_id)?;
        Ok(())
    }

    pub fn rebuild_at_step(&self, session_id: &str, step: usize) -> Result<IRState, String> {
        if step == 0 {
            return Ok(self.load_checkpoint(session_id, 0)?.state);
        }
        let validation = self.validate_hash_chain(session_id)?;
        if step > validation.latest_valid_step {
            return Err(format!(
                "requested step {step} exceeds latest valid step {}",
                validation.latest_valid_step
            ));
        }
        let transitions =
            self.read_jsonl::<IRTransitionRecord>(&self.transitions_path(session_id))?;
        let transition = transitions
            .into_iter()
            .find(|record| record.step_index == step)
            .ok_or_else(|| format!("transition step {step} not found"))?;
        deserialize_state(&transition.serialized_post_ir_state)
    }

    pub fn export_timeline(&self, session_id: &str) -> Result<Vec<ReplayTimelineEntry>, String> {
        let validation = self.validate_hash_chain(session_id)?;
        let transitions =
            self.read_jsonl::<IRTransitionRecord>(&self.transitions_path(session_id))?;
        let mut entries = Vec::new();
        entries.push(ReplayTimelineEntry {
            step: 0,
            action: "Checkpoint".to_string(),
            target: validation
                .fallback_state
                .current_target
                .as_ref()
                .map(|path| path.display().to_string()),
            state_hash: state_hash(&validation.fallback_state)?,
            next_actions: validation
                .fallback_state
                .next_allowed_actions
                .iter()
                .map(|action| format!("{action:?}"))
                .collect(),
        });

        for transition in transitions
            .into_iter()
            .filter(|record| record.step_index <= validation.latest_valid_step)
        {
            let state = deserialize_state(&transition.serialized_post_ir_state)?;
            entries.push(ReplayTimelineEntry {
                step: transition.step_index,
                action: format!("{:?}", transition.action_kind),
                target: transition.target.map(|path| path.display().to_string()),
                state_hash: transition.to_state_hash,
                next_actions: state
                    .next_allowed_actions
                    .iter()
                    .map(|action| format!("{action:?}"))
                    .collect(),
            });
        }

        let replay_dir = self.workspace_root.join(".dbm").join("replay");
        fs::create_dir_all(&replay_dir).map_err(|err| err.to_string())?;
        let export_path = replay_dir.join(format!("{session_id}.jsonl"));
        let mut file = File::create(&export_path).map_err(|err| err.to_string())?;
        for entry in &entries {
            let line = serde_json::to_string(entry).map_err(|err| err.to_string())?;
            writeln!(file, "{line}").map_err(|err| err.to_string())?;
        }
        Ok(entries)
    }

    pub fn validate_hash_chain(&self, session_id: &str) -> Result<ChainValidation, String> {
        let baseline = self.load_checkpoint(session_id, 0)?;
        let transitions =
            self.read_jsonl::<IRTransitionRecord>(&self.transitions_path(session_id))?;
        let snapshots =
            self.read_jsonl::<IRStateSnapshotRecord>(&self.snapshots_path(session_id))?;

        let mut current_hash = state_hash(&baseline.state)?;
        let mut latest_valid_step = 0usize;
        let mut latest_checkpoint_step = 0usize;
        let mut fallback_state = baseline.state.clone();
        let snapshot_map = snapshots
            .into_iter()
            .map(|snapshot| (snapshot.step_index, snapshot))
            .collect::<BTreeMap<_, _>>();

        for transition in transitions {
            let post_state = deserialize_state(&transition.serialized_post_ir_state)?;
            let expected_hash = state_hash(&post_state)?;
            if transition.from_state_hash != current_hash
                || transition.to_state_hash != expected_hash
            {
                return Ok(ChainValidation {
                    valid: false,
                    latest_valid_step,
                    latest_checkpoint_step,
                    fallback_state,
                });
            }
            current_hash = expected_hash;
            latest_valid_step = transition.step_index;

            if let Some(snapshot) = snapshot_map.get(&transition.step_index) {
                if snapshot.state_hash != current_hash {
                    return Ok(ChainValidation {
                        valid: false,
                        latest_valid_step: transition.step_index.saturating_sub(1),
                        latest_checkpoint_step,
                        fallback_state,
                    });
                }
                latest_checkpoint_step = snapshot.step_index;
                fallback_state = deserialize_state(&snapshot.serialized_ir_state)?;
            }
        }

        Ok(ChainValidation {
            valid: true,
            latest_valid_step,
            latest_checkpoint_step,
            fallback_state,
        })
    }

    fn initial_state(&self, session_id: String, workspace_root: PathBuf) -> IRState {
        IRState {
            session_id,
            workspace_root,
            current_target: Some(PathBuf::from(".")),
            validation_scope: PathBuf::from("."),
            active_transaction: None,
            next_allowed_actions: vec![
                ActionKind::CodingPreview,
                ActionKind::Analyze,
                ActionKind::Refactor,
            ],
        }
    }

    fn should_checkpoint(&self, step_index: usize, action_kind: ActionKind) -> bool {
        matches!(
            action_kind,
            ActionKind::Apply | ActionKind::Validate | ActionKind::Rollback
        ) || step_index % CHECKPOINT_INTERVAL == 0
    }

    fn dbm_dir(&self) -> PathBuf {
        self.workspace_root.join(".dbm")
    }

    fn ir_dir(&self) -> PathBuf {
        self.dbm_dir().join("ir")
    }

    fn sessions_dir(&self) -> PathBuf {
        self.ir_dir().join("sessions")
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(session_id)
    }

    fn session_record_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("session.json")
    }

    fn transitions_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("ir_transitions.jsonl")
    }

    fn snapshots_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id)
            .join("ir_state_snapshots.jsonl")
    }

    fn artifacts_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id)
            .join("ir_transaction_artifacts.jsonl")
    }

    fn workspace_index_path(&self) -> PathBuf {
        self.ir_dir().join("workspace_index.json")
    }

    fn workspace_key(&self) -> String {
        self.workspace_root.display().to_string()
    }

    fn workspace_index(&self) -> BTreeMap<String, String> {
        self.read_json(&self.workspace_index_path())
            .unwrap_or_default()
    }

    fn update_workspace_index(&self, session_id: &str) -> Result<(), String> {
        let mut index = self.workspace_index();
        index.insert(self.workspace_key(), session_id.to_string());
        self.write_json(&self.workspace_index_path(), &index)
    }

    fn read_session(&self, session_id: &str) -> Result<IRSessionRecord, String> {
        self.read_json(&self.session_record_path(session_id))
    }

    fn read_json<T>(&self, path: &Path) -> Result<T, String>
    where
        T: for<'de> Deserialize<'de>,
    {
        let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
        serde_json::from_str(&raw).map_err(|err| err.to_string())
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let tmp_path = path.with_extension("tmp");
        let body = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
        fs::write(&tmp_path, body).map_err(|err| err.to_string())?;
        fs::rename(tmp_path, path).map_err(|err| err.to_string())
    }

    fn append_jsonl<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| err.to_string())?;
        let line = serde_json::to_string(value).map_err(|err| err.to_string())?;
        writeln!(file, "{line}").map_err(|err| err.to_string())
    }

    fn read_jsonl<T>(&self, path: &Path) -> Result<Vec<T>, String>
    where
        T: for<'de> Deserialize<'de>,
    {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(path).map_err(|err| err.to_string())?;
        let reader = BufReader::new(file);
        let mut items = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|err| err.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            items.push(serde_json::from_str(&line).map_err(|err| err.to_string())?);
        }
        Ok(items)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedCheckpoint {
    pub step_index: usize,
    pub state: IRState,
}

pub fn restore_or_initialize_ir_state(workspace_root: &Path) -> Result<RecoveryOutcome, String> {
    IRPersistenceStore::new(workspace_root).recover_or_create()
}

pub fn persist_ir_transition(
    before: &IRState,
    after: &IRState,
    action_kind: ActionKind,
    input_text: impl Into<String>,
    artifact: IRPersistenceArtifact,
) -> Result<(), String> {
    let workspace_root = if after.workspace_root.as_os_str().is_empty() {
        before.workspace_root.clone()
    } else {
        after.workspace_root.clone()
    };
    IRPersistenceStore::new(workspace_root).record_transition(
        before,
        after,
        action_kind,
        input_text,
        artifact,
    )
}

fn serialize_state(state: &IRState) -> Result<String, String> {
    serde_json::to_string(state).map_err(|err| err.to_string())
}

fn deserialize_state(raw: &str) -> Result<IRState, String> {
    serde_json::from_str(raw).map_err(|err| err.to_string())
}

fn state_hash(state: &IRState) -> Result<String, String> {
    let encoded = serialize_state(state)?;
    Ok(sha256_hex(encoded.as_bytes()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn non_empty_session_id(before: &IRState, after: &IRState) -> Result<String, String> {
    if !after.session_id.is_empty() {
        return Ok(after.session_id.clone());
    }
    if !before.session_id.is_empty() {
        return Ok(before.session_id.clone());
    }
    Err("IR session_id is empty".to_string())
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn new_session_id() -> String {
    format!("ir-{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::nl::session::ConversationState;

    fn store() -> (tempfile::TempDir, IRPersistenceStore, IRState) {
        let dir = tempdir().expect("tempdir");
        let store = IRPersistenceStore::new(dir.path());
        let recovered = store.recover_or_create().expect("recover");
        (dir, store, recovered.state)
    }

    fn apply_transition(
        store: &IRPersistenceStore,
        before: &IRState,
        action: ActionKind,
        mutate: impl FnOnce(&mut ConversationState),
    ) -> IRState {
        let mut conversation = ConversationState {
            ir_state: before.clone(),
            ..ConversationState::default()
        };
        mutate(&mut conversation);
        let after = conversation.ir_state.clone();
        store
            .record_transition(
                before,
                &after,
                action,
                format!("{action:?}"),
                IRPersistenceArtifact::default(),
            )
            .expect("record");
        after
    }

    #[test]
    fn ir_persistence_restores_after_repl_restart() {
        let (_dir, store, baseline) = store();
        let preview = apply_transition(
            &store,
            &baseline,
            ActionKind::CodingPreview,
            |conversation| {
                conversation.start_preview_transaction(PathBuf::from("apps/cli/src/repl.rs"));
            },
        );
        let applied = apply_transition(&store, &preview, ActionKind::Apply, |conversation| {
            conversation.mark_transaction_applied(None);
        });

        let restored = store.load_latest(&applied.session_id).expect("restore");
        assert_eq!(restored.recovered_step, 2);
        assert_eq!(restored.state, applied);
        assert!(restored.state.active_transaction.is_some());
    }

    #[test]
    fn ir_replay_rebuild_matches_live_state() {
        let (_dir, store, baseline) = store();
        let preview = apply_transition(
            &store,
            &baseline,
            ActionKind::CodingPreview,
            |conversation| {
                conversation.start_preview_transaction(PathBuf::from("apps/cli/src/repl.rs"));
            },
        );
        let applied = apply_transition(&store, &preview, ActionKind::Apply, |conversation| {
            conversation.mark_transaction_applied(None);
        });
        let validated = apply_transition(&store, &applied, ActionKind::Validate, |conversation| {
            conversation.mark_transaction_validated();
        });

        let rebuilt = store
            .rebuild_at_step(&validated.session_id, 3)
            .expect("rebuild");
        assert_eq!(rebuilt, validated);
    }

    #[test]
    fn ir_checkpoint_hash_chain_is_valid() {
        let (_dir, store, baseline) = store();
        let preview = apply_transition(
            &store,
            &baseline,
            ActionKind::CodingPreview,
            |conversation| {
                conversation.start_preview_transaction(PathBuf::from("apps/cli/src/repl.rs"));
            },
        );
        let applied = apply_transition(&store, &preview, ActionKind::Apply, |conversation| {
            conversation.mark_transaction_applied(None);
        });
        let validated = apply_transition(&store, &applied, ActionKind::Validate, |conversation| {
            conversation.mark_transaction_validated();
        });
        let validation = store
            .validate_hash_chain(&validated.session_id)
            .expect("validate");
        assert!(validation.valid);
        assert_eq!(validation.latest_checkpoint_step, 3);
    }

    #[test]
    fn crash_recovery_restores_previous_valid_snapshot() {
        let (_dir, store, baseline) = store();
        let preview = apply_transition(
            &store,
            &baseline,
            ActionKind::CodingPreview,
            |conversation| {
                conversation.start_preview_transaction(PathBuf::from("apps/cli/src/repl.rs"));
            },
        );
        let applied = apply_transition(&store, &preview, ActionKind::Apply, |conversation| {
            conversation.mark_transaction_applied(None);
        });
        let validated = apply_transition(&store, &applied, ActionKind::Validate, |conversation| {
            conversation.mark_transaction_validated();
        });

        let path = store.transitions_path(&validated.session_id);
        let mut transitions = store
            .read_jsonl::<IRTransitionRecord>(&path)
            .expect("read transitions");
        let last = transitions.last_mut().expect("last");
        last.to_state_hash = "corrupted".to_string();
        fs::remove_file(&path).expect("remove");
        for transition in transitions {
            store.append_jsonl(&path, &transition).expect("rewrite");
        }

        let restored = store.load_latest(&validated.session_id).expect("restore");
        assert!(restored.fallback_used);
        assert_eq!(restored.recovered_step, 2);
        assert_eq!(restored.state, applied);
    }

    #[test]
    fn replay_export_contains_transaction_lifecycle() {
        let (_dir, store, baseline) = store();
        let preview = apply_transition(
            &store,
            &baseline,
            ActionKind::CodingPreview,
            |conversation| {
                conversation.start_preview_transaction(PathBuf::from("apps/cli/src/repl.rs"));
            },
        );
        let applied = apply_transition(&store, &preview, ActionKind::Apply, |conversation| {
            conversation.mark_transaction_applied(None);
        });
        let validated = apply_transition(&store, &applied, ActionKind::Validate, |conversation| {
            conversation.mark_transaction_validated();
        });
        let rolled_back =
            apply_transition(&store, &validated, ActionKind::Rollback, |conversation| {
                conversation.rollback_current_transaction();
            });

        let entries = store
            .export_timeline(&rolled_back.session_id)
            .expect("export");
        let actions = entries
            .iter()
            .map(|entry| entry.action.clone())
            .collect::<Vec<_>>();
        assert!(actions.iter().any(|action| action == "Apply"));
        assert!(actions.iter().any(|action| action == "Validate"));
        assert!(actions.iter().any(|action| action == "Rollback"));
    }
}
