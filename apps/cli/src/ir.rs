use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::trace;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::control_event::{DecisionAction, DecisionSource, RequestId};
use crate::nl::types::{CodingIntent, CodingOptions, CommandPlan, PlannedStep};
use crate::plan::Plan;
use crate::service::dto::{ActionKind, IRState, SessionAppliedDiff};

const CHECKPOINT_INTERVAL: usize = 5;
const DEFAULT_MEMORY_TOP_K: usize = 5;
const EMBEDDING_DIMENSION: usize = 8;
const SYMBOLIC_WEIGHT: f32 = 1.0;
const EMBEDDING_WEIGHT: f32 = 1.0;
const MEMORY_COUNT_SCALE: u32 = 2;
const MEMORY_MAX_COUNT: u32 = 100;
const MEMORY_MAX_COUNT_SCALED: u32 = MEMORY_MAX_COUNT * MEMORY_COUNT_SCALE;

type Embedding = Vec<f32>;

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    System,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentCapturedPayload {
    pub input_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<CodingIntent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanPayload {
    pub plan_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<CodingIntent>,
    pub steps: Vec<PlanStepRecord>,
    pub planner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStepRecord {
    pub kind: String,
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanAcceptedPayload {
    pub plan_id: Uuid,
    pub accepted_by: ActorType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanRejectedPayload {
    pub plan_id: Uuid,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_kind", content = "payload", rename_all = "snake_case")]
pub enum IRPlanEventPayload {
    IntentCaptured(IntentCapturedPayload),
    PlanProposed(PlanPayload),
    PlanAccepted(PlanAcceptedPayload),
    PlanRejected(PlanRejectedPayload),
}

// ── Control Event IR Payload Types ───────────────────────────────────────────

/// Recorded when the Executor emits a Control Event to an Agent (§8.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlEventEmittedPayload {
    /// Step that triggered the control event.
    pub step_id: String,
    /// Unique per-request identifier matching the emitted [`ControlEvent`].
    pub request_id: RequestId,
    /// `"decision_required"` | `"input_required"` | `"approval_required"`.
    pub event_kind: String,
    /// Full serialised [`ControlEvent`] for audit and replay (§11).
    pub event: serde_json::Value,
}

/// Recorded when the Executor receives an Agent response and resumes (§8.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlEventResolvedPayload {
    pub step_id: String,
    pub request_id: RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<DecisionAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub source: DecisionSource,
}

// ── Phase 2: Step Lifecycle Event Types ──────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Failure,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRef {
    pub artifact_kind: String,
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    ExecutionTrace,
    Artifact,
    SemanticHint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryMetadata {
    pub timestamp: u64,
    pub step_index: usize,
    pub relevance: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEntry {
    pub memory_id: String,
    pub source_event: Uuid,
    pub memory_type: MemoryType,
    pub content: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Embedding>,
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub failure_count: u32,
    pub metadata: MemoryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryReferencePayload {
    pub step_id: Uuid,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryStorePayload {
    pub step_id: Uuid,
    pub step_index: usize,
    pub memory_id: String,
    pub entry: MemoryEntry,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOutcome {
    CompileSuccess,
    CompileWithWarnings,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryOutcomePayload {
    pub step_id: Uuid,
    pub memory_id: String,
    pub outcome: MemoryOutcome,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemoryContext {
    pub entries: Vec<MemoryEntry>,
}

impl MemoryContext {
    pub fn ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.memory_id.clone())
            .collect()
    }
}

pub trait MemoryStore {
    fn query(&self, step: &PlannedStep, step_index: usize) -> Result<MemoryContext, String>;
    fn store(
        &self,
        step: &PlannedStep,
        step_index: usize,
        result: &StepExecutionResultPayload,
    ) -> Result<MemoryEntry, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepScheduledPayload {
    pub step_id: Uuid,
    pub plan_id: Uuid,
    pub step_index: usize,
    pub step_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepStartedPayload {
    pub step_id: Uuid,
    pub started_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepCompletedPayload {
    pub step_id: Uuid,
    pub completed_at: u64,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepExecutionResultPayload {
    pub step_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_kind", content = "payload", rename_all = "snake_case")]
pub enum IRExecutionEventPayload {
    StepScheduled(StepScheduledPayload),
    StepStarted(StepStartedPayload),
    StepCompleted(StepCompletedPayload),
    ExecutionResultRecorded(StepExecutionResultPayload),
    ArtifactProduced(ArtifactRef),
    ArtifactApplied(ArtifactRef),
    ArtifactRolledBack(ArtifactRef),
    MemoryReferenced(MemoryReferencePayload),
    MemoryStored(MemoryStorePayload),
    MemoryOutcomeRecorded(MemoryOutcomePayload),
    /// Executor blocked — Control Event emitted to Agent (§8.1).
    ControlEventEmitted(ControlEventEmittedPayload),
    /// Executor resumed — Agent response received and validated (§8.2).
    ControlEventResolved(ControlEventResolvedPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IRExecutionEventRecord {
    pub event_id: String,
    pub session_id: String,
    pub event_index: usize,
    pub timestamp: u64,
    pub payload: IRExecutionEventPayload,
}

/// IR-projected execution state reconstructed from execution events.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecutionState {
    pub active_plan: Option<Uuid>,
    pub active_step: Option<Uuid>,
    pub last_result: Option<StepExecutionResultPayload>,
    pub artifacts: Vec<ArtifactRef>,
    pub memory_context: Vec<MemoryEntry>,
}

struct IrBackedMemoryStore<'a> {
    store: &'a IRPersistenceStore,
    session_id: &'a str,
}

impl MemoryStore for IrBackedMemoryStore<'_> {
    fn query(&self, step: &PlannedStep, step_index: usize) -> Result<MemoryContext, String> {
        let events = self.store.list_execution_events(self.session_id)?;
        let query_tags = memory_tags_for_step(step);
        let query_embedding = self.store.query_embedding(step, step_index, &query_tags);
        let mut recalled = stored_memories_from_events(&events)
            .into_values()
            .filter_map(|entry| {
                if entry.metadata.step_index >= step_index {
                    return None;
                }
                let overlap = tag_overlap(&query_tags, &entry.metadata.tags);
                if overlap <= 0.0 {
                    return None;
                }
                let age_steps = step_index.saturating_sub(entry.metadata.step_index);
                let embedding = self.store.embedding_for_entry(&entry);
                Some((
                    memory_score(
                        &entry,
                        overlap,
                        age_steps,
                        Some(&query_embedding),
                        Some(&embedding),
                    ),
                    entry,
                ))
            })
            .collect::<Vec<_>>();
        recalled.sort_by(|(lhs_score, lhs_entry), (rhs_score, rhs_entry)| {
            rhs_score
                .partial_cmp(lhs_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    lhs_entry
                        .metadata
                        .timestamp
                        .cmp(&rhs_entry.metadata.timestamp)
                })
                .then_with(|| lhs_entry.memory_id.cmp(&rhs_entry.memory_id))
        });
        Ok(MemoryContext {
            entries: recalled
                .into_iter()
                .map(|(_, entry)| entry)
                .take(DEFAULT_MEMORY_TOP_K)
                .collect(),
        })
    }

    fn store(
        &self,
        step: &PlannedStep,
        step_index: usize,
        result: &StepExecutionResultPayload,
    ) -> Result<MemoryEntry, String> {
        let step_kind = memory_step_kind(step);
        let content = serde_json::json!({
            "step_kind": step_kind,
            "stdout": result.stdout.clone(),
            "stderr": result.stderr.clone(),
            "structured_output": result.structured_output.clone(),
            "artifacts": result.artifacts.clone(),
        });
        let content_hash = sha256_hex(
            serde_json::to_string(&content)
                .map_err(|err| err.to_string())?
                .as_bytes(),
        );
        let embedding_input = format!(
            "{}\n{}",
            step_kind,
            serde_json::to_string(&content).map_err(|err| err.to_string())?
        );
        Ok(MemoryEntry {
            memory_id: format!("mem:{}:{content_hash}", result.step_id),
            source_event: result.step_id,
            memory_type: if result.artifacts.is_empty() {
                MemoryType::ExecutionTrace
            } else {
                MemoryType::Artifact
            },
            content,
            embedding: Some(deterministic_embedding(&embedding_input)),
            success_count: 0,
            failure_count: 0,
            metadata: MemoryMetadata {
                timestamp: now_ts(),
                step_index,
                relevance: 1.0,
                tags: memory_tags_for_step(step),
            },
        })
    }
}

/// Per-step lifecycle state derived from the execution event log.
/// Used internally by `IRPersistenceStore` to enforce single-execution semantics.
#[derive(Debug, Default, Clone, Copy)]
struct StepState {
    scheduled: bool,
    started: bool,
    completed: bool,
}

/// In-memory cache of step states for the current `IRPersistenceStore` instance.
///
/// Eliminates repeated O(N) log scans within a single store lifetime.
/// The event log remains authoritative; this cache is derived, not canonical.
///
/// Uses `std::sync::Mutex` so the store can be shared across threads safely.
/// Lock scope is kept minimal: the lock is never held during I/O or log scans.
#[derive(Debug)]
struct StepStateCache {
    map: Mutex<HashMap<Uuid, StepState>>,
}

impl StepStateCache {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    /// Return the cached state for `step_id`, or `None` if not yet seen.
    /// Lock is acquired and immediately released.
    fn get(&self, step_id: Uuid) -> Option<StepState> {
        self.map
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&step_id)
            .copied()
    }

    fn set_scheduled(&self, step_id: Uuid) {
        self.map
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(step_id)
            .or_default()
            .scheduled = true;
    }

    fn set_started(&self, step_id: Uuid) {
        self.map
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(step_id)
            .or_default()
            .started = true;
    }

    fn set_completed(&self, step_id: Uuid) {
        self.map
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(step_id)
            .or_default()
            .completed = true;
    }

    /// Populate (or refresh) the cache from a full event list.
    /// Acquires the lock for the duration of the update only (not during I/O).
    fn populate_from_events(&self, events: &[IRExecutionEventRecord]) {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        for e in events {
            match &e.payload {
                IRExecutionEventPayload::StepScheduled(p) => {
                    map.entry(p.step_id).or_default().scheduled = true;
                }
                IRExecutionEventPayload::StepStarted(p) => {
                    map.entry(p.step_id).or_default().started = true;
                }
                IRExecutionEventPayload::StepCompleted(p) => {
                    map.entry(p.step_id).or_default().completed = true;
                }
                _ => {}
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IRPlanEventRecord {
    pub event_id: String,
    pub session_id: String,
    pub event_index: usize,
    pub timestamp: u64,
    pub payload: IRPlanEventPayload,
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

#[derive(Debug)]
pub struct IRPersistenceStore {
    workspace_root: PathBuf,
    /// Optional caching layer for step lifecycle state.
    /// The event log remains authoritative; this cache is derived, not canonical.
    cache: StepStateCache,
    embedding_cache: Mutex<HashMap<String, Embedding>>,
    query_embedding_cache: Mutex<HashMap<String, Embedding>>,
}

impl Clone for IRPersistenceStore {
    /// Clone the store. The cache is intentionally reset to empty:
    /// it is derived data and will be repopulated on first access.
    fn clone(&self) -> Self {
        Self {
            workspace_root: self.workspace_root.clone(),
            cache: StepStateCache::new(),
            embedding_cache: Mutex::new(HashMap::new()),
            query_embedding_cache: Mutex::new(HashMap::new()),
        }
    }
}

impl IRPersistenceStore {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            cache: StepStateCache::new(),
            embedding_cache: Mutex::new(HashMap::new()),
            query_embedding_cache: Mutex::new(HashMap::new()),
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

    fn embedding_for_entry(&self, entry: &MemoryEntry) -> Embedding {
        if let Some(embedding) = &entry.embedding {
            return embedding.clone();
        }
        let mut cache = self
            .embedding_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(embedding) = cache.get(&entry.memory_id) {
            return embedding.clone();
        }
        let derived = deterministic_embedding(
            &serde_json::to_string(&entry.content).unwrap_or_else(|_| String::new()),
        );
        cache.insert(entry.memory_id.clone(), derived.clone());
        derived
    }

    fn query_embedding(
        &self,
        step: &PlannedStep,
        step_index: usize,
        query_tags: &[String],
    ) -> Embedding {
        let cache_key = format!("{}:{step_index}", memory_query_key(step));
        let mut cache = self
            .query_embedding_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(embedding) = cache.get(&cache_key) {
            return embedding.clone();
        }
        let derived = embed_query(step, query_tags);
        cache.insert(cache_key, derived.clone());
        derived
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

    pub fn emit_intent_captured(
        &self,
        session_id: &str,
        input_text: impl Into<String>,
        intent: Option<CodingIntent>,
    ) -> Result<String, String> {
        let payload = IRPlanEventPayload::IntentCaptured(IntentCapturedPayload {
            input_text: input_text.into(),
            intent,
        });
        self.append_plan_event(session_id, payload)
    }

    pub fn emit_plan_proposed(
        &self,
        session_id: &str,
        plan: CommandPlan,
        planner: impl Into<String>,
    ) -> Result<Uuid, String> {
        self.emit_plan_proposed_with_id(session_id, Uuid::new_v4(), plan, planner)
    }

    pub fn emit_plan_proposed_with_id(
        &self,
        session_id: &str,
        plan_id: Uuid,
        plan: CommandPlan,
        planner: impl Into<String>,
    ) -> Result<Uuid, String> {
        let payload = PlanPayload {
            plan_id,
            intent: plan.intent.clone(),
            steps: plan_steps_from_command_plan(&plan),
            planner: planner.into(),
        };
        self.append_plan_event(
            session_id,
            IRPlanEventPayload::PlanProposed(payload.clone()),
        )?;
        Ok(payload.plan_id)
    }

    pub fn emit_runtime_plan_proposed(
        &self,
        session_id: &str,
        plan: &Plan,
        planner: impl Into<String>,
    ) -> Result<Uuid, String> {
        let payload = PlanPayload {
            plan_id: Uuid::new_v4(),
            intent: None,
            steps: plan_steps_from_runtime_plan(plan),
            planner: planner.into(),
        };
        self.append_plan_event(
            session_id,
            IRPlanEventPayload::PlanProposed(payload.clone()),
        )?;
        Ok(payload.plan_id)
    }

    pub fn emit_plan_accepted(
        &self,
        session_id: &str,
        plan_id: Uuid,
        accepted_by: ActorType,
    ) -> Result<String, String> {
        if self.load_plan(session_id, plan_id)?.is_none() {
            return Err(format!(
                "plan {plan_id} was not proposed in session {session_id}"
            ));
        }
        self.append_plan_event(
            session_id,
            IRPlanEventPayload::PlanAccepted(PlanAcceptedPayload {
                plan_id,
                accepted_by,
            }),
        )
    }

    pub fn emit_plan_rejected(
        &self,
        session_id: &str,
        plan_id: Uuid,
        reason: Option<String>,
    ) -> Result<String, String> {
        if self.load_plan(session_id, plan_id)?.is_none() {
            return Err(format!(
                "plan {plan_id} was not proposed in session {session_id}"
            ));
        }
        self.append_plan_event(
            session_id,
            IRPlanEventPayload::PlanRejected(PlanRejectedPayload { plan_id, reason }),
        )
    }

    pub fn list_plan_events(&self, session_id: &str) -> Result<Vec<IRPlanEventRecord>, String> {
        self.read_jsonl(&self.plan_events_path(session_id))
    }

    pub fn list_transitions(&self, session_id: &str) -> Result<Vec<IRTransitionRecord>, String> {
        self.read_jsonl(&self.transitions_path(session_id))
    }

    pub fn list_transaction_artifacts(
        &self,
        session_id: &str,
    ) -> Result<Vec<IRTransactionArtifactRecord>, String> {
        self.read_jsonl(&self.artifacts_path(session_id))
    }

    pub fn list_plans(&self, session_id: &str) -> Result<Vec<PlanPayload>, String> {
        Ok(self
            .list_plan_events(session_id)?
            .into_iter()
            .filter_map(|record| match record.payload {
                IRPlanEventPayload::PlanProposed(payload) => Some(payload),
                _ => None,
            })
            .collect())
    }

    pub fn accepted_plan_ids(&self, session_id: &str) -> Result<Vec<Uuid>, String> {
        Ok(self
            .list_plan_events(session_id)?
            .into_iter()
            .filter_map(|record| match record.payload {
                IRPlanEventPayload::PlanAccepted(payload) => Some(payload.plan_id),
                _ => None,
            })
            .collect())
    }

    pub fn load_plan(
        &self,
        session_id: &str,
        plan_id: Uuid,
    ) -> Result<Option<PlanPayload>, String> {
        Ok(self
            .list_plans(session_id)?
            .into_iter()
            .find(|payload| payload.plan_id == plan_id))
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
        ) || step_index.is_multiple_of(CHECKPOINT_INTERVAL)
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

    fn plan_events_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("plan_events.jsonl")
    }

    fn execution_events_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("execution_events.jsonl")
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

    fn append_plan_event(
        &self,
        session_id: &str,
        payload: IRPlanEventPayload,
    ) -> Result<String, String> {
        let _ = self.read_session(session_id)?;
        let event_index = self
            .read_jsonl::<IRPlanEventRecord>(&self.plan_events_path(session_id))?
            .len()
            + 1;
        let event_id = format!("pe:{session_id}:{event_index}");
        self.append_jsonl(
            &self.plan_events_path(session_id),
            &IRPlanEventRecord {
                event_id: event_id.clone(),
                session_id: session_id.to_string(),
                event_index,
                timestamp: now_ts(),
                payload,
            },
        )?;
        Ok(event_id)
    }

    fn append_execution_event(
        &self,
        session_id: &str,
        payload: IRExecutionEventPayload,
    ) -> Result<String, String> {
        let _ = self.read_session(session_id)?;
        let event_index = self
            .read_jsonl::<IRExecutionEventRecord>(&self.execution_events_path(session_id))?
            .len()
            + 1;
        let event_id = format!("ee:{session_id}:{event_index}");
        self.append_jsonl(
            &self.execution_events_path(session_id),
            &IRExecutionEventRecord {
                event_id: event_id.clone(),
                session_id: session_id.to_string(),
                event_index,
                timestamp: now_ts(),
                payload,
            },
        )?;
        Ok(event_id)
    }

    // ── Phase 2: Step Lifecycle Emit Methods ─────────────────────────────────

    /// Derive the lifecycle state of a single step.
    ///
    /// Returns the cached state on a hit (O(1)).
    /// On a miss, scans the event log (O(N)), populates the cache from all events,
    /// then returns the state for `step_id`.
    fn step_state(&self, session_id: &str, step_id: Uuid) -> Result<StepState, String> {
        // Cache hit: the entry is present and `scheduled` being set means the cache
        // was populated for this step (every scheduled step reaches the cache).
        if let Some(state) = self.cache.get(step_id)
            && state.scheduled
        {
            return Ok(state);
        }
        // Cache miss: read the full log and warm the cache for all steps at once.
        let events =
            self.read_jsonl::<IRExecutionEventRecord>(&self.execution_events_path(session_id))?;
        self.cache.populate_from_events(&events);
        Ok(self.cache.get(step_id).unwrap_or_default())
    }

    pub fn emit_step_scheduled(
        &self,
        session_id: &str,
        plan_id: Uuid,
        step_index: usize,
        step_kind: impl Into<String>,
    ) -> Result<Uuid, String> {
        let step_id = Uuid::new_v4();
        let step_kind = step_kind.into();
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::StepScheduled(StepScheduledPayload {
                step_id,
                plan_id,
                step_index,
                step_kind: step_kind.clone(),
            }),
        )?;
        self.cache.set_scheduled(step_id);
        trace!("[TRACE] StepScheduled step_id={step_id} kind={step_kind} index={step_index}");
        Ok(step_id)
    }

    pub fn emit_step_scheduled_with_id(
        &self,
        session_id: &str,
        step_id: Uuid,
        plan_id: Uuid,
        step_index: usize,
        step_kind: impl Into<String>,
    ) -> Result<Uuid, String> {
        let step_kind = step_kind.into();
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::StepScheduled(StepScheduledPayload {
                step_id,
                plan_id,
                step_index,
                step_kind: step_kind.clone(),
            }),
        )?;
        self.cache.set_scheduled(step_id);
        trace!("[TRACE] StepScheduled step_id={step_id} kind={step_kind} index={step_index}");
        Ok(step_id)
    }

    /// Emit StepStarted.
    ///
    /// Returns `Err` if:
    /// - The step was never scheduled (`StepScheduled` not present)
    /// - The step was already started (`StepStarted` already present)
    pub fn emit_step_started(&self, session_id: &str, step_id: Uuid) -> Result<String, String> {
        let s = self.step_state(session_id, step_id)?;
        if !s.scheduled {
            return Err(format!(
                "IR violation: step {step_id} was never scheduled — emit_step_scheduled must precede emit_step_started"
            ));
        }
        if s.started {
            return Err(format!(
                "IR violation: step {step_id} was already started — each step may be started exactly once"
            ));
        }
        let event_id = self.append_execution_event(
            session_id,
            IRExecutionEventPayload::StepStarted(StepStartedPayload {
                step_id,
                started_at: now_ts(),
            }),
        )?;
        self.cache.set_started(step_id);
        trace!("[TRACE] StepStarted step_id={step_id}");
        Ok(event_id)
    }

    /// Emit StepCompleted.
    ///
    /// Returns `Err` if:
    /// - The step was never started (`StepStarted` not present)
    /// - The step was already completed (`StepCompleted` already present)
    pub fn emit_step_completed(
        &self,
        session_id: &str,
        step_id: Uuid,
        status: ExecutionStatus,
    ) -> Result<String, String> {
        let s = self.step_state(session_id, step_id)?;
        if !s.started {
            return Err(format!(
                "IR violation: step {step_id} was never started — emit_step_started must precede emit_step_completed"
            ));
        }
        if s.completed {
            return Err(format!(
                "IR violation: step {step_id} was already completed — each step may be completed exactly once"
            ));
        }
        let event_id = self.append_execution_event(
            session_id,
            IRExecutionEventPayload::StepCompleted(StepCompletedPayload {
                step_id,
                completed_at: now_ts(),
                status,
            }),
        )?;
        self.cache.set_completed(step_id);
        trace!("[TRACE] StepCompleted step_id={step_id} status={status:?}");
        Ok(event_id)
    }

    pub fn emit_execution_result(
        &self,
        session_id: &str,
        result: StepExecutionResultPayload,
    ) -> Result<String, String> {
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::ExecutionResultRecorded(result),
        )
    }

    pub fn emit_artifact_produced(
        &self,
        session_id: &str,
        artifact: ArtifactRef,
    ) -> Result<String, String> {
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::ArtifactProduced(artifact),
        )
    }

    /// Emit ArtifactApplied.
    ///
    /// Returns `Err` if `ArtifactProduced` for the same `artifact_id` was never emitted.
    pub fn emit_artifact_applied(
        &self,
        session_id: &str,
        artifact: ArtifactRef,
    ) -> Result<String, String> {
        let events =
            self.read_jsonl::<IRExecutionEventRecord>(&self.execution_events_path(session_id))?;
        let produced = events.iter().any(|e| {
            matches!(
                &e.payload,
                IRExecutionEventPayload::ArtifactProduced(a) if a.artifact_id == artifact.artifact_id
            )
        });
        if !produced {
            return Err(format!(
                "IR violation: artifact '{}' was never produced — ArtifactProduced must precede ArtifactApplied",
                artifact.artifact_id
            ));
        }
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::ArtifactApplied(artifact),
        )
    }

    pub fn emit_artifact_rolled_back(
        &self,
        session_id: &str,
        artifact: ArtifactRef,
    ) -> Result<String, String> {
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::ArtifactRolledBack(artifact),
        )
    }

    pub fn query_memory_context(
        &self,
        session_id: &str,
        step: &PlannedStep,
        step_index: usize,
    ) -> Result<MemoryContext, String> {
        IrBackedMemoryStore {
            store: self,
            session_id,
        }
        .query(step, step_index)
    }

    pub fn emit_memory_referenced(
        &self,
        session_id: &str,
        step_id: Uuid,
        references: Vec<String>,
    ) -> Result<String, String> {
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::MemoryReferenced(MemoryReferencePayload {
                step_id,
                references,
            }),
        )
    }

    pub fn emit_memory_stored(
        &self,
        session_id: &str,
        step_id: Uuid,
        step_index: usize,
        entry: MemoryEntry,
    ) -> Result<String, String> {
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::MemoryStored(MemoryStorePayload {
                step_id,
                step_index,
                memory_id: entry.memory_id.clone(),
                entry,
            }),
        )
    }

    pub fn emit_memory_outcome(
        &self,
        session_id: &str,
        step_id: Uuid,
        memory_id: impl Into<String>,
        outcome: MemoryOutcome,
    ) -> Result<String, String> {
        let memory_id = memory_id.into();
        let events = self.list_execution_events(session_id)?;
        let stored = stored_memories_from_events(&events);
        if !stored.contains_key(&memory_id) {
            return Err(format!(
                "IR violation: memory '{memory_id}' was never stored — MemoryStored must precede MemoryOutcomeRecorded"
            ));
        }
        self.append_execution_event(
            session_id,
            IRExecutionEventPayload::MemoryOutcomeRecorded(MemoryOutcomePayload {
                step_id,
                memory_id,
                outcome,
            }),
        )
    }

    pub fn store_execution_memory(
        &self,
        session_id: &str,
        step: &PlannedStep,
        step_index: usize,
        result: &StepExecutionResultPayload,
    ) -> Result<MemoryEntry, String> {
        let memory = IrBackedMemoryStore {
            store: self,
            session_id,
        }
        .store(step, step_index, result)?;
        self.emit_memory_stored(session_id, result.step_id, step_index, memory.clone())?;
        Ok(memory)
    }

    pub fn list_execution_events(
        &self,
        session_id: &str,
    ) -> Result<Vec<IRExecutionEventRecord>, String> {
        self.read_jsonl(&self.execution_events_path(session_id))
    }

    /// Reconstruct current execution state by replaying execution events.
    pub fn project_execution_state(&self, session_id: &str) -> Result<ExecutionState, String> {
        let events = self.list_execution_events(session_id)?;
        let mut state = ExecutionState::default();
        let mut stored_memories = HashMap::new();
        for record in events {
            match record.payload {
                IRExecutionEventPayload::StepScheduled(p) => {
                    state.active_plan = Some(p.plan_id);
                    state.active_step = Some(p.step_id);
                }
                IRExecutionEventPayload::StepCompleted(_) => {
                    state.active_step = None;
                }
                IRExecutionEventPayload::ExecutionResultRecorded(result) => {
                    state.last_result = Some(result);
                }
                IRExecutionEventPayload::ArtifactApplied(artifact) => {
                    state.artifacts.push(artifact);
                }
                IRExecutionEventPayload::ArtifactRolledBack(artifact) => {
                    state
                        .artifacts
                        .retain(|a| a.artifact_id != artifact.artifact_id);
                }
                IRExecutionEventPayload::MemoryStored(payload) => {
                    stored_memories.insert(payload.memory_id, payload.entry);
                }
                IRExecutionEventPayload::MemoryOutcomeRecorded(payload) => {
                    if let Some(entry) = stored_memories.get_mut(&payload.memory_id) {
                        apply_memory_outcome(entry, payload.outcome);
                    }
                    if let Some(entry) = state
                        .memory_context
                        .iter_mut()
                        .find(|entry| entry.memory_id == payload.memory_id)
                    {
                        apply_memory_outcome(entry, payload.outcome);
                    }
                }
                IRExecutionEventPayload::MemoryReferenced(payload) => {
                    state.memory_context = payload
                        .references
                        .iter()
                        .filter_map(|memory_id| stored_memories.get(memory_id).cloned())
                        .collect();
                }
                _ => {}
            }
        }
        Ok(state)
    }

    /// Validate that `ExecutionResultRecorded` exists for a given step.
    ///
    /// Returns `Ok(())` if the result is present, `Err` otherwise.
    /// Callers can use this to assert completeness after `emit_step_completed`.
    pub fn assert_execution_result_exists(
        &self,
        session_id: &str,
        step_id: Uuid,
    ) -> Result<(), String> {
        let events = self.list_execution_events(session_id)?;
        let found = events.iter().any(|e| {
            matches!(
                &e.payload,
                IRExecutionEventPayload::ExecutionResultRecorded(r) if r.step_id == step_id
            )
        });
        if found {
            Ok(())
        } else {
            Err(format!(
                "IR consistency error: step {step_id} is missing ExecutionResultRecorded"
            ))
        }
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

pub fn emit_intent_captured(
    ir_state: &IRState,
    input_text: impl Into<String>,
    intent: Option<CodingIntent>,
) -> Result<String, String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    IRPersistenceStore::new(ir_state.workspace_root.clone()).emit_intent_captured(
        &session_id,
        input_text,
        intent,
    )
}

pub fn emit_plan_proposed(
    ir_state: &IRState,
    plan: CommandPlan,
    planner: impl Into<String>,
) -> Result<Uuid, String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    IRPersistenceStore::new(ir_state.workspace_root.clone()).emit_plan_proposed(
        &session_id,
        plan,
        planner,
    )
}

pub fn emit_plan_proposed_with_id(
    ir_state: &IRState,
    plan_id: Uuid,
    plan: CommandPlan,
    planner: impl Into<String>,
) -> Result<Uuid, String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    IRPersistenceStore::new(ir_state.workspace_root.clone()).emit_plan_proposed_with_id(
        &session_id,
        plan_id,
        plan,
        planner,
    )
}

pub fn emit_runtime_plan_proposed(
    ir_state: &IRState,
    plan: &Plan,
    planner: impl Into<String>,
) -> Result<Uuid, String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    IRPersistenceStore::new(ir_state.workspace_root.clone()).emit_runtime_plan_proposed(
        &session_id,
        plan,
        planner,
    )
}

pub fn emit_plan_accepted(ir_state: &IRState, plan_id: Uuid) -> Result<String, String> {
    emit_plan_accepted_by(ir_state, plan_id, ActorType::System)
}

pub fn emit_plan_accepted_by(
    ir_state: &IRState,
    plan_id: Uuid,
    accepted_by: ActorType,
) -> Result<String, String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    IRPersistenceStore::new(ir_state.workspace_root.clone()).emit_plan_accepted(
        &session_id,
        plan_id,
        accepted_by,
    )
}

pub fn emit_plan_rejected(
    ir_state: &IRState,
    plan_id: Uuid,
    reason: Option<String>,
) -> Result<String, String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    IRPersistenceStore::new(ir_state.workspace_root.clone()).emit_plan_rejected(
        &session_id,
        plan_id,
        reason,
    )
}

// ── Phase 2: Module-level step lifecycle helpers ──────────────────────────────

fn execution_store(ir_state: &IRState) -> Result<(IRPersistenceStore, String), String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    let store = IRPersistenceStore::new(ir_state.workspace_root.clone());
    Ok((store, session_id))
}

pub fn emit_step_scheduled(
    ir_state: &IRState,
    plan_id: Uuid,
    step_index: usize,
    step_kind: impl Into<String>,
) -> Result<Uuid, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_step_scheduled(&session_id, plan_id, step_index, step_kind)
}

