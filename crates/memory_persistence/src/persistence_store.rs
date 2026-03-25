//! persistence_store — 永続化記憶ストアの実装
//!
//! [`DecisionEngine`] による意思決定を経由して記憶の取り込みを行う。
//! [`PersistentMemoryStore::save`] / [`PersistentMemoryStore::load`] で
//! JSON スナップショットをディスクに保存・復元できる。

use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use memory_space_phase14::stable_v03::MemoryRecord;
use serde::{Deserialize, Serialize};

use crate::generalized_memory::GeneralizedMemory;
use crate::optimizer::{
    DecisionAction, DecisionEngine, DecisionEvidence, DecisionPolicy, IngestResult,
};
use crate::similarity::combined_similarity;

/// 最適化処理の累積統計
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OptimizationStats {
    /// 取り込まれた記憶の総数
    pub total_ingested: usize,
    /// 新規保存された記憶の数
    pub total_stored: usize,
    /// アップグレードされた記憶の数
    pub total_upgraded: usize,
    /// 重複としてスキップされた記憶の数
    pub total_skipped: usize,
    /// 最後に取り込まれた記憶の ID
    pub last_ingest_id: Option<String>,
}

/// 取り込みトランザクションの記録 (監査ログ用)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IngestAuditEntry {
    /// 元記録の ID
    pub source_id: String,
    /// 意思決定結果
    pub result: IngestResult,
    /// 意思決定の根拠
    pub evidence: DecisionEvidence,
}

/// 永続化記憶の最適化ストア
///
/// [`DecisionEngine`] を内包し、多基準意思決定によって
/// 記憶の保存 / アップグレード / 重複排除を管理する。
///
/// # 動作フロー
///
/// ```text
/// ingest(record)
///   ↓
///   DecisionEngine.decide(...)
///   ├── DecisionAction::StoreNew      → GeneralizedMemory::from_record で新規保存
///   ├── DecisionAction::Upgrade { i } → memories[i].upgrade(...)
///   └── DecisionAction::Skip   { i } → 何もしない (統計のみ更新)
/// ```
#[derive(Debug)]
pub struct PersistentMemoryStore {
    memories: Vec<GeneralizedMemory>,
    stats: OptimizationStats,
    engine: DecisionEngine,
    audit_log: Vec<IngestAuditEntry>,
}

impl Default for PersistentMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentMemoryStore {
    /// デフォルトポリシーでストアを作成する。
    pub fn new() -> Self {
        Self::with_policy(DecisionPolicy::default())
    }

    /// カスタムポリシーでストアを作成する。
    pub fn with_policy(policy: DecisionPolicy) -> Self {
        Self {
            memories: Vec::new(),
            stats: OptimizationStats::default(),
            engine: DecisionEngine::new(policy),
            audit_log: Vec::new(),
        }
    }

    /// 現在の意思決定ポリシーを返す。
    pub fn policy(&self) -> &DecisionPolicy {
        &self.engine.policy
    }

    // ── 取り込み ────────────────────────────────────────────────────────────

