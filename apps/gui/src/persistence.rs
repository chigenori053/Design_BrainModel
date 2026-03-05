use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::domain::hash::compute_hash;
use agent_core::domain::{AppState, DesignScoreVector, ProposedDiff, UnifiedDesignState};
use serde::{Deserialize, Serialize};

pub const PERSISTED_SCHEMA_VERSION: u32 = 1;
pub const MAX_DELTAS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedState {
    pub schema_version: u32,
    pub version_id: u64,
    pub uds_hash: u64,
    pub uds: UnifiedDesignState,
    pub evaluation: DesignScoreVector,
    pub timestamp: u64,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedDelta {
    pub version_id: u64,
    pub diff: ProposedDiff,
    pub resulting_hash: u64,
    pub resulting_evaluation: DesignScoreVector,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedHistory {
    pub schema_version: u32,
    pub base: PersistedState,
    pub deltas: Vec<PersistedDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointEntry {
    pub version_id: u64,
    pub timestamp: u64,
    pub is_base: bool,
}

#[derive(Debug)]
pub enum PersistError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    IntegrityViolation(String),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Serde(e) => write!(f, "serde error: {e}"),
            Self::IntegrityViolation(reason) => write!(f, "integrity violation: {reason}"),
        }
    }
}

impl std::error::Error for PersistError {}

impl From<std::io::Error> for PersistError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for PersistError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}

impl PersistedState {
    pub fn from_state(state: &AppState) -> Self {
        let uds = state.uds.clone();
        let evaluation = state.evaluation.clone();

        Self {
            schema_version: PERSISTED_SCHEMA_VERSION,
            version_id: state.current_version_id(),
            uds_hash: compute_hash(&uds),
            uds,
            evaluation,
            timestamp: now_timestamp_secs(),
            metadata: None,
        }
    }
}

pub fn save_checkpoint(state: &AppState, path: &Path) -> Result<(), PersistError> {
    let mut history = match load_persisted_history(path)? {
        Some(h) => h,
        None => {
            let history = PersistedHistory {
                schema_version: PERSISTED_SCHEMA_VERSION,
                base: PersistedState::from_state(state),
                deltas: Vec::new(),
            };
            return save_history_atomically(&history, path);
        }
    };

    if history.deltas.len() >= MAX_DELTAS {
        history.base = PersistedState::from_state(state);
        history.deltas.clear();
        return save_history_atomically(&history, path);
    }

    let latest_state = reconstruct_latest_state(&history)?;
    let current_uds = state.uds.clone();
    let current_eval = state.evaluation.clone();
    let current_hash = compute_hash(&current_uds);

    if latest_state.uds_hash == current_hash {
        return Ok(());
    }

    let diffs = generate_diffs(&latest_state.uds, &current_uds);
    if diffs.is_empty() {
        return Ok(());
    }

    let mut simulated_uds = latest_state.uds.clone();
    let total_diffs = diffs.len();
    for (idx, diff) in diffs.into_iter().enumerate() {
        apply_diff_to_uds(&mut simulated_uds, &diff)?;

        let is_last = idx + 1 == total_diffs;
        let resulting_hash = if is_last {
            current_hash
        } else {
            compute_hash(&simulated_uds)
        };
        let resulting_evaluation = if is_last {
            current_eval.clone()
        } else {
            recompute_eval(&simulated_uds)?
        };

        history.deltas.push(PersistedDelta {
            version_id: state.current_version_id(),
            diff,
            resulting_hash,
            resulting_evaluation,
            timestamp: now_timestamp_secs(),
        });
    }

    save_history_atomically(&history, path)
}

pub fn load_checkpoint(path: &Path) -> Result<Option<PersistedState>, PersistError> {
    let history = match load_persisted_history(path)? {
        Some(h) => h,
        None => return Ok(None),
    };
    let latest = reconstruct_latest_state(&history)?;
    Ok(Some(latest))
}

pub fn load_checkpoint_entries(path: &Path) -> Result<Vec<CheckpointEntry>, PersistError> {
    let history = match load_persisted_history(path)? {
        Some(h) => h,
        None => return Ok(Vec::new()),
    };

    let mut entries = Vec::with_capacity(1 + history.deltas.len());
    entries.push(CheckpointEntry {
        version_id: history.base.version_id,
        timestamp: history.base.timestamp,
        is_base: true,
    });
    for d in &history.deltas {
        entries.push(CheckpointEntry {
            version_id: d.version_id,
            timestamp: d.timestamp,
            is_base: false,
        });
    }
    Ok(entries)
}

pub fn load_checkpoint_history(path: &Path) -> Result<Option<PersistedHistory>, PersistError> {
    load_persisted_history(path)
}