pub fn emit_step_scheduled_with_id(
    ir_state: &IRState,
    step_id: Uuid,
    plan_id: Uuid,
    step_index: usize,
    step_kind: impl Into<String>,
) -> Result<Uuid, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_step_scheduled_with_id(&session_id, step_id, plan_id, step_index, step_kind)
}

pub fn emit_step_started(ir_state: &IRState, step_id: Uuid) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_step_started(&session_id, step_id)
}

pub fn emit_step_completed(
    ir_state: &IRState,
    step_id: Uuid,
    status: ExecutionStatus,
) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_step_completed(&session_id, step_id, status)
}

pub fn emit_execution_result(
    ir_state: &IRState,
    result: StepExecutionResultPayload,
) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_execution_result(&session_id, result)
}

pub fn emit_artifact_produced(ir_state: &IRState, artifact: ArtifactRef) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_artifact_produced(&session_id, artifact)
}

pub fn emit_artifact_applied(ir_state: &IRState, artifact: ArtifactRef) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_artifact_applied(&session_id, artifact)
}

pub fn emit_artifact_rolled_back(
    ir_state: &IRState,
    artifact: ArtifactRef,
) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_artifact_rolled_back(&session_id, artifact)
}

pub fn query_memory_context(
    ir_state: &IRState,
    step: &PlannedStep,
    step_index: usize,
) -> Result<MemoryContext, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.query_memory_context(&session_id, step, step_index)
}

