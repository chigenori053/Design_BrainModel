use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HolographicMemoryObservationEvent {
    MemoryInserted,
    RecallExecuted,
    DuplicateCandidateDetected,
    RecallConflictDetected,
    CanonicalCandidateSelected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateClass {
    None,
    ExactDuplicate,
    SemanticDuplicate,
    NearDuplicate,
    ConflictCandidate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HolographicMemoryObservationLog {
    pub event_id: String,
    pub event_type: HolographicMemoryObservationEvent,
    pub memory_id: String,
    pub source_input_hash: String,
    pub canonical_key: String,
    pub embedding_dim: usize,
    pub holographic_dim: usize,
    pub resonance_score: f32,
    pub ambiguity_score: f32,
    pub recall_count: u64,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
    pub duplicate_candidate_ids: Vec<String>,
    pub duplicate_class: DuplicateClass,
    pub selected_as_canonical: bool,
    pub rejected_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HolographicMemoryLogStore {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryLogFilter {
    pub recent: Option<usize>,
    pub memory_id: Option<String>,
    pub duplicate_only: bool,
    pub conflict_only: bool,
    pub duplicate_class: Option<DuplicateClass>,
    pub since: Option<u64>,
}

impl HolographicMemoryLogStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_for_workspace(root: &Path) -> Self {
        Self::new(root.join(".dbm/logs/holographic_memory_observation.jsonl"))
    }

    pub fn append(&self, log: HolographicMemoryObservationLog) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| err.to_string())?;
        let line = serde_json::to_string(&log).map_err(|err| err.to_string())?;
        writeln!(file, "{line}").map_err(|err| err.to_string())
    }

    pub fn read_all(&self) -> Result<Vec<HolographicMemoryObservationLog>, String> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .map_err(|err| err.to_string())?;
        let reader = BufReader::new(file);
        let mut logs = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|err| err.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(log) = serde_json::from_str::<HolographicMemoryObservationLog>(&line) {
                logs.push(log);
            }
        }
        Ok(logs)
    }

    pub fn query(
        &self,
        filter: MemoryLogFilter,
    ) -> Result<Vec<HolographicMemoryObservationLog>, String> {
        let mut logs = self.read_all()?;
        logs.retain(|log| {
            if let Some(memory_id) = &filter.memory_id
                && &log.memory_id != memory_id
            {
                return false;
            }
            if filter.duplicate_only && log.duplicate_class == DuplicateClass::None {
                return false;
            }
            if filter.conflict_only && log.duplicate_class != DuplicateClass::ConflictCandidate {
                return false;
            }
            if let Some(class) = &filter.duplicate_class
                && &log.duplicate_class != class
            {
                return false;
            }
            if let Some(since) = filter.since
                && log.created_at < since
            {
                return false;
            }
            true
        });
        if let Some(recent) = filter.recent
            && logs.len() > recent
        {
            logs = logs.split_off(logs.len() - recent);
        }
        Ok(logs)
    }
}

pub fn classify_duplicate(
    source_input_hash_matches: bool,
    canonical_key_matches: bool,
    resonance_score: f32,
    recall_result_differs: bool,
) -> DuplicateClass {
    if source_input_hash_matches {
        DuplicateClass::ExactDuplicate
    } else if resonance_score >= 0.80 && recall_result_differs {
        DuplicateClass::ConflictCandidate
    } else if canonical_key_matches {
        DuplicateClass::SemanticDuplicate
    } else if resonance_score >= 0.80 {
        DuplicateClass::NearDuplicate
    } else {
        DuplicateClass::None
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn observation_event_id(
    event_type: &HolographicMemoryObservationEvent,
    memory_id: &str,
) -> String {
    format!("{:?}:{memory_id}:{}", event_type, now_secs())
}

pub fn parse_duplicate_class(value: &str) -> Option<DuplicateClass> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(DuplicateClass::None),
        "exact" | "exactduplicate" => Some(DuplicateClass::ExactDuplicate),
        "semantic" | "semanticduplicate" => Some(DuplicateClass::SemanticDuplicate),
        "near" | "nearduplicate" => Some(DuplicateClass::NearDuplicate),
        "conflict" | "conflictcandidate" => Some(DuplicateClass::ConflictCandidate),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(class: DuplicateClass, memory_id: &str) -> HolographicMemoryObservationLog {
        HolographicMemoryObservationLog {
            event_id: format!("event-{memory_id}"),
            event_type: HolographicMemoryObservationEvent::DuplicateCandidateDetected,
            memory_id: memory_id.to_string(),
            source_input_hash: "hash".to_string(),
            canonical_key: "key".to_string(),
            embedding_dim: 8,
            holographic_dim: 8,
            resonance_score: 0.93,
            ambiguity_score: 0.1,
            recall_count: 1,
            created_at: 100,
            last_used_at: Some(101),
            duplicate_candidate_ids: vec!["mem_001".to_string()],
            duplicate_class: class,
            selected_as_canonical: false,
            rejected_reason: None,
        }
    }

    #[test]
    fn exact_duplicate_is_classified() {
        assert_eq!(
            classify_duplicate(true, false, 0.1, false),
            DuplicateClass::ExactDuplicate
        );
    }

    #[test]
    fn semantic_duplicate_is_classified() {
        assert_eq!(
            classify_duplicate(false, true, 0.5, false),
            DuplicateClass::SemanticDuplicate
        );
    }

    #[test]
    fn near_duplicate_is_classified() {
        assert_eq!(
            classify_duplicate(false, false, 0.9, false),
            DuplicateClass::NearDuplicate
        );
    }

    #[test]
    fn conflict_candidate_is_classified() {
        assert_eq!(
            classify_duplicate(false, false, 0.9, true),
            DuplicateClass::ConflictCandidate
        );
    }

    #[test]
    fn log_store_query_filters_duplicates_and_recent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = HolographicMemoryLogStore::new(dir.path().join("log.jsonl"));
        store
            .append(sample(DuplicateClass::None, "mem_001"))
            .expect("append");
        store
            .append(sample(DuplicateClass::SemanticDuplicate, "mem_002"))
            .expect("append");

        let logs = store
            .query(MemoryLogFilter {
                duplicate_only: true,
                recent: Some(1),
                ..MemoryLogFilter::default()
            })
            .expect("query");

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].memory_id, "mem_002");
    }

    #[test]
    fn broken_jsonl_line_is_skipped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("log.jsonl");
        fs::write(&path, "{broken}\n").expect("write");
        let store = HolographicMemoryLogStore::new(path);

        assert!(store.read_all().expect("read").is_empty());
    }
}