    /// 元記憶を取り込み、意思決定エンジンによるユニーク判定・アップグレード・重複排除を行う。
    pub fn ingest(&mut self, record: &MemoryRecord) -> IngestResult {
        self.stats.total_ingested += 1;

        let record_tags: Vec<String> = record
            .tags
            .iter()
            .map(|t| t.to_ascii_lowercase())
            .collect();
        let record_embedding: Vec<f32> = record.embedding.clone().unwrap_or_default();

        let (action, evidence) = self.engine.decide(
            &self.memories,
            &record_tags,
            &record_embedding,
            &record.text,
        );

        let result = match action {
            DecisionAction::StoreNew => {
                let id = generate_id(&record.id, self.memories.len());
                let mem = GeneralizedMemory::from_record(
                    id.clone(),
                    &record.text,
                    &record_tags,
                    &record_embedding,
                );
                self.memories.push(mem);
                self.stats.total_stored += 1;
                IngestResult::Stored(id)
            }

            DecisionAction::Upgrade { target_idx } => {
                self.memories[target_idx].upgrade(
                    &record.text,
                    &record_tags,
                    &record_embedding,
                );
                let id = self.memories[target_idx].id.clone();
                let version = self.memories[target_idx].version;
                self.stats.total_upgraded += 1;
                IngestResult::Upgraded { id, version }
            }

            DecisionAction::Skip { matched_idx } => {
                let matched_id = self.memories[matched_idx].id.clone();
                let similarity = evidence
                    .profile
                    .as_ref()
                    .map(|p| p.combined)
                    .unwrap_or(1.0);
                self.stats.total_skipped += 1;
                IngestResult::Skipped { matched_id, similarity }
            }
        };

        if let Some(id) = result.stored_or_upgraded_id() {
            self.stats.last_ingest_id = Some(id.to_string());
        }

        self.audit_log.push(IngestAuditEntry {
            source_id: record.id.clone(),
            result: result.clone(),
            evidence,
        });

        result
    }

    // ── 想起 ────────────────────────────────────────────────────────────────

    /// タグとベクトルで記憶を想起する。上位 top_k 件を返す。
    pub fn recall(
        &mut self,
        query_tags: &[String],
        query_embedding: &[f32],
        top_k: usize,
    ) -> Vec<(&GeneralizedMemory, f32)> {
        let mut scored: Vec<(usize, f32)> = self
            .memories
            .iter()
            .enumerate()
            .map(|(idx, mem)| {
                let sim = combined_similarity(
                    &mem.abstract_tags,
                    &mem.centroid_embedding,
                    query_tags,
                    query_embedding,
                );
                (idx, sim)
            })
            .filter(|(_, sim)| *sim > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        for (idx, _) in &scored {
            self.memories[*idx].bump_recall();
        }

        scored
            .iter()
            .map(|(idx, sim)| (&self.memories[*idx], *sim))
            .collect()
    }

    // ── 参照 ────────────────────────────────────────────────────────────────

    /// 全汎化記憶の一覧を返す。
    pub fn list(&self) -> &[GeneralizedMemory] {
        &self.memories
    }

    /// 累積統計を返す。
    pub fn stats(&self) -> &OptimizationStats {
        &self.stats
    }

    /// 記憶件数を返す。
    pub fn memory_count(&self) -> usize {
        self.memories.len()
    }

    /// ID で記憶を取得する。
    pub fn get_by_id(&self, id: &str) -> Option<&GeneralizedMemory> {
        self.memories.iter().find(|m| m.id == id)
    }

    /// 監査ログを返す。
    pub fn audit_log(&self) -> &[IngestAuditEntry] {
        &self.audit_log
    }

    // ── 検索 ────────────────────────────────────────────────────────────────

    /// キーワード検索 (タグと要約テキストを対象)。
    pub fn search(&self, query: &str) -> Vec<(&GeneralizedMemory, f32)> {
        let q_terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_ascii_lowercase())
            .collect();
        if q_terms.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(&GeneralizedMemory, f32)> = self
            .memories
            .iter()
            .map(|mem| {
                let summary_lower = mem.summary.to_ascii_lowercase();
                let overlap = q_terms
                    .iter()
                    .filter(|t| {
                        mem.abstract_tags.contains(t)
                            || summary_lower.contains(t.as_str())
                    })
                    .count() as f32;
                (mem, overlap / q_terms.len() as f32)
            })
            .filter(|(_, s)| *s > 0.0)
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    // ── メンテナンス ─────────────────────────────────────────────────────────

    /// 想起回数が min_recall 未満の記憶を削除する。削除件数を返す。
    pub fn prune_below_recall(&mut self, min_recall: usize) -> usize {
        let before = self.memories.len();
        self.memories.retain(|m| m.recall_count >= min_recall);
        before - self.memories.len()
    }

    /// バージョン v1 (一度もアップグレードされていない) かつ想起回数 0 の記憶を削除する。
    pub fn prune_stale(&mut self) -> usize {
        let before = self.memories.len();
        self.memories
            .retain(|m| m.version > 1 || m.recall_count > 0);
        before - self.memories.len()
    }

    /// 監査ログをクリアする。
    pub fn clear_audit_log(&mut self) {
        self.audit_log.clear();
    }

    // ── 永続化 (save / load) ─────────────────────────────────────────────────

    /// ストア全体を JSON スナップショットとしてファイルに保存する。
    ///
    /// アトミック書き込みを行う (temp ファイル → rename)。
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let snapshot = MemoryStoreSnapshot {
            format_version: MemoryStoreSnapshot::CURRENT_VERSION,
            saved_epoch: epoch_now(),
            memories: self.memories.clone(),
            stats: self.stats.clone(),
            policy: self.engine.policy.clone(),
            audit_log: self.audit_log.clone(),
        };

        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // アトミック書き込み: 同ディレクトリの temp ファイルに書いてから rename
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// JSON スナップショットファイルからストアを復元する。
    pub fn load(path: &Path) -> io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let snapshot: MemoryStoreSnapshot = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        if snapshot.format_version > MemoryStoreSnapshot::CURRENT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "snapshot format version {} is newer than supported {}",
                    snapshot.format_version,
                    MemoryStoreSnapshot::CURRENT_VERSION
                ),
            ));
        }

        Ok(Self {
            memories: snapshot.memories,
            stats: snapshot.stats,
            engine: DecisionEngine::new(snapshot.policy),
            audit_log: snapshot.audit_log,
        })
    }

    /// ファイルが存在すればロード、存在しなければ空のストアを返す。
    pub fn load_or_new(path: &Path) -> io::Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::new())
        }
    }
}