pub fn emit_memory_referenced(
    ir_state: &IRState,
    step_id: Uuid,
    references: Vec<String>,
) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_memory_referenced(&session_id, step_id, references)
}

pub fn emit_memory_outcome(
    ir_state: &IRState,
    step_id: Uuid,
    memory_id: impl Into<String>,
    outcome: MemoryOutcome,
) -> Result<String, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.emit_memory_outcome(&session_id, step_id, memory_id, outcome)
}

pub fn store_execution_memory(
    ir_state: &IRState,
    step: &PlannedStep,
    step_index: usize,
    result: &StepExecutionResultPayload,
) -> Result<MemoryEntry, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.store_execution_memory(&session_id, step, step_index, result)
}

pub fn project_execution_state(ir_state: &IRState) -> Result<ExecutionState, String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.project_execution_state(&session_id)
}

pub fn assert_execution_result_exists(ir_state: &IRState, step_id: Uuid) -> Result<(), String> {
    let (store, session_id) = execution_store(ir_state)?;
    store.assert_execution_result_exists(&session_id, step_id)
}

// ─────────────────────────────────────────────────────────────────────────────

pub fn assert_accepted_plan_exists(ir_state: &IRState, plan_id: Uuid) -> Result<(), String> {
    let session_id = non_empty_session_id(ir_state, ir_state)?;
    let store = IRPersistenceStore::new(ir_state.workspace_root.clone());
    if store.load_plan(&session_id, plan_id)?.is_none() {
        return Err(format!(
            "IR bypass detected: plan {plan_id} was never proposed in session {session_id}"
        ));
    }
    if !store.accepted_plan_ids(&session_id)?.contains(&plan_id) {
        return Err(format!(
            "IR bypass detected: plan {plan_id} was never accepted in session {session_id}"
        ));
    }
    Ok(())
}