pub fn load_checkpoint_at_version(
    path: &Path,
    version_id: u64,
) -> Result<Option<AppState>, PersistError> {
    let history = match load_persisted_history(path)? {
        Some(h) => h,
        None => return Ok(None),
    };

    if version_id == history.base.version_id {
        return Ok(Some(app_state_from_persisted(history.base)));
    }

    let mut state = app_state_from_persisted(history.base.clone());
    for delta in &history.deltas {
        apply_diff_to_uds(&mut state.uds, &delta.diff)?;
        let hash = compute_hash(&state.uds);
        let eval = recompute_eval(&state.uds)?;
        if hash != delta.resulting_hash || eval != delta.resulting_evaluation {
            return Err(PersistError::IntegrityViolation(
                "delta integrity mismatch during version replay".to_string(),
            ));
        }
        state.evaluation = eval;
        if delta.version_id == version_id {
            return Ok(Some(AppState::from_persisted(
                delta.version_id,
                hash,
                state.uds.clone(),
                state.evaluation.clone(),
            )));
        }
    }

    Ok(None)
}

pub fn app_state_from_persisted(persisted: PersistedState) -> AppState {
    AppState::from_persisted(
        persisted.version_id,
        persisted.uds_hash,
        persisted.uds,
        persisted.evaluation,
    )
}