// ── スナップショット形式 ───────────────────────────────────────────────────────

/// ディスク保存形式。フォーマットバージョンで将来の互換性を管理する。
#[derive(Serialize, Deserialize)]
pub struct MemoryStoreSnapshot {
    /// スナップショット形式バージョン
    pub format_version: u32,
    /// 保存時刻 (Unix エポック秒)
    pub saved_epoch: u64,
    /// 汎化記憶の一覧
    pub memories: Vec<GeneralizedMemory>,
    /// 累積統計
    pub stats: OptimizationStats,
    /// 意思決定ポリシー
    pub policy: DecisionPolicy,
    /// 監査ログ
    pub audit_log: Vec<IngestAuditEntry>,
}

impl MemoryStoreSnapshot {
    pub const CURRENT_VERSION: u32 = 1;
}

fn generate_id(base: &str, count: usize) -> String {
    let clean: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(12)
        .collect();
    format!("gm_{clean}_{count:04}")
}

fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── テスト ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use memory_space_phase14::stable_v03::MemoryRecord;

    fn record(id: &str, text: &str, tags: &[&str], embed: &[f32]) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            text: text.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            embedding: Some(embed.to_vec()),
            architecture: None,
            relations: Vec::new(),
        }
    }

    #[test]
    fn first_record_is_stored() {
        let mut store = PersistentMemoryStore::new();
        let r = record("r1", "Design a REST API", &["api", "rest"], &[1.0, 0.0]);
        let result = store.ingest(&r);
        assert!(result.is_stored());
        assert_eq!(store.memory_count(), 1);
        assert_eq!(store.stats().total_stored, 1);
    }

    #[test]
    fn identical_record_is_skipped() {
        let mut store = PersistentMemoryStore::new();
        let r = record("r1", "Design a REST API", &["api", "rest"], &[1.0, 0.0]);
        store.ingest(&r);
        let result = store.ingest(&r);
        assert!(result.is_skipped());
        assert_eq!(store.memory_count(), 1);
        assert_eq!(store.stats().total_skipped, 1);
    }

    #[test]
    fn similar_record_upgrades_existing() {
        let mut store = PersistentMemoryStore::new();
        let r1 = record("r1", "Design a REST API", &["api", "rest"], &[1.0, 0.0, 0.0]);
        let r2 = record(
            "r2",
            "Add auth to REST API",
            &["api", "rest", "auth"],
            &[0.95, 0.05, 0.0],
        );
        store.ingest(&r1);
        let result = store.ingest(&r2);
        assert!(result.is_upgraded(), "expected Upgraded, got {:?}", result);
        assert_eq!(store.memory_count(), 1);
        assert_eq!(store.stats().total_upgraded, 1);
        assert_eq!(store.list()[0].version, 2);
    }

    #[test]
    fn unique_record_adds_new_entry() {
        let mut store = PersistentMemoryStore::new();
        let r1 = record("r1", "REST API design", &["api"], &[1.0, 0.0]);
        let r2 = record("r2", "Database schema", &["db", "sql"], &[0.0, 1.0]);
        store.ingest(&r1);
        let result = store.ingest(&r2);
        assert!(result.is_stored());
        assert_eq!(store.memory_count(), 2);
    }

    #[test]
    fn audit_log_records_all_ingests() {
        let mut store = PersistentMemoryStore::new();
        let r1 = record("r1", "REST API design", &["api"], &[1.0, 0.0]);
        let r2 = record("r2", "DB schema", &["db"], &[0.0, 1.0]);
        store.ingest(&r1);
        store.ingest(&r2);
        assert_eq!(store.audit_log().len(), 2);
        assert!(store.audit_log()[0].evidence.reason.len() > 0);
    }

    #[test]
    fn prune_stale_removes_unrecalled_v1() {
        let mut store = PersistentMemoryStore::new();
        let r1 = record("r1", "REST API", &["api"], &[1.0, 0.0]);
        let r2 = record("r2", "DB schema", &["db"], &[0.0, 1.0]);
        store.ingest(&r1);
        store.ingest(&r2);
        // r2 を想起してカウントを増やす
        store.recall(&["db".to_string()], &[0.0, 1.0], 1);
        let pruned = store.prune_stale();
        assert_eq!(pruned, 1);
        assert_eq!(store.memory_count(), 1);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "memory_persistence_test_{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // ストアを構築して保存
        let mut store = PersistentMemoryStore::new();
        store.ingest(&record("r1", "REST API design", &["api", "rest"], &[1.0, 0.0]));
        store.ingest(&record("r2", "DB schema", &["db", "sql"], &[0.0, 1.0]));
        store.save(&tmp).expect("save failed");

        // 別インスタンスとしてロードして同一性を検証
        let loaded = PersistentMemoryStore::load(&tmp).expect("load failed");
        assert_eq!(loaded.memory_count(), store.memory_count());
        assert_eq!(loaded.stats().total_stored, store.stats().total_stored);
        assert_eq!(loaded.list()[0].id, store.list()[0].id);
        assert_eq!(loaded.list()[0].abstract_tags, store.list()[0].abstract_tags);
        assert_eq!(loaded.audit_log().len(), store.audit_log().len());

        let _ = std::fs::remove_file(tmp);
    }

    #[test]
    fn load_or_new_returns_empty_when_no_file() {
        let tmp = std::env::temp_dir().join("memory_persistence_nonexistent_xyz.json");
        let _ = std::fs::remove_file(&tmp); // 念のため削除
        let store = PersistentMemoryStore::load_or_new(&tmp).expect("load_or_new failed");
        assert_eq!(store.memory_count(), 0);
    }

    #[test]
    fn saved_epoch_is_set() {
        let tmp = std::env::temp_dir().join(format!(
            "memory_persistence_epoch_{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = PersistentMemoryStore::new();
        store.save(&tmp).expect("save failed");
        let json = std::fs::read_to_string(&tmp).unwrap();
        assert!(json.contains("\"saved_epoch\""));
        assert!(json.contains("\"format_version\""));
        let _ = std::fs::remove_file(tmp);
    }
}