pub fn log_ir_bypass_warning(reason: &str) {
    eprintln!("[WARN] IR bypass detected: {reason}");
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

fn plan_steps_from_command_plan(plan: &CommandPlan) -> Vec<PlanStepRecord> {
    plan.steps.iter().map(plan_step_from_planned_step).collect()
}

fn plan_step_from_planned_step(step: &PlannedStep) -> PlanStepRecord {
    match step {
        PlannedStep::Analyze(path) => simple_plan_step("analyze", Some(path.clone()), Vec::new()),
        PlannedStep::Coding(path, options) => {
            plan_step_with_coding_options("coding", path.clone(), options)
        }
        PlannedStep::Validate(path) => simple_plan_step("validate", Some(path.clone()), Vec::new()),
        PlannedStep::StructureView(path) => {
            simple_plan_step("structure_view", Some(path.clone()), Vec::new())
        }
        PlannedStep::StructureEdit(path) => {
            simple_plan_step("structure_edit", Some(path.clone()), Vec::new())
        }
        PlannedStep::StructureDiff(path, node) => simple_plan_step(
            "structure_diff",
            Some(path.clone()),
            node.iter().cloned().collect(),
        ),
        PlannedStep::StructureUndo(path) => {
            simple_plan_step("structure_undo", Some(path.clone()), Vec::new())
        }
        PlannedStep::StructureRedo(path) => {
            simple_plan_step("structure_redo", Some(path.clone()), Vec::new())
        }
        PlannedStep::Run(path) => simple_plan_step("run", Some(path.clone()), Vec::new()),
        PlannedStep::Rules => simple_plan_step("rules", None, Vec::new()),
        PlannedStep::Memory(path) => simple_plan_step("memory", Some(path.clone()), Vec::new()),
        PlannedStep::GitCommit(path) => {
            simple_plan_step("git_commit", Some(path.clone()), Vec::new())
        }
        PlannedStep::GitPR(path) => simple_plan_step("git_pr", Some(path.clone()), Vec::new()),
        PlannedStep::AlternativeMutationSearch(spec) => {
            simple_plan_step("alternative_mutation_search", None, vec![spec.clone()])
        }
        PlannedStep::DesignDeltaReasoning(spec) => {
            simple_plan_step("design_delta_reasoning", None, vec![spec.clone()])
        }
        PlannedStep::ExplainDesignTradeoff(prompt) => {
            simple_plan_step("explain_design_tradeoff", None, vec![prompt.clone()])
        }
        PlannedStep::ApplyPreviousCodingStep => {
            simple_plan_step("apply_previous_coding", None, Vec::new())
        }
        PlannedStep::RollbackCurrentTransaction => {
            simple_plan_step("rollback_current_transaction", None, Vec::new())
        }
        PlannedStep::IrReload(path) => {
            simple_plan_step("ir_reload", Some(path.clone()), Vec::new())
        }
        PlannedStep::IrReloadAll(path) => {
            simple_plan_step("ir_reload_all", Some(path.clone()), Vec::new())
        }
        PlannedStep::ShowDeps(path) => {
            simple_plan_step("show_deps", Some(path.clone()), Vec::new())
        }
        PlannedStep::Refactor(spec) => simple_plan_step(
            "refactor",
            Some(spec.target.clone()),
            vec![spec.request.clone()],
        ),
        PlannedStep::Repair(spec) => {
            simple_plan_step("repair", Some(spec.target.clone()), Vec::new())
        }
        PlannedStep::Apply => simple_plan_step("apply", None, Vec::new()),
        PlannedStep::Reload => simple_plan_step("reload", None, Vec::new()),
    }
}

fn plan_steps_from_runtime_plan(plan: &Plan) -> Vec<PlanStepRecord> {
    plan.steps
        .iter()
        .map(|step| {
            let mut kind = "noop".to_string();
            let mut path = None;
            let mut args = Vec::new();
            if let Some(command) = &step.command {
                kind = command.name.clone();
                args = command.args.clone();
                path = command.args.first().map(PathBuf::from);
                if let Some(subcommand) = &command.subcommand {
                    args.insert(0, subcommand.clone());
                }
            }
            PlanStepRecord {
                kind,
                path,
                args,
                description: Some(step.description.clone()),
            }
        })
        .collect()
}

fn plan_step_with_coding_options(
    kind: &str,
    path: PathBuf,
    options: &CodingOptions,
) -> PlanStepRecord {
    let mut args = Vec::new();
    if let Some(request) = &options.request {
        args.push(format!("request={request}"));
    }
    args.push(format!("safe={}", options.safe));
    args.push(format!("check={}", options.check));
    PlanStepRecord {
        kind: kind.to_string(),
        path: Some(path),
        args,
        description: None,
    }
}

fn simple_plan_step(kind: &str, path: Option<PathBuf>, args: Vec<String>) -> PlanStepRecord {
    PlanStepRecord {
        kind: kind.to_string(),
        path,
        args,
        description: None,
    }
}

fn memory_step_kind(step: &PlannedStep) -> &'static str {
    match step {
        PlannedStep::Analyze(_) => "analyze",
        PlannedStep::Coding(_, _) => "coding",
        PlannedStep::Validate(_) => "validate",
        PlannedStep::StructureView(_) => "structure_view",
        PlannedStep::StructureEdit(_) => "structure_edit",
        PlannedStep::StructureDiff(_, _) => "structure_diff",
        PlannedStep::StructureUndo(_) => "structure_undo",
        PlannedStep::StructureRedo(_) => "structure_redo",
        PlannedStep::Run(_) => "run",
        PlannedStep::Rules => "rules",
        PlannedStep::Memory(_) => "memory",
        PlannedStep::GitCommit(_) => "git_commit",
        PlannedStep::GitPR(_) => "git_pr",
        PlannedStep::AlternativeMutationSearch(_) => "alternative_mutation_search",
        PlannedStep::DesignDeltaReasoning(_) => "design_delta_reasoning",
        PlannedStep::ExplainDesignTradeoff(_) => "explain_design_tradeoff",
        PlannedStep::ApplyPreviousCodingStep => "apply_previous_coding",
        PlannedStep::RollbackCurrentTransaction => "rollback_current_transaction",
        PlannedStep::IrReload(_) => "ir_reload",
        PlannedStep::IrReloadAll(_) => "ir_reload_all",
        PlannedStep::ShowDeps(_) => "show_deps",
        PlannedStep::Refactor(_) => "refactor",
        PlannedStep::Repair(_) => "repair",
        PlannedStep::Apply => "apply",
        PlannedStep::Reload => "reload",
    }
}

