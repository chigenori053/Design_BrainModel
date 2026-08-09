use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::holographic_memory_observation::DuplicateClass;
use crate::ir::MemoryEntry;

pub type CanonicalMemoryId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalMemory {
    pub canonical_id: String,
    pub source_hash: String,
    pub canonical_key: String,
    pub reinforcement_count: u64,
    pub first_seen_at: u64,
    pub last_seen_at: u64,
    pub memory_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalMemoryIndex {
    pub by_source_hash: HashMap<String, CanonicalMemoryId>,
    pub by_canonical_key: HashMap<String, CanonicalMemoryId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalMemoryRecord {
    pub canonical: CanonicalMemory,
    pub entry: MemoryEntry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReinforcementRecord {
    pub canonical_id: String,
    pub memory_id: String,
    #[serde(default)]
    pub source_hash: String,
    pub duplicate_class: DuplicateClass,
    pub resonance: f32,
    pub reinforced_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryConflictRecord {
    pub existing_canonical_id: String,
    pub incoming_canonical_id: String,
    pub incoming_memory_id: String,
    pub canonical_key: String,
    pub resonance: f32,
    pub detected_at: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalInsertResult {
    pub canonical: CanonicalMemoryRecord,
    pub duplicate_class: DuplicateClass,
    pub resonance: f32,
    pub inserted: bool,
    pub conflicting_canonical_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GlobalHolographicMemoryStore {
    root: PathBuf,
    policy: memory_persistence::MemoryPolicy,
}

impl GlobalHolographicMemoryStore {
    pub fn default_for_workspace(workspace_root: &Path) -> Self {
        Self::with_policy(workspace_root, memory_persistence::MemoryPolicy::default())
    }

    /// カスタム判別ポリシーでストアを構築する。
    ///
    /// 系統B (`PersistentMemoryStore` の `DecisionEngine`) と同じ `MemoryPolicy` を
    /// 渡すことで、重複/衝突閾値の管理場所を一元化できる。
    pub fn with_policy(workspace_root: &Path, policy: memory_persistence::MemoryPolicy) -> Self {
        let workspace_root = core_types::WorkspaceRoot::discover_from(workspace_root);
        Self {
            root: workspace_root.join(".dbm/memory"),
            policy,
        }
    }

    pub fn policy(&self) -> &memory_persistence::MemoryPolicy {
        &self.policy
    }

    pub fn canonical_path(&self) -> PathBuf {
        self.root.join("canonical_memory.jsonl")
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join("global_memory_index.json")
    }

    pub fn reinforcement_path(&self) -> PathBuf {
        self.root.join("reinforcement_history.jsonl")
    }

    pub fn conflicts_path(&self) -> PathBuf {
        self.root.join("memory_conflicts.jsonl")
    }

    pub fn archived_path(&self) -> PathBuf {
        self.root.join("archived_canonical_memory.jsonl")
    }

    /// `memory_persistence::PersistentMemoryStore` の JSON スナップショット保存先。
    ///
    /// canonical/index/reinforcement と同じ `.dbm/memory` ルート配下に置くことで、
    /// 判別エンジン (DecisionEngine) が実行時に書き込まれる記憶と同じ場所を見るようにする。
    pub fn persistent_store_path(&self) -> PathBuf {
        self.root.join("memory_store.json")
    }

    /// `core_types::CanonicalReuseResolver` のスナップショット保存先。
    ///
    /// 同じ `.dbm/memory` ルート配下に置くことで、プロセス再起動をまたいだ
    /// canonical dedup (followup 解決) の索引が単一の記憶ルートに統合される。
    pub fn canonical_reuse_resolver_path(&self) -> PathBuf {
        self.root.join("canonical_reuse_resolver.json")
    }

    pub fn load_canonical(&self) -> Result<Vec<CanonicalMemoryRecord>, String> {
        read_jsonl(&self.canonical_path())
    }

    pub fn load_index(&self) -> Result<GlobalMemoryIndex, String> {
        if !self.index_path().exists() {
            return Ok(GlobalMemoryIndex::default());
        }
        let body = fs::read_to_string(self.index_path()).map_err(|err| err.to_string())?;
        serde_json::from_str(&body).map_err(|err| err.to_string())
    }

    pub fn rebuild_index(&self) -> Result<GlobalMemoryIndex, String> {
        let records = self.load_canonical()?;
        let index = self.rebuild_index_with_history(&records)?;
        self.persist(&records, &index)?;
        Ok(index)
    }

    pub fn insert(
        &self,
        memory: MemoryEntry,
        source_hash: String,
        canonical_key: String,
        now: u64,
    ) -> Result<CanonicalInsertResult, String> {
        let mut records = self.load_canonical()?;
        let mut index = self.load_or_rebuild_index(&records)?;

        if let Some(canonical_id) = index.by_source_hash.get(&source_hash)
            && let Some(position) = record_position(&records, canonical_id)
        {
            return self.reinforce(
                &mut records,
                &mut index,
                position,
                memory,
                DuplicateClass::ExactDuplicate,
                1.0,
                now,
            );
        }

        if let Some(canonical_id) = index.by_canonical_key.get(&canonical_key)
            && let Some(position) = record_position(&records, canonical_id)
        {
            let existing_canonical_id = canonical_id.clone();
            let resonance = memory_resonance(&memory, &records[position].entry);
            if resonance < self.policy.conflict_threshold {
                return self.insert_conflict(
                    &mut records,
                    &mut index,
                    memory,
                    source_hash,
                    canonical_key,
                    existing_canonical_id,
                    resonance,
                    now,
                );
            }
            return self.reinforce(
                &mut records,
                &mut index,
                position,
                memory,
                DuplicateClass::SemanticDuplicate,
                resonance,
                now,
            );
        }

        if let Some((position, resonance)) = records
            .iter()
            .enumerate()
            .filter_map(|(position, record)| {
                let resonance = memory_resonance(&memory, &record.entry);
                (resonance >= self.policy.semantic_duplicate_threshold).then_some((position, resonance))
            })
            .max_by(|left, right| left.1.total_cmp(&right.1))
        {
            return self.reinforce(
                &mut records,
                &mut index,
                position,
                memory,
                DuplicateClass::SemanticDuplicate,
                resonance,
                now,
            );
        }

        let record = new_canonical(memory, source_hash, canonical_key, now);
        index_record(&mut index, &record, true);
        records.push(record.clone());
        self.persist(&records, &index)?;
        Ok(CanonicalInsertResult {
            canonical: record,
            duplicate_class: DuplicateClass::None,
            resonance: 0.0,
            inserted: true,
            conflicting_canonical_id: None,
        })
    }

    pub fn migrate<I, F, K>(
        &self,
        memories: I,
        mut source_hash: F,
        mut canonical_key: K,
    ) -> Result<usize, String>
    where
        I: IntoIterator<Item = MemoryEntry>,
        F: FnMut(&MemoryEntry) -> String,
        K: FnMut(&MemoryEntry) -> String,
    {
        if self.index_path().exists() || self.canonical_path().exists() {
            return Ok(0);
        }
        let mut migrated = 0;
        for memory in memories {
            let timestamp = memory.metadata.timestamp;
            let hash = source_hash(&memory);
            let key = canonical_key(&memory);
            self.insert(memory, hash, key, timestamp)?;
            migrated += 1;
        }
        Ok(migrated)
    }

    pub fn compress(&self, retention_window: u64, now: u64) -> Result<usize, String> {
        let records = self.load_canonical()?;
        let (expired, retained): (Vec<_>, Vec<_>) = records.into_iter().partition(|record| {
            record.canonical.reinforcement_count == 1
                && now.saturating_sub(record.canonical.last_seen_at) > retention_window
        });
        if expired.is_empty() {
            return Ok(0);
        }
        let archive_path = self.archived_path();
        for record in &expired {
            append_jsonl(&archive_path, record)?;
        }
        let index = self.rebuild_index_with_history(&retained)?;
        self.persist(&retained, &index)?;
        Ok(expired.len())
    }

    pub fn update_entry<F>(&self, memory_id: &str, update: F) -> Result<bool, String>
    where
        F: FnOnce(&mut MemoryEntry),
    {
        let mut records = self.load_canonical()?;
        let Some(record) = records.iter_mut().find(|record| {
            record.entry.memory_id == memory_id
                || record
                    .canonical
                    .memory_ids
                    .iter()
                    .any(|candidate| candidate == memory_id)
        }) else {
            return Ok(false);
        };
        update(&mut record.entry);
        let index = self.load_or_rebuild_index(&records)?;
        self.persist(&records, &index)?;
        Ok(true)
    }

    fn reinforce(
        &self,
        records: &mut [CanonicalMemoryRecord],
        index: &mut GlobalMemoryIndex,
        position: usize,
        memory: MemoryEntry,
        duplicate_class: DuplicateClass,
        resonance: f32,
        now: u64,
    ) -> Result<CanonicalInsertResult, String> {
        let record = &mut records[position];
        let incoming_source_hash = memory_source_hash(&memory);
        record.canonical.reinforcement_count =
            record.canonical.reinforcement_count.saturating_add(1);
        record.canonical.last_seen_at = now;
        if !record.canonical.memory_ids.contains(&memory.memory_id) {
            record.canonical.memory_ids.push(memory.memory_id.clone());
        }
        let canonical = record.clone();
        index.by_source_hash.insert(
            incoming_source_hash.clone(),
            canonical.canonical.canonical_id.clone(),
        );
        self.persist(records, index)?;
        append_jsonl(
            &self.reinforcement_path(),
            &ReinforcementRecord {
                canonical_id: canonical.canonical.canonical_id.clone(),
                memory_id: memory.memory_id,
                source_hash: incoming_source_hash,
                duplicate_class: duplicate_class.clone(),
                resonance,
                reinforced_at: now,
            },
        )?;
        Ok(CanonicalInsertResult {
            canonical,
            duplicate_class,
            resonance,
            inserted: false,
            conflicting_canonical_id: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_conflict(
        &self,
        records: &mut Vec<CanonicalMemoryRecord>,
        index: &mut GlobalMemoryIndex,
        memory: MemoryEntry,
        source_hash: String,
        canonical_key: String,
        existing_canonical_id: String,
        resonance: f32,
        now: u64,
    ) -> Result<CanonicalInsertResult, String> {
        let record = new_canonical(memory, source_hash, canonical_key.clone(), now);
        index_record(index, &record, false);
        records.push(record.clone());
        self.persist(records, index)?;
        append_jsonl(
            &self.conflicts_path(),
            &MemoryConflictRecord {
                existing_canonical_id: existing_canonical_id.clone(),
                incoming_canonical_id: record.canonical.canonical_id.clone(),
                incoming_memory_id: record.entry.memory_id.clone(),
                canonical_key,
                resonance,
                detected_at: now,
            },
        )?;
        Ok(CanonicalInsertResult {
            canonical: record,
            duplicate_class: DuplicateClass::ConflictCandidate,
            resonance,
            inserted: true,
            conflicting_canonical_id: Some(existing_canonical_id),
        })
    }

    fn load_or_rebuild_index(
        &self,
        records: &[CanonicalMemoryRecord],
    ) -> Result<GlobalMemoryIndex, String> {
        let index = self.load_index()?;
        if !index.by_source_hash.is_empty()
            || !index.by_canonical_key.is_empty()
            || records.is_empty()
        {
            return Ok(index);
        }
        let index = self.rebuild_index_with_history(records)?;
        write_json(&self.index_path(), &index)?;
        Ok(index)
    }

    fn rebuild_index_with_history(
        &self,
        records: &[CanonicalMemoryRecord],
    ) -> Result<GlobalMemoryIndex, String> {
        let mut index = rebuild_index(records);
        let canonical_ids = records
            .iter()
            .map(|record| record.canonical.canonical_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        for reinforcement in read_jsonl::<ReinforcementRecord>(&self.reinforcement_path())? {
            if !reinforcement.source_hash.is_empty()
                && canonical_ids.contains(reinforcement.canonical_id.as_str())
            {
                index
                    .by_source_hash
                    .insert(reinforcement.source_hash, reinforcement.canonical_id);
            }
        }
        Ok(index)
    }

    fn persist(
        &self,
        records: &[CanonicalMemoryRecord],
        index: &GlobalMemoryIndex,
    ) -> Result<(), String> {
        write_jsonl(&self.canonical_path(), records)?;
        write_json(&self.index_path(), index)?;
        ensure_jsonl_exists(&self.reinforcement_path())?;
        ensure_jsonl_exists(&self.conflicts_path())?;
        ensure_jsonl_exists(&self.archived_path())
    }

    /// `core_types::CanonicalReuseResolver` をディスクへ保存する (アトミック書き込み)。
    ///
    /// プロセス再起動をまたいでも canonical dedup / followup 解決の索引が
    /// 引き継がれるようにする。
    pub fn save_canonical_reuse_resolver(
        &self,
        resolver: &core_types::CanonicalReuseResolver,
    ) -> Result<(), String> {
        write_json(&self.canonical_reuse_resolver_path(), &resolver.to_snapshot())
    }

    /// ディスクに保存済みのスナップショットがあれば復元し、なければ空のリゾルバを返す。
    /// 破損したスナップショットも (エラーにせず) 空のリゾルバにフォールバックする。
    pub fn load_or_new_canonical_reuse_resolver(&self) -> core_types::CanonicalReuseResolver {
        let path = self.canonical_reuse_resolver_path();
        let Ok(body) = fs::read_to_string(&path) else {
            return core_types::CanonicalReuseResolver::new();
        };
        match serde_json::from_str(&body) {
            Ok(snapshot) => core_types::CanonicalReuseResolver::from_snapshot(snapshot),
            Err(_) => core_types::CanonicalReuseResolver::new(),
        }
    }
}

fn new_canonical(
    memory: MemoryEntry,
    source_hash: String,
    canonical_key: String,
    now: u64,
) -> CanonicalMemoryRecord {
    CanonicalMemoryRecord {
        canonical: CanonicalMemory {
            canonical_id: format!("canonical:{source_hash}"),
            source_hash,
            canonical_key,
            reinforcement_count: 1,
            first_seen_at: now,
            last_seen_at: now,
            memory_ids: vec![memory.memory_id.clone()],
        },
        entry: memory,
    }
}

fn record_position(records: &[CanonicalMemoryRecord], canonical_id: &str) -> Option<usize> {
    records
        .iter()
        .position(|record| record.canonical.canonical_id == canonical_id)
}

fn rebuild_index(records: &[CanonicalMemoryRecord]) -> GlobalMemoryIndex {
    let mut index = GlobalMemoryIndex::default();
    for record in records {
        index_record(&mut index, record, true);
    }
    index
}

fn index_record(
    index: &mut GlobalMemoryIndex,
    record: &CanonicalMemoryRecord,
    index_canonical_key: bool,
) {
    index.by_source_hash.insert(
        record.canonical.source_hash.clone(),
        record.canonical.canonical_id.clone(),
    );
    if index_canonical_key && !record.canonical.canonical_key.is_empty() {
        index.by_canonical_key.insert(
            record.canonical.canonical_key.clone(),
            record.canonical.canonical_id.clone(),
        );
    }
}

fn memory_source_hash(memory: &MemoryEntry) -> String {
    use sha2::{Digest, Sha256};
    let encoded = serde_json::to_string(&memory.content).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(encoded.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn memory_resonance(left: &MemoryEntry, right: &MemoryEntry) -> f32 {
    let (Some(left), Some(right)) = (left.embedding.as_ref(), right.embedding.as_ref()) else {
        return 0.0;
    };
    let len = left.len().min(right.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for index in 0..len {
        dot += left[index] * right[index];
        left_norm += left[index] * left[index];
        right_norm += right[index] * right[index];
    }
    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        return 0.0;
    }
    (dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(-1.0, 1.0)
}

fn read_jsonl<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let reader = BufReader::new(File::open(path).map_err(|err| err.to_string())?);
    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|err| err.to_string())?;
        if !line.trim().is_empty() {
            records.push(serde_json::from_str(&line).map_err(|err| err.to_string())?);
        }
    }
    Ok(records)
}

fn write_jsonl<T: Serialize>(path: &Path, records: &[T]) -> Result<(), String> {
    ensure_parent(path)?;
    let temporary = temporary_path(path);
    let mut file = File::create(&temporary).map_err(|err| err.to_string())?;
    for record in records {
        let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
        writeln!(file, "{line}").map_err(|err| err.to_string())?;
    }
    fs::rename(temporary, path).map_err(|err| err.to_string())
}

fn append_jsonl<T: Serialize>(path: &Path, record: &T) -> Result<(), String> {
    ensure_parent(path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    ensure_parent(path)?;
    let temporary = temporary_path(path);
    let body = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    fs::write(&temporary, body).map_err(|err| err.to_string())?;
    fs::rename(temporary, path).map_err(|err| err.to_string())
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn ensure_jsonl_exists(path: &Path) -> Result<(), String> {
    ensure_parent(path)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn temporary_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("data");
    path.with_extension(format!("{extension}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::ir::{MemoryMetadata, MemoryType};

    fn memory(id: &str, content: &str, embedding: Vec<f32>, timestamp: u64) -> MemoryEntry {
        MemoryEntry {
            memory_id: id.to_string(),
            source_event: Uuid::new_v4(),
            memory_type: MemoryType::SemanticHint,
            content: serde_json::json!({ "content": content }),
            embedding: Some(embedding),
            success_count: 0,
            failure_count: 0,
            metadata: MemoryMetadata {
                timestamp,
                step_index: 0,
                relevance: 1.0,
                tags: vec!["shared-key".to_string()],
            },
        }
    }

    #[test]
    fn exact_duplicate_reinforces_one_canonical_one_hundred_times() {
        let dir = tempdir().expect("tempdir");
        let store = GlobalHolographicMemoryStore::default_for_workspace(dir.path());
        for index in 0..100 {
            let result = store
                .insert(
                    memory(&format!("mem-{index}"), "same", vec![1.0, 0.0], index + 1),
                    "same-hash".to_string(),
                    "shared-key".to_string(),
                    index + 1,
                )
                .expect("insert");
            if index == 0 {
                assert_eq!(result.duplicate_class, DuplicateClass::None);
            } else {
                assert_eq!(result.duplicate_class, DuplicateClass::ExactDuplicate);
            }
        }

        let canonical = store.load_canonical().expect("canonical");
        assert_eq!(canonical.len(), 1);
        assert_eq!(canonical[0].canonical.reinforcement_count, 100);
        assert_eq!(
            read_jsonl::<ReinforcementRecord>(&store.reinforcement_path())
                .expect("reinforcements")
                .len(),
            99
        );
    }

    #[test]
    fn canonical_key_duplicate_is_semantic() {
        let dir = tempdir().expect("tempdir");
        let store = GlobalHolographicMemoryStore::default_for_workspace(dir.path());
        store
            .insert(
                memory("mem-a", "alpha", vec![1.0, 0.0], 1),
                "hash-a".to_string(),
                "shared-key".to_string(),
                1,
            )
            .expect("first");
        let result = store
            .insert(
                memory("mem-b", "beta", vec![0.8, 0.6], 2),
                "hash-b".to_string(),
                "shared-key".to_string(),
                2,
            )
            .expect("second");

        assert_eq!(result.duplicate_class, DuplicateClass::SemanticDuplicate);
        assert_eq!(store.load_canonical().expect("canonical").len(), 1);
    }

    #[test]
    fn low_resonance_same_key_is_conflict_candidate() {
        let dir = tempdir().expect("tempdir");
        let store = GlobalHolographicMemoryStore::default_for_workspace(dir.path());
        store
            .insert(
                memory("mem-a", "alpha", vec![1.0, 0.0], 1),
                "hash-a".to_string(),
                "shared-key".to_string(),
                1,
            )
            .expect("first");
        let result = store
            .insert(
                memory("mem-b", "beta", vec![0.0, 1.0], 2),
                "hash-b".to_string(),
                "shared-key".to_string(),
                2,
            )
            .expect("second");

        assert_eq!(result.duplicate_class, DuplicateClass::ConflictCandidate);
        assert_eq!(store.load_canonical().expect("canonical").len(), 2);
        assert_eq!(
            read_jsonl::<MemoryConflictRecord>(&store.conflicts_path())
                .expect("conflicts")
                .len(),
            1
        );
    }

    /// `MemoryPolicy` (系統B と共有する設定) を差し替えると、系統D の
    /// セマンティック重複判定 (canonical_key が異なる記憶間のフォールバック走査) の
    /// 挙動が実際に変わることを確認する。閾値が構造体上に存在するだけで
    /// 判定に反映されていない、という統合漏れを防ぐための回帰テスト。
    #[test]
    fn semantic_duplicate_threshold_is_driven_by_shared_memory_policy() {
        let mut alpha = memory("mem-alpha", "alpha", vec![1.0, 0.0], 1);
        alpha.metadata.tags = vec!["key-a".to_string()];
        let mut beta = memory("mem-beta", "beta", vec![0.95, 0.3122], 2);
        beta.metadata.tags = vec!["key-b".to_string()];
        // cosine(alpha, beta) ≈ 0.950: DEFAULT (0.92) 以上・厳格化後 (0.999) 未満
        let cosine = memory_resonance(&alpha, &beta);
        assert!(
            (0.92..0.999).contains(&cosine),
            "test fixture must sit strictly between the two thresholds under test, got {cosine}"
        );

        // デフォルトポリシー (0.92) では canonical_key が異なっていても
        // resonance が十分高いのでセマンティック重複として reinforce される。
        let default_dir = tempdir().expect("tempdir");
        let default_store = GlobalHolographicMemoryStore::default_for_workspace(default_dir.path());
        default_store
            .insert(alpha.clone(), "hash-alpha".to_string(), "key-a".to_string(), 1)
            .expect("first");
        let default_result = default_store
            .insert(beta.clone(), "hash-beta".to_string(), "key-b".to_string(), 2)
            .expect("second");
        assert_eq!(default_result.duplicate_class, DuplicateClass::SemanticDuplicate);
        assert_eq!(default_store.load_canonical().expect("canonical").len(), 1);

        // 閾値を 0.999 に厳格化したポリシーでは、同じ埋め込みペアはもう
        // セマンティック重複とみなされず、別の canonical として保存される。
        let strict_dir = tempdir().expect("tempdir");
        let mut strict_policy = memory_persistence::MemoryPolicy::default();
        strict_policy.semantic_duplicate_threshold = 0.999;
        let strict_store =
            GlobalHolographicMemoryStore::with_policy(strict_dir.path(), strict_policy.clone());
        assert_eq!(strict_store.policy(), &strict_policy);
        strict_store
            .insert(alpha, "hash-alpha".to_string(), "key-a".to_string(), 1)
            .expect("first");
        let strict_result = strict_store
            .insert(beta, "hash-beta".to_string(), "key-b".to_string(), 2)
            .expect("second");
        assert_eq!(strict_result.duplicate_class, DuplicateClass::None);
        assert_eq!(strict_store.load_canonical().expect("canonical").len(), 2);
    }

    #[test]
    fn compression_archives_unreinforced_expired_memory() {
        let dir = tempdir().expect("tempdir");
        let store = GlobalHolographicMemoryStore::default_for_workspace(dir.path());
        store
            .insert(
                memory("mem-old", "old", vec![1.0, 0.0], 1),
                "old-hash".to_string(),
                "old-key".to_string(),
                1,
            )
            .expect("insert");

        assert_eq!(store.compress(10, 20).expect("compress"), 1);
        assert!(store.load_canonical().expect("canonical").is_empty());
        assert!(
            dir.path()
                .join(".dbm/memory/archived_canonical_memory.jsonl")
                .exists()
        );
    }

    #[test]
    fn persistence_layout_is_created() {
        let dir = tempdir().expect("tempdir");
        let store = GlobalHolographicMemoryStore::default_for_workspace(dir.path());
        store
            .insert(
                memory("mem-a", "alpha", vec![1.0, 0.0], 1),
                "hash-a".to_string(),
                "key-a".to_string(),
                1,
            )
            .expect("insert");

        assert!(store.canonical_path().exists());
        assert!(store.index_path().exists());
        assert!(store.reinforcement_path().exists());
        assert!(store.conflicts_path().exists());
        assert!(store.archived_path().exists());
    }

    #[test]
    fn subdirectory_input_still_uses_workspace_memory_root() {
        let dir = tempdir().expect("tempdir");
        core_types::WorkspaceRoot::ensure_layout(dir.path()).expect("layout");
        let nested = dir.path().join("apps/cli/src");
        fs::create_dir_all(&nested).expect("nested");
        let store = GlobalHolographicMemoryStore::default_for_workspace(&nested);
        store
            .insert(
                memory("mem-a", "alpha", vec![1.0, 0.0], 1),
                "hash-a".to_string(),
                "key-a".to_string(),
                1,
            )
            .expect("insert");

        assert!(
            dir.path()
                .join(".dbm/memory/canonical_memory.jsonl")
                .exists()
        );
        assert!(!nested.join(".dbm").exists());
    }
}