fn save_history_atomically(history: &PersistedHistory, path: &Path) -> Result<(), PersistError> {
    let bytes = serde_json::to_vec_pretty(history)?;
    let tmp_path = path.with_extension("tmp");

    let mut file = File::create(&tmp_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn load_persisted_history(path: &Path) -> Result<Option<PersistedHistory>, PersistError> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let raw_value: serde_json::Value = serde_json::from_str(&content)?;

    if raw_value.get("base").is_some() && raw_value.get("deltas").is_some() {
        let history: PersistedHistory = serde_json::from_value(raw_value)?;
        if history.schema_version != PERSISTED_SCHEMA_VERSION {
            return Err(PersistError::IntegrityViolation(format!(
                "unsupported history schema_version: {}",
                history.schema_version
            )));
        }
        return Ok(Some(history));
    }

    // Backward compatibility with single-checkpoint files.
    let persisted: PersistedState = serde_json::from_str(&content)?;
    if persisted.schema_version != PERSISTED_SCHEMA_VERSION {
        return Err(PersistError::IntegrityViolation(format!(
            "unsupported schema_version: {}",
            persisted.schema_version
        )));
    }
    Ok(Some(PersistedHistory {
        schema_version: PERSISTED_SCHEMA_VERSION,
        base: persisted,
        deltas: Vec::new(),
    }))
}

fn reconstruct_latest_state(history: &PersistedHistory) -> Result<PersistedState, PersistError> {
    let mut state = app_state_from_persisted(history.base.clone());

    validate_persisted_state(&history.base)?;

    for delta in &history.deltas {
        apply_diff_to_uds(&mut state.uds, &delta.diff)?;

        let recomputed_hash = compute_hash(&state.uds);
        let recomputed_eval = recompute_eval(&state.uds)?;

        if recomputed_hash != delta.resulting_hash {
            return Err(PersistError::IntegrityViolation(format!(
                "delta hash mismatch at version {}",
                delta.version_id
            )));
        }
        if recomputed_eval != delta.resulting_evaluation {
            return Err(PersistError::IntegrityViolation(format!(
                "delta evaluation mismatch at version {}",
                delta.version_id
            )));
        }

        state.evaluation = recomputed_eval;
    }

    Ok(PersistedState {
        schema_version: PERSISTED_SCHEMA_VERSION,
        version_id: history
            .deltas
            .last()
            .map(|d| d.version_id)
            .unwrap_or(history.base.version_id),
        uds_hash: compute_hash(&state.uds),
        uds: state.uds,
        evaluation: state.evaluation,
        timestamp: history
            .deltas
            .last()
            .map(|d| d.timestamp)
            .unwrap_or(history.base.timestamp),
        metadata: None,
    })
}

fn validate_persisted_state(persisted: &PersistedState) -> Result<(), PersistError> {
    let actual_hash = compute_hash(&persisted.uds);
    if actual_hash != persisted.uds_hash {
        return Err(PersistError::IntegrityViolation(format!(
            "uds_hash mismatch: expected={}, actual={actual_hash}",
            persisted.uds_hash
        )));
    }

    let recomputed_eval = recompute_eval(&persisted.uds)?;
    if recomputed_eval != persisted.evaluation {
        return Err(PersistError::IntegrityViolation(
            "evaluation mismatch".to_string(),
        ));
    }

    Ok(())
}

fn generate_diffs(from: &UnifiedDesignState, to: &UnifiedDesignState) -> Vec<ProposedDiff> {
    let mut diffs = Vec::new();

    for key in from.nodes.keys() {
        if !to.nodes.contains_key(key) {
            diffs.push(ProposedDiff::RemoveNode { key: key.clone() });
        }
    }

    for (key, value) in &to.nodes {
        if from.nodes.get(key) != Some(value) {
            diffs.push(ProposedDiff::UpsertNode {
                key: key.clone(),
                value: value.clone(),
            });
        }
    }

    let all_dep_keys = from
        .dependencies
        .keys()
        .chain(to.dependencies.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    for key in all_dep_keys {
        match (from.dependencies.get(&key), to.dependencies.get(&key)) {
            (Some(_), None) => diffs.push(ProposedDiff::RemoveDependencies { key }),
            (_, Some(next)) => {
                let mut sorted = next.clone();
                sorted.sort();
                sorted.dedup();
                if from.dependencies.get(&key) != Some(&sorted) {
                    diffs.push(ProposedDiff::SetDependencies {
                        key,
                        dependencies: sorted,
                    });
                }
            }
            (None, None) => {}
        }
    }

    diffs
}

fn apply_diff_to_uds(
    uds: &mut UnifiedDesignState,
    diff: &ProposedDiff,
) -> Result<(), PersistError> {
    match diff {
        ProposedDiff::UpsertNode { key, value } => {
            uds.nodes.insert(key.clone(), value.clone());
        }
        ProposedDiff::RemoveNode { key } => {
            if uds.nodes.remove(key).is_none() {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta remove node failed: missing key {key}"
                )));
            }
            uds.dependencies.remove(key);
            for deps in uds.dependencies.values_mut() {
                deps.retain(|dep| dep != key);
            }
        }
        ProposedDiff::SetDependencies { key, dependencies } => {
            if !uds.nodes.contains_key(key) {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta set dependencies failed: missing owner {key}"
                )));
            }
            for dep in dependencies {
                if !uds.nodes.contains_key(dep) {
                    return Err(PersistError::IntegrityViolation(format!(
                        "delta set dependencies failed: missing dependency {dep}"
                    )));
                }
            }
            let mut sorted = dependencies.clone();
            sorted.sort();
            sorted.dedup();
            uds.dependencies.insert(key.clone(), sorted);
        }
        ProposedDiff::RemoveDependencies { key } => {
            uds.dependencies.remove(key);
        }
        ProposedDiff::SplitHighOutDegreeNode { key } => {
            if !uds.nodes.contains_key(key) {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta split failed: missing owner {key}"
                )));
            }
            let deps = uds.dependencies.get(key).cloned().ok_or_else(|| {
                PersistError::IntegrityViolation(format!(
                    "delta split failed: missing dependencies for {key}"
                ))
            })?;

            let mut normalized = deps;
            normalized.sort();
            normalized.dedup();
            if normalized.len() < 3 {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta split failed: invalid candidate {key}"
                )));
            }

            let split_at = normalized.len() / 2;
            if split_at == 0 || split_at >= normalized.len() {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta split failed: invalid split index for {key}"
                )));
            }

            let kept = normalized[..split_at].to_vec();
            let moved = normalized[split_at..].to_vec();
            let new_key = next_split_node_key(uds, key);
            let new_value = uds.nodes.get(key).cloned().unwrap_or_default();

            uds.nodes.insert(new_key.clone(), new_value);
            uds.dependencies.insert(new_key.clone(), moved);

            let mut owner_deps = kept;
            owner_deps.push(new_key);
            owner_deps.sort();
            owner_deps.dedup();
            uds.dependencies.insert(key.clone(), owner_deps);
        }
        ProposedDiff::RewireHighImpactEdge { key, from, to } => {
            if !uds.nodes.contains_key(key)
                || !uds.nodes.contains_key(from)
                || !uds.nodes.contains_key(to)
            {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta rewire failed: invalid node in {key}:{from}->{to}"
                )));
            }
            if key == to {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta rewire failed: self loop {key}->{to}"
                )));
            }
            let deps = uds.dependencies.get_mut(key).ok_or_else(|| {
                PersistError::IntegrityViolation(format!(
                    "delta rewire failed: missing owner deps for {key}"
                ))
            })?;
            if !deps.iter().any(|d| d == from) {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta rewire failed: missing source edge {key}->{from}"
                )));
            }
            if deps.iter().any(|d| d == to) {
                return Err(PersistError::IntegrityViolation(format!(
                    "delta rewire failed: target already exists {key}->{to}"
                )));
            }
            deps.retain(|d| d != from);
            deps.push(to.clone());
            deps.sort();
            deps.dedup();
        }
        ProposedDiff::TwoStep { first, second } => {
            apply_diff_to_uds(uds, first)?;
            apply_diff_to_uds(uds, second)?;
        }
    }
    Ok(())
}

fn next_split_node_key(uds: &UnifiedDesignState, base: &str) -> String {
    let mut idx = 1_usize;
    loop {
        let key = format!("{base}__split{idx}");
        if !uds.nodes.contains_key(&key) {
            return key;
        }
        idx = idx.saturating_add(1);
    }
}

fn recompute_eval(uds: &UnifiedDesignState) -> Result<DesignScoreVector, PersistError> {
    let mut app = AppState::from_persisted(
        0,
        compute_hash(uds),
        uds.clone(),
        DesignScoreVector::default(),
    );
    app.evaluate_now().map_err(|e| {
        PersistError::IntegrityViolation(format!("evaluation recompute failed: {e:?}"))
    })?;
    Ok(app.evaluation)
}

fn now_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