fn memory_tags_for_step(step: &PlannedStep) -> Vec<String> {
    let mut tags = vec![memory_step_kind(step).to_string()];
    match step {
        PlannedStep::Analyze(path)
        | PlannedStep::Validate(path)
        | PlannedStep::StructureView(path)
        | PlannedStep::StructureEdit(path)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path)
        | PlannedStep::Run(path)
        | PlannedStep::Memory(path)
        | PlannedStep::GitCommit(path)
        | PlannedStep::GitPR(path)
        | PlannedStep::IrReload(path)
        | PlannedStep::IrReloadAll(path)
        | PlannedStep::ShowDeps(path) => {
            tags.push(path.display().to_string());
        }
        PlannedStep::Coding(path, options) => {
            tags.push(path.display().to_string());
            if let Some(request) = &options.request {
                tags.push(request.clone());
            }
        }
        PlannedStep::StructureDiff(path, node) => {
            tags.push(path.display().to_string());
            if let Some(node) = node {
                tags.push(node.clone());
            }
        }
        PlannedStep::Refactor(spec) => {
            tags.push(spec.target.display().to_string());
            tags.push(spec.request.clone());
        }
        PlannedStep::Repair(spec) => {
            tags.push(spec.target.display().to_string());
        }
        PlannedStep::AlternativeMutationSearch(spec)
        | PlannedStep::DesignDeltaReasoning(spec)
        | PlannedStep::ExplainDesignTradeoff(spec) => tags.push(spec.clone()),
        PlannedStep::Apply
        | PlannedStep::Reload
        | PlannedStep::Rules
        | PlannedStep::ApplyPreviousCodingStep
        | PlannedStep::RollbackCurrentTransaction => {}
    }
    tags
}
fn tag_overlap(query_tags: &[String], entry_tags: &[String]) -> f32 {
    query_tags
        .iter()
        .filter(|tag| entry_tags.iter().any(|entry_tag| entry_tag == *tag))
        .count() as f32
}

fn recency_weight(age_steps: usize) -> f32 {
    1.0 / (1.0 + age_steps as f32)
}

fn type_weight(memory_type: MemoryType) -> f32 {
    match memory_type {
        MemoryType::ExecutionTrace => 1.0,
        MemoryType::Artifact => 1.2,
        MemoryType::SemanticHint => 1.5,
    }
}

fn decayed_score(score: f32, age_steps: usize) -> f32 {
    score / (1.0 + age_steps as f32)
}

fn apply_memory_outcome(entry: &mut MemoryEntry, outcome: MemoryOutcome) {
    match outcome {
        MemoryOutcome::CompileSuccess => {
            entry.success_count = entry
                .success_count
                .saturating_add(MEMORY_COUNT_SCALE)
                .min(MEMORY_MAX_COUNT_SCALED);
        }
        MemoryOutcome::CompileWithWarnings => {
            entry.success_count = entry
                .success_count
                .saturating_add(1)
                .min(MEMORY_MAX_COUNT_SCALED);
        }
        MemoryOutcome::Failure => {
            entry.failure_count = entry
                .failure_count
                .saturating_add(MEMORY_COUNT_SCALE)
                .min(MEMORY_MAX_COUNT_SCALED);
        }
    }
}

fn embedding_score(
    query_embedding: Option<&Embedding>,
    entry_embedding: Option<&Embedding>,
) -> f32 {
    match (query_embedding, entry_embedding) {
        (Some(query), Some(entry)) => normalized_cosine_similarity(query, entry),
        _ => 0.0,
    }
}

fn memory_score(
    entry: &MemoryEntry,
    overlap: f32,
    age_steps: usize,
    query_embedding: Option<&Embedding>,
    entry_embedding: Option<&Embedding>,
) -> f32 {
    let symbolic_score = overlap + recency_weight(age_steps) + type_weight(entry.memory_type);
    let hybrid_score = SYMBOLIC_WEIGHT * symbolic_score
        + EMBEDDING_WEIGHT * embedding_score(query_embedding, entry_embedding);
    decayed_score(hybrid_score * entry.metadata.relevance, age_steps)
}

fn stored_memories_from_events(events: &[IRExecutionEventRecord]) -> HashMap<String, MemoryEntry> {
    let mut stored = HashMap::new();
    for event in events {
        match &event.payload {
            IRExecutionEventPayload::MemoryStored(payload) => {
                stored.insert(payload.memory_id.clone(), payload.entry.clone());
            }
            IRExecutionEventPayload::MemoryOutcomeRecorded(payload) => {
                if let Some(entry) = stored.get_mut(&payload.memory_id) {
                    apply_memory_outcome(entry, payload.outcome);
                }
            }
            _ => {}
        }
    }
    stored
}

fn embed_query(step: &PlannedStep, query_tags: &[String]) -> Embedding {
    let mut source = memory_query_key(step);
    source.push('\n');
    source.push_str(&query_tags.join("|"));
    deterministic_embedding(&source)
}

fn memory_query_key(step: &PlannedStep) -> String {
    match step {
        PlannedStep::Analyze(path) => format!("{}:{}", memory_step_kind(step), path.display()),
        PlannedStep::Coding(path, options) => format!(
            "{}:{}:{}",
            memory_step_kind(step),
            path.display(),
            options.request.as_deref().unwrap_or("")
        ),
        PlannedStep::Validate(path)
        | PlannedStep::StructureView(path)
        | PlannedStep::StructureEdit(path)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path)
        | PlannedStep::Run(path)
        | PlannedStep::Memory(path)
        | PlannedStep::GitCommit(path)
        | PlannedStep::GitPR(path)
        | PlannedStep::IrReload(path)
        | PlannedStep::IrReloadAll(path)
        | PlannedStep::ShowDeps(path) => format!("{}:{}", memory_step_kind(step), path.display()),
        PlannedStep::StructureDiff(path, node) => {
            format!("{}:{}:{node:?}", memory_step_kind(step), path.display())
        }
        PlannedStep::Refactor(spec) => format!(
            "{}:{}:{}",
            memory_step_kind(step),
            spec.target.display(),
            spec.request
        ),
        PlannedStep::Repair(spec) => {
            format!("{}:{}", memory_step_kind(step), spec.target.display())
        }
        PlannedStep::AlternativeMutationSearch(spec)
        | PlannedStep::DesignDeltaReasoning(spec)
        | PlannedStep::ExplainDesignTradeoff(spec) => format!("{}:{spec}", memory_step_kind(step)),
        PlannedStep::Apply
        | PlannedStep::Reload
        | PlannedStep::Rules
        | PlannedStep::ApplyPreviousCodingStep
        | PlannedStep::RollbackCurrentTransaction => memory_step_kind(step).to_string(),
    }
}
fn deterministic_embedding(text: &str) -> Embedding {
    let mut embedding = vec![0.0; EMBEDDING_DIMENSION];
    for token in tokenize_embedding_source(text) {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let digest = hasher.finalize();
        let bucket = (digest[0] as usize) % EMBEDDING_DIMENSION;
        let magnitude = 1.0 + (digest[1] as f32 / 255.0);
        embedding[bucket] += magnitude;
    }
    normalize_embedding(embedding)
}

fn tokenize_embedding_source(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn normalize_embedding(mut embedding: Embedding) -> Embedding {
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > 0.0 {
        for value in &mut embedding {
            *value /= norm;
        }
    }
    embedding
}

fn cosine_similarity(a: &Embedding, b: &Embedding) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let dot = a
        .iter()
        .zip(b.iter())
        .take(len)
        .map(|(lhs, rhs)| lhs * rhs)
        .sum::<f32>();
    let norm_a = a
        .iter()
        .take(len)
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    let norm_b = b
        .iter()
        .take(len)
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

fn normalized_cosine_similarity(a: &Embedding, b: &Embedding) -> f32 {
    (cosine_similarity(a, b) + 1.0) / 2.0
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::nl::session::ConversationState;
    use crate::nl::types::{CodingOptions, CommandPlan, PlannedStep};

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
        assert!(restored.state.active_transaction.is_none());
        assert_eq!(
            restored.state.next_allowed_actions,
            vec![
                ActionKind::Validate,
                ActionKind::Refactor,
                ActionKind::Rollback
            ]
        );
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

    #[test]
    fn plan_events_record_proposed_and_accepted_lifecycle() {
        let (_dir, store, baseline) = store();
        let plan = CommandPlan {
            intent: None,
            steps: vec![
                PlannedStep::Analyze(PathBuf::from(".")),
                PlannedStep::Coding(
                    PathBuf::from("apps/cli/src/repl.rs"),
                    CodingOptions {
                        request: Some("planner_v2 wiring".to_string()),
                        ..CodingOptions::default()
                    },
                ),
                PlannedStep::Validate(PathBuf::from(".")),
            ],
        };

        store
            .emit_intent_captured(&baseline.session_id, "fix repl planner", None)
            .expect("intent event");
        let plan_id = store
            .emit_plan_proposed(&baseline.session_id, plan.clone(), "planner_v2")
            .expect("plan proposed");
        store
            .emit_plan_accepted(&baseline.session_id, plan_id, ActorType::System)
            .expect("plan accepted");

        let events = store
            .list_plan_events(&baseline.session_id)
            .expect("list plan events");
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0].payload,
            IRPlanEventPayload::IntentCaptured(_)
        ));
        assert!(matches!(
            events[1].payload,
            IRPlanEventPayload::PlanProposed(_)
        ));
        assert!(matches!(
            events[2].payload,
            IRPlanEventPayload::PlanAccepted(_)
        ));

        let stored_plan = store
            .load_plan(&baseline.session_id, plan_id)
            .expect("load plan")
            .expect("plan should exist");
        assert_eq!(stored_plan.plan_id, plan_id);
        assert_eq!(stored_plan.planner, "planner_v2");
        assert_eq!(stored_plan.steps, plan_steps_from_command_plan(&plan));
        assert_eq!(
            store
                .accepted_plan_ids(&baseline.session_id)
                .expect("accepted ids"),
            vec![plan_id]
        );
    }

    #[test]
    fn accepting_unknown_plan_is_rejected() {
        let (_dir, store, baseline) = store();
        let err = store
            .emit_plan_accepted(&baseline.session_id, Uuid::new_v4(), ActorType::System)
            .expect_err("orphan accept must fail");
        assert!(err.contains("was not proposed"));
    }

    // ── Phase 2 negative tests ────────────────────────────────────────────────

    /// emit_step_started with a step_id that was never scheduled must return Err.
    #[test]
    fn emit_step_started_without_scheduled_returns_err() {
        let (_dir, store, baseline) = store();
        let random_step_id = Uuid::new_v4();
        let err = store
            .emit_step_started(&baseline.session_id, random_step_id)
            .expect_err("starting an unscheduled step must fail");
        assert!(
            err.contains("was never scheduled"),
            "unexpected error: {err}"
        );
    }

    /// Full happy-path step lifecycle: Scheduled → Started → Completed → Result.
    /// project_execution_state must reflect the completed result.
    #[test]
    fn step_lifecycle_recorded_in_execution_state() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();

        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Coding")
            .expect("schedule step");
        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("start step");
        store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect("complete step");
        store
            .emit_execution_result(
                &baseline.session_id,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some("ok".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: vec![],
                },
            )
            .expect("record result");

        let state = store
            .project_execution_state(&baseline.session_id)
            .expect("project state");
        assert_eq!(state.active_plan, Some(plan_id));
        // After StepCompleted, active_step clears.
        assert_eq!(state.active_step, None);
        assert!(state.last_result.is_some());
        assert_eq!(state.last_result.unwrap().stdout.as_deref(), Some("ok"));
    }

    /// ArtifactApplied adds to state.artifacts; ArtifactRolledBack removes it.
    #[test]
    fn artifact_apply_and_rollback_reflected_in_state() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Coding")
            .expect("schedule");
        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("start");

        let artifact = ArtifactRef {
            artifact_kind: "code_diff".into(),
            artifact_id: "patch-001".into(),
            description: Some("initial patch".into()),
        };
        store
            .emit_artifact_produced(&baseline.session_id, artifact.clone())
            .expect("produced");
        store
            .emit_artifact_applied(&baseline.session_id, artifact.clone())
            .expect("applied");

        let state = store
            .project_execution_state(&baseline.session_id)
            .expect("state after apply");
        assert_eq!(state.artifacts.len(), 1);
        assert_eq!(state.artifacts[0].artifact_id, "patch-001");

        store
            .emit_artifact_rolled_back(&baseline.session_id, artifact)
            .expect("rolled back");

        let state = store
            .project_execution_state(&baseline.session_id)
            .expect("state after rollback");
        assert!(
            state.artifacts.is_empty(),
            "artifact must be removed after rollback"
        );
    }

    /// emit_step_started a second time for the same step must return Err.
    #[test]
    fn emit_step_started_twice_for_same_step_fails() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Analyze")
            .expect("schedule");
        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("first start");
        let err = store
            .emit_step_started(&baseline.session_id, step_id)
            .expect_err("second start must fail");
        assert!(err.contains("already started"), "unexpected error: {err}");
    }

    /// emit_step_completed without a preceding emit_step_started must return Err.
    #[test]
    fn step_completed_without_started_fails() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Coding")
            .expect("schedule");
        let err = store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect_err("completing without starting must fail");
        assert!(err.contains("never started"), "unexpected error: {err}");
    }

    /// Full lifecycle is strictly linear: Started and Completed are each accepted once;
    /// any subsequent call to either is rejected.
    #[test]
    fn step_lifecycle_is_strictly_linear() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Validate")
            .expect("schedule");

        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("start");
        store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect("complete");

        // Re-starting a completed step must fail.
        let err = store
            .emit_step_started(&baseline.session_id, step_id)
            .expect_err("re-start after complete must fail");
        assert!(err.contains("already started"), "unexpected error: {err}");

        // Re-completing a completed step must fail.
        let err = store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect_err("re-complete must fail");
        assert!(err.contains("already completed"), "unexpected error: {err}");
    }

    // ── Phase 2.1 tests ───────────────────────────────────────────────────────

    /// Any transition that skips Scheduled → Started → Completed ordering must fail.
    #[test]
    fn invalid_step_transition_fails() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();

        // Completing before starting must fail.
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Coding")
            .expect("schedule");
        store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect_err("completing before starting must fail");

        // Starting an unscheduled step must fail.
        let ghost_id = Uuid::new_v4();
        store
            .emit_step_started(&baseline.session_id, ghost_id)
            .expect_err("starting an unscheduled step must fail");
    }

    /// ArtifactApplied without a preceding ArtifactProduced must return Err.
    #[test]
    fn artifact_without_produced_fails() {
        let (_dir, store, baseline) = store();
        let artifact = ArtifactRef {
            artifact_kind: "code_diff".into(),
            artifact_id: "patch-orphan".into(),
            description: None,
        };
        let err = store
            .emit_artifact_applied(&baseline.session_id, artifact)
            .expect_err("applying without producing must fail");
        assert!(
            err.contains("was never produced"),
            "unexpected error: {err}"
        );
    }

    /// Replaying execution events via project_execution_state must match
    /// the state built up incrementally by the emit calls.
    #[test]
    fn projection_matches_replay() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();

        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Analyze")
            .expect("schedule");
        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("start");

        let artifact = ArtifactRef {
            artifact_kind: "analysis_output".into(),
            artifact_id: "a-001".into(),
            description: Some("static analysis result".into()),
        };
        store
            .emit_artifact_produced(&baseline.session_id, artifact.clone())
            .expect("produced");
        store
            .emit_artifact_applied(&baseline.session_id, artifact.clone())
            .expect("applied");

        store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect("complete");
        store
            .emit_execution_result(
                &baseline.session_id,
                StepExecutionResultPayload {
                    step_id,
                    stdout: Some("analysis done".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: vec![artifact.clone()],
                },
            )
            .expect("record result");

        // Validate result completeness.
        store
            .assert_execution_result_exists(&baseline.session_id, step_id)
            .expect("result must exist");

        // Replay must reflect all side effects correctly.
        let state = store
            .project_execution_state(&baseline.session_id)
            .expect("project state");
        assert_eq!(state.active_plan, Some(plan_id));
        assert_eq!(state.active_step, None); // cleared by StepCompleted
        assert_eq!(state.artifacts.len(), 1);
        assert_eq!(state.artifacts[0].artifact_id, "a-001");
        assert!(state.last_result.is_some());
        assert_eq!(
            state.last_result.unwrap().stdout.as_deref(),
            Some("analysis done")
        );
    }

    #[test]
    fn memory_reference_is_recorded() {
        let (_dir, store, baseline) = store();
        let stored_step_id = Uuid::new_v4();
        let stored_memory = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: stored_step_id,
                    stdout: Some("prior analysis".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store memory");

        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 1, "Analyze")
            .expect("schedule");
        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                1,
            )
            .expect("query");
        assert_eq!(context.ids(), vec![stored_memory.memory_id.clone()]);
        store
            .emit_memory_referenced(&baseline.session_id, step_id, context.ids())
            .expect("emit references");

        let events = store
            .list_execution_events(&baseline.session_id)
            .expect("events");
        assert!(events.iter().any(|event| {
            matches!(
                &event.payload,
                IRExecutionEventPayload::MemoryReferenced(payload)
                    if payload.step_id == step_id
                        && payload.references == vec![stored_memory.memory_id.clone()]
            )
        }));
    }

    #[test]
    fn memory_store_is_recorded() {
        let (_dir, store, baseline) = store();
        let step_id = Uuid::new_v4();
        let entry = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Coding(PathBuf::from("src/lib.rs"), CodingOptions::default()),
                0,
                &StepExecutionResultPayload {
                    step_id,
                    stdout: Some("preview".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: vec![ArtifactRef {
                        artifact_kind: "code_diff".into(),
                        artifact_id: "diff:memory".into(),
                        description: Some("preview artifact".into()),
                    }],
                },
            )
            .expect("store execution memory");

        let events = store
            .list_execution_events(&baseline.session_id)
            .expect("events");
        assert!(events.iter().any(|event| {
            matches!(
                &event.payload,
                IRExecutionEventPayload::MemoryStored(payload)
                    if payload.step_id == step_id
                        && payload.step_index == 0
                        && payload.memory_id == entry.memory_id
                        && payload.entry == entry
            )
        }));
    }

    #[test]
    fn memory_context_reconstructed_from_ir() {
        let (dir, store, baseline) = store();
        let remembered = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("analysis snapshot".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store memory");
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 1, "Analyze")
            .expect("schedule");
        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                1,
            )
            .expect("query");
        store
            .emit_memory_referenced(&baseline.session_id, step_id, context.ids())
            .expect("emit references");

        let replay_store = IRPersistenceStore::new(dir.path());
        let projected = replay_store
            .project_execution_state(&baseline.session_id)
            .expect("project");
        assert_eq!(projected.memory_context.len(), 1);
        assert_eq!(projected.memory_context[0].memory_id, remembered.memory_id);
        assert_eq!(
            projected.memory_context[0]
                .content
                .get("stdout")
                .and_then(serde_json::Value::as_str),
            Some("analysis snapshot")
        );
    }

    #[test]
    fn memory_query_is_deterministic() {
        let (_dir, store, baseline) = store();
        store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("same tags".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store first");
        store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Coding(PathBuf::from("src/lib.rs"), CodingOptions::default()),
                1,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("artifact memory".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: vec![ArtifactRef {
                        artifact_kind: "code_diff".into(),
                        artifact_id: "d1".into(),
                        description: None,
                    }],
                },
            )
            .expect("store second");

        let lhs = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Coding(PathBuf::from("src/lib.rs"), CodingOptions::default()),
                2,
            )
            .expect("query lhs");
        let rhs = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Coding(PathBuf::from("src/lib.rs"), CodingOptions::default()),
                2,
            )
            .expect("query rhs");

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn future_memory_not_accessible() {
        let (_dir, store, baseline) = store();
        let future = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                2,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("future".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store future");

        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                1,
            )
            .expect("query");

        assert!(
            context
                .entries
                .iter()
                .all(|entry| entry.memory_id != future.memory_id),
            "future memory must not be accessible"
        );
    }

    #[test]
    fn higher_score_ranked_first() {
        let (_dir, store, baseline) = store();
        let trace = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("trace".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store trace");
        let artifact = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Coding(PathBuf::from("src/lib.rs"), CodingOptions::default()),
                1,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("artifact".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: vec![ArtifactRef {
                        artifact_kind: "code_diff".into(),
                        artifact_id: "artifact-ranked".into(),
                        description: None,
                    }],
                },
            )
            .expect("store artifact");

        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Coding(PathBuf::from("src/lib.rs"), CodingOptions::default()),
                2,
            )
            .expect("query");

        assert_eq!(context.entries[0].memory_id, artifact.memory_id);
        assert!(
            context
                .entries
                .iter()
                .any(|entry| entry.memory_id == trace.memory_id)
        );
    }

    #[test]
    fn older_memory_has_lower_score() {
        let (_dir, store, baseline) = store();
        let older = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("older".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store older");
        let newer = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                2,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("newer".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store newer");

        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                3,
            )
            .expect("query");

        assert_eq!(context.entries[0].memory_id, newer.memory_id);
        assert!(
            context
                .entries
                .iter()
                .any(|entry| entry.memory_id == older.memory_id)
        );
    }

    #[test]
    fn embedding_retrieval_is_deterministic() {
        let (_dir, store, baseline) = store();
        store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::DesignDeltaReasoning("semantic memory bridge".to_string()),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("semantic memory bridge".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store embedding-backed memory");

        let lhs = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::DesignDeltaReasoning("semantic memory bridge".to_string()),
                1,
            )
            .expect("lhs");
        let rhs = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::DesignDeltaReasoning("semantic memory bridge".to_string()),
                1,
            )
            .expect("rhs");

        assert_eq!(lhs, rhs);
        assert!(lhs.entries[0].embedding.is_some());
    }

    #[test]
    fn embedding_affects_ranking() {
        let (_dir, store, baseline) = store();
        let step = PlannedStep::DesignDeltaReasoning("memory meaning".to_string());
        let matching = MemoryEntry {
            memory_id: "mem:matching".to_string(),
            source_event: Uuid::new_v4(),
            memory_type: MemoryType::SemanticHint,
            content: serde_json::json!({ "summary": "memory meaning" }),
            embedding: Some(deterministic_embedding("memory meaning semantic alignment")),
            success_count: 0,
            failure_count: 0,
            metadata: MemoryMetadata {
                timestamp: 10,
                step_index: 0,
                relevance: 1.0,
                tags: vec!["design_delta_reasoning".to_string()],
            },
        };
        let non_matching = MemoryEntry {
            memory_id: "mem:non-matching".to_string(),
            source_event: Uuid::new_v4(),
            memory_type: MemoryType::SemanticHint,
            content: serde_json::json!({ "summary": "network sockets" }),
            embedding: Some(deterministic_embedding("network sockets transport layer")),
            success_count: 0,
            failure_count: 0,
            metadata: MemoryMetadata {
                timestamp: 10,
                step_index: 0,
                relevance: 1.0,
                tags: vec!["design_delta_reasoning".to_string()],
            },
        };
        store
            .emit_memory_stored(
                &baseline.session_id,
                matching.source_event,
                0,
                matching.clone(),
            )
            .expect("emit matching");
        store
            .emit_memory_stored(
                &baseline.session_id,
                non_matching.source_event,
                0,
                non_matching.clone(),
            )
            .expect("emit non matching");

        let context = store
            .query_memory_context(&baseline.session_id, &step, 1)
            .expect("query");

        assert_eq!(context.entries[0].memory_id, matching.memory_id);
    }

    #[test]
    fn embedding_respects_step_index_constraint() {
        let (_dir, store, baseline) = store();
        let future = MemoryEntry {
            memory_id: "mem:future-embedding".to_string(),
            source_event: Uuid::new_v4(),
            memory_type: MemoryType::SemanticHint,
            content: serde_json::json!({ "summary": "future semantic hint" }),
            embedding: Some(deterministic_embedding("future semantic hint")),
            success_count: 0,
            failure_count: 0,
            metadata: MemoryMetadata {
                timestamp: 20,
                step_index: 3,
                relevance: 1.0,
                tags: vec!["design_delta_reasoning".to_string()],
            },
        };
        store
            .emit_memory_stored(&baseline.session_id, future.source_event, 3, future.clone())
            .expect("emit future");

        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::DesignDeltaReasoning("future semantic hint".to_string()),
                2,
            )
            .expect("query");

        assert!(
            context
                .entries
                .iter()
                .all(|entry| entry.memory_id != future.memory_id)
        );
    }

    #[test]
    fn embedding_replay_consistency() {
        let (dir, store, baseline) = store();
        let entry = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::DesignDeltaReasoning("hybrid memory replay".to_string()),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("hybrid memory replay".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store");
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 1, "design_delta_reasoning")
            .expect("schedule");
        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::DesignDeltaReasoning("hybrid memory replay".to_string()),
                1,
            )
            .expect("query");
        store
            .emit_memory_referenced(&baseline.session_id, step_id, context.ids())
            .expect("reference");

        let replay_store = IRPersistenceStore::new(dir.path());
        let projected = replay_store
            .project_execution_state(&baseline.session_id)
            .expect("project");

        assert_eq!(projected.memory_context.len(), 1);
        assert_eq!(projected.memory_context[0].memory_id, entry.memory_id);
        assert_eq!(projected.memory_context[0].embedding, entry.embedding);
    }

    #[test]
    fn memory_outcome_is_recorded() {
        let (_dir, store, baseline) = store();
        let entry = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("learned".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store memory");

        let step_id = Uuid::new_v4();
        store
            .emit_memory_outcome(
                &baseline.session_id,
                step_id,
                entry.memory_id.clone(),
                MemoryOutcome::CompileSuccess,
            )
            .expect("emit outcome");

        let events = store
            .list_execution_events(&baseline.session_id)
            .expect("events");
        assert!(events.iter().any(|event| {
            matches!(
                &event.payload,
                IRExecutionEventPayload::MemoryOutcomeRecorded(payload)
                    if payload.step_id == step_id
                        && payload.memory_id == entry.memory_id
                        && payload.outcome == MemoryOutcome::CompileSuccess
            )
        }));
    }

    #[test]
    fn memory_learning_replay_consistency() {
        let (dir, store, baseline) = store();
        let entry = store
            .store_execution_memory(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                0,
                &StepExecutionResultPayload {
                    step_id: Uuid::new_v4(),
                    stdout: Some("analysis snapshot".into()),
                    stderr: None,
                    structured_output: None,
                    artifacts: Vec::new(),
                },
            )
            .expect("store memory");
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 1, "Analyze")
            .expect("schedule");
        let context = store
            .query_memory_context(
                &baseline.session_id,
                &PlannedStep::Analyze(PathBuf::from("src/lib.rs")),
                1,
            )
            .expect("query");
        store
            .emit_memory_referenced(&baseline.session_id, step_id, context.ids())
            .expect("emit references");
        store
            .emit_memory_outcome(
                &baseline.session_id,
                step_id,
                entry.memory_id.clone(),
                MemoryOutcome::CompileSuccess,
            )
            .expect("emit outcome");

        let replay_store = IRPersistenceStore::new(dir.path());
        let projected = replay_store
            .project_execution_state(&baseline.session_id)
            .expect("project");

        assert_eq!(projected.memory_context.len(), 1);
        assert_eq!(projected.memory_context[0].memory_id, entry.memory_id);
        assert_eq!(projected.memory_context[0].success_count, 2);
        assert_eq!(projected.memory_context[0].failure_count, 0);
    }

    /// Validates that the cache is updated incrementally after each emit,
    /// so repeated step_state() calls within a single store lifetime are consistent.
    #[test]
    fn step_state_cache_reduces_log_reads() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();

        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Coding")
            .expect("schedule");

        // Cache is populated by emit_step_scheduled; no log scan needed.
        let s1 = store
            .step_state(&baseline.session_id, step_id)
            .expect("state after schedule");
        assert!(s1.scheduled);
        assert!(!s1.started);
        assert!(!s1.completed);

        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("start");
        let s2 = store
            .step_state(&baseline.session_id, step_id)
            .expect("state after start");
        assert!(s2.scheduled && s2.started);
        assert!(!s2.completed);

        store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect("complete");
        let s3 = store
            .step_state(&baseline.session_id, step_id)
            .expect("state after complete");
        assert!(s3.scheduled && s3.started && s3.completed);
    }

    /// assert_execution_result_exists returns Err when no result has been recorded.
    #[test]
    fn assert_execution_result_exists_fails_when_missing() {
        let (_dir, store, baseline) = store();
        let plan_id = Uuid::new_v4();
        let step_id = store
            .emit_step_scheduled(&baseline.session_id, plan_id, 0, "Validate")
            .expect("schedule");
        store
            .emit_step_started(&baseline.session_id, step_id)
            .expect("start");
        store
            .emit_step_completed(&baseline.session_id, step_id, ExecutionStatus::Success)
            .expect("complete");

        // No ExecutionResultRecorded emitted yet.
        let err = store
            .assert_execution_result_exists(&baseline.session_id, step_id)
            .expect_err("missing result must return Err");
        assert!(
            err.contains("missing ExecutionResultRecorded"),
            "unexpected error: {err}"
        );
    }

    // ── Phase 2.1 Patch: concurrency test ─────────────────────────────────────

    /// Verify that `StepStateCache` is safe to access from multiple threads
    /// sharing the same `IRPersistenceStore` via `Arc`.
    ///
    /// Each thread calls `step_state()` for an unknown step_id (always a cache
    /// miss → log scan path). No data races or borrow panics must occur.
    #[test]
    fn step_state_cache_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let (dir, store, baseline) = store();
        // Keep dir alive for the duration of the test.
        let _dir = dir;
        let store = Arc::new(store);
        let session_id = Arc::new(baseline.session_id.clone());

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let session_id = Arc::clone(&session_id);
                thread::spawn(move || {
                    // Each thread queries a fresh random step_id (guaranteed cache miss).
                    let ghost_id = Uuid::new_v4();
                    let state = store
                        .step_state(&session_id, ghost_id)
                        .expect("step_state must not error");
                    // An unknown step has no lifecycle events.
                    assert!(!state.scheduled);
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread must not panic");
        }
    }
}
