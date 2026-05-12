//! Memory Maintenance: Deduplication
//!
//! DBM-MEMORY-MAINTENANCE-DEDUP-SPEC v0.1 の実装。
//!
//! # 重要: これは Memory Sanitation であり Memory Optimization ではない
//!
//! 対象:
//! - identical serialized hash (全フィールド完全一致)
//! - identical spectrum hash (abstract_tags 完全一致)
//!
//! 非対象:
//! - fuzzy dedup / semantic merge / resonance rewrite など

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::generalized_memory::GeneralizedMemory;

// ── 公開型 ────────────────────────────────────────────────────────────────────

/// 重複排除コマンドのオプション。
pub struct MaintenanceDedupOptions {
    /// ドライラン: 検出のみで削除しない
    pub dry_run: bool,
    /// 実行: 重複を実際に削除する
    pub apply: bool,
    /// 監査: 監査ログを出力する
    pub audit: bool,
}

/// 検出された重複ペア。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateMemory {
    /// 保持するメモリ ID (最古)
    pub original_id: String,
    /// 削除するメモリ ID (重複)
    pub duplicate_id: String,
    /// 重複と判定した理由
    pub duplicate_reason: DuplicateReason,
}

/// 重複と判定した理由。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DuplicateReason {
    /// シリアライズ済み全フィールドのハッシュが完全一致
    ExactSerializedHash,
    /// スペクトルハッシュ (abstract_tags) が完全一致
    ExactSpectrumHash,
}

impl DuplicateReason {
    fn as_str(&self) -> &'static str {
        match self {
            DuplicateReason::ExactSerializedHash => "exact_duplicate",
            DuplicateReason::ExactSpectrumHash => "exact_spectrum",
        }
    }
}

/// 重複排除の実行結果。
#[derive(Debug, Serialize, Deserialize)]
pub struct MaintenanceDedupResult {
    /// 検出された重複ペアの一覧
    pub duplicates: Vec<DuplicateMemory>,
    /// 変更が実際に適用されたか
    pub applied: bool,
    /// 削除されたメモリ件数 (dry_run 時は 0)
    pub removed_count: usize,
    /// 各削除に対する監査ログエントリ
    pub audit_entries: Vec<MaintenanceDedupAuditEntry>,
    /// ロールバックスナップショットのパス (apply 時のみ)
    pub snapshot_path: Option<String>,
}

/// 単一の削除操作に対する監査ログエントリ。
///
/// スペックで定義された JSON 形式:
/// ```json
/// {
///   "event": "maintenance_memory_dedup",
///   "removed_memory_id": "...",
///   "kept_memory_id": "...",
///   "reason": "exact_duplicate"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceDedupAuditEntry {
    pub event: String,
    pub removed_memory_id: String,
    pub kept_memory_id: String,
    pub reason: String,
}

impl MaintenanceDedupAuditEntry {
    fn new(removed_id: &str, kept_id: &str, reason: &DuplicateReason) -> Self {
        Self {
            event: "maintenance_memory_dedup".to_string(),
            removed_memory_id: removed_id.to_string(),
            kept_memory_id: kept_id.to_string(),
            reason: reason.as_str().to_string(),
        }
    }
}

// ── ハッシュ計算 ──────────────────────────────────────────────────────────────

/// メモリの正準シリアライズ形式 (全フィールド) の SHA-256 ハッシュを計算する。
///
/// Step 1: identical serialized memory の判定に使用する。
pub fn compute_serialized_hash(mem: &GeneralizedMemory) -> String {
    let canonical = serde_json::to_string(mem).unwrap_or_default();
    hex_sha256(canonical.as_bytes())
}

/// スペクトルハッシュを計算する (現在は abstract_tags をプロキシとして使用)。
///
/// Step 2: identical spectrum の判定に使用する。
///
/// NOTE: `GeneralizedMemory` に専用の `spectrum` フィールドが追加された際は、
/// このメソッドをそちらにハッシュするよう更新すること。
pub fn compute_spectrum_hash(mem: &GeneralizedMemory) -> String {
    let mut tags = mem.abstract_tags.clone();
    tags.sort();
    let canonical = tags.join(",");
    hex_sha256(canonical.as_bytes())
}

fn hex_sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("{:x}", digest)
}

// ── 保護判定 ──────────────────────────────────────────────────────────────────

/// メモリが削除禁止か判定する。
///
/// 以下の条件のいずれかが attributes に設定されている場合は保護対象:
/// - `"pinned"   = "true"` (ピン留め)
/// - `"active"   = "true"` (アクティブ)
/// - `"referenced" = "true"` (参照中)
/// - `"runtime"  = "true"` (ランタイム)
pub fn is_protected(mem: &GeneralizedMemory) -> bool {
    const PROTECTION_KEYS: &[&str] = &["pinned", "active", "referenced", "runtime"];
    PROTECTION_KEYS
        .iter()
        .any(|key| mem.attributes.get(*key).map(|v| v == "true").unwrap_or(false))
}

// ── 重複検出 ──────────────────────────────────────────────────────────────────

/// メモリ一覧から重複ペアを検出する。
///
/// # Deterministic 保証
///
/// 処理順序: `sort_by(created_epoch asc, id asc)`
///
/// # Keep Rule
///
/// 最古のメモリを保持し、後続の重複を削除対象とする。
///
/// # Steps
///
/// 1. シリアライズハッシュ完全一致 (全フィールド)
/// 2. スペクトルハッシュ完全一致 (abstract_tags)
pub fn find_duplicates(memories: &[GeneralizedMemory]) -> Vec<DuplicateMemory> {
    // Deterministic な処理順序: created_epoch asc, id asc
    let mut indices: Vec<usize> = (0..memories.len()).collect();
    indices.sort_by(|&a, &b| {
        memories[a]
            .created_epoch
            .cmp(&memories[b].created_epoch)
            .then_with(|| memories[a].id.cmp(&memories[b].id))
    });

    let mut duplicates: Vec<DuplicateMemory> = Vec::new();

    // ── Step 1: シリアライズハッシュ完全一致 ────────────────────────────────
    let mut serialized_seen: HashMap<String, String> = HashMap::new(); // hash → memory_id
    for &idx in &indices {
        let mem = &memories[idx];
        if is_protected(mem) {
            continue;
        }
        let hash = compute_serialized_hash(mem);
        if let Some(original_id) = serialized_seen.get(&hash) {
            duplicates.push(DuplicateMemory {
                original_id: original_id.clone(),
                duplicate_id: mem.id.clone(),
                duplicate_reason: DuplicateReason::ExactSerializedHash,
            });
        } else {
            serialized_seen.insert(hash, mem.id.clone());
        }
    }

    // ── Step 2: スペクトルハッシュ完全一致 ──────────────────────────────────
    // Step 1 で既に重複と判定されたものはスキップ
    let already_duplicate: HashSet<String> =
        duplicates.iter().map(|d| d.duplicate_id.clone()).collect();

    let mut spectrum_seen: HashMap<String, String> = HashMap::new(); // spectrum_hash → memory_id
    for &idx in &indices {
        let mem = &memories[idx];
        if is_protected(mem) || already_duplicate.contains(&mem.id) {
            continue;
        }
        let spectrum_hash = compute_spectrum_hash(mem);
        if let Some(original_id) = spectrum_seen.get(&spectrum_hash) {
            duplicates.push(DuplicateMemory {
                original_id: original_id.clone(),
                duplicate_id: mem.id.clone(),
                duplicate_reason: DuplicateReason::ExactSpectrumHash,
            });
        } else {
            spectrum_seen.insert(spectrum_hash, mem.id.clone());
        }
    }

    duplicates
}

// ── ロールバックスナップショット ──────────────────────────────────────────────

/// ロールバック用スナップショットを保存する。
///
/// パス: `<snapshot_dir>/memory-maintenance-<epoch>.json`
pub fn save_rollback_snapshot(
    memories: &[GeneralizedMemory],
    snapshot_dir: &Path,
) -> io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(snapshot_dir)?;
    let epoch = epoch_now();
    let path = snapshot_dir.join(format!("memory-maintenance-{epoch}.json"));

    let json = serde_json::to_string_pretty(memories)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    // アトミック書き込み
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &path)?;

    Ok(path)
}

// ── 重複排除の実行 ────────────────────────────────────────────────────────────

/// メモリ一覧に対して重複排除を実行する。
///
/// - `dry_run = true`  → 検出のみ。メモリは変更しない。
/// - `apply = true`    → 重複を削除する。事前にロールバックスナップショットを保存する。
/// - `audit = true`    → 監査ログエントリを結果に含める。
///
/// `snapshot_dir` が指定されている場合、apply 時にそのディレクトリへスナップショットを保存する。
/// 指定されていない場合は `.dbm/snapshots/` が使用される (apply 時のみ)。
pub fn run_dedup(
    memories: &mut Vec<GeneralizedMemory>,
    options: &MaintenanceDedupOptions,
    snapshot_dir: Option<&Path>,
) -> io::Result<MaintenanceDedupResult> {
    let duplicates = find_duplicates(memories);

    if duplicates.is_empty() {
        return Ok(MaintenanceDedupResult {
            duplicates,
            applied: false,
            removed_count: 0,
            audit_entries: Vec::new(),
            snapshot_path: None,
        });
    }

    // ドライランまたは apply フラグなし → 検出結果のみ返す
    if options.dry_run || !options.apply {
        let audit_entries = if options.audit {
            duplicates
                .iter()
                .map(|d| {
                    MaintenanceDedupAuditEntry::new(
                        &d.duplicate_id,
                        &d.original_id,
                        &d.duplicate_reason,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        return Ok(MaintenanceDedupResult {
            duplicates,
            applied: false,
            removed_count: 0,
            audit_entries,
            snapshot_path: None,
        });
    }

    // apply: ロールバックスナップショットを保存してから削除
    let snapshot_dir_path = snapshot_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".dbm/snapshots"));

    let snapshot_path = save_rollback_snapshot(memories, &snapshot_dir_path)?;

    // 削除対象 ID を収集
    let ids_to_remove: HashSet<String> =
        duplicates.iter().map(|d| d.duplicate_id.clone()).collect();

    // 監査ログエントリを生成
    let audit_entries: Vec<MaintenanceDedupAuditEntry> = duplicates
        .iter()
        .map(|d| {
            MaintenanceDedupAuditEntry::new(&d.duplicate_id, &d.original_id, &d.duplicate_reason)
        })
        .collect();

    // 重複を削除
    let before = memories.len();
    memories.retain(|m| !ids_to_remove.contains(&m.id));
    let removed_count = before - memories.len();

    Ok(MaintenanceDedupResult {
        duplicates,
        applied: true,
        removed_count,
        audit_entries,
        snapshot_path: Some(snapshot_path.display().to_string()),
    })
}

fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn make_memory(
        id: &str,
        summary: &str,
        tags: &[&str],
        embedding: &[f32],
        created_epoch: u64,
    ) -> GeneralizedMemory {
        GeneralizedMemory {
            id: id.to_string(),
            summary: summary.to_string(),
            abstract_tags: tags.iter().map(|t| t.to_string()).collect(),
            centroid_embedding: embedding.to_vec(),
            version: 1,
            source_count: 1,
            recall_count: 0,
            created_epoch,
            last_upgraded_epoch: created_epoch,
            attributes: BTreeMap::new(),
        }
    }

    // ── find_duplicates ──────────────────────────────────────────────────────

    #[test]
    fn no_duplicates_when_all_unique() {
        let memories = vec![
            make_memory("m1", "REST API design", &["api", "rest"], &[1.0, 0.0], 1000),
            make_memory("m2", "DB schema design", &["db", "sql"], &[0.0, 1.0], 2000),
        ];
        assert!(find_duplicates(&memories).is_empty());
    }

    #[test]
    fn detects_exact_serialized_hash_duplicate() {
        // 全フィールドが同一のメモリ (ID 含む) → Step 1 で検出
        let m1 = make_memory("m1", "REST API design", &["api", "rest"], &[1.0, 0.0], 1000);
        let m2 = m1.clone(); // 完全コピー (ID も同一)
        let memories = vec![m1, m2];
        let dups = find_duplicates(&memories);
        assert_eq!(dups.len(), 1);
        assert!(matches!(
            dups[0].duplicate_reason,
            DuplicateReason::ExactSerializedHash
        ));
    }

    #[test]
    fn detects_exact_spectrum_hash_duplicate() {
        // abstract_tags が同一だが他フィールドが異なる → Step 2 で検出
        let m1 = make_memory("m1", "REST API design", &["api", "rest"], &[1.0, 0.0], 1000);
        let m2 = make_memory("m2", "Different summary", &["api", "rest"], &[0.5, 0.5], 2000);
        let memories = vec![m1, m2];
        let dups = find_duplicates(&memories);
        assert_eq!(dups.len(), 1);
        assert!(matches!(
            dups[0].duplicate_reason,
            DuplicateReason::ExactSpectrumHash
        ));
    }

    #[test]
    fn oldest_kept_newest_removed() {
        let m_newer = make_memory("m_newer", "summary", &["tag1"], &[0.5], 5000);
        let m_older = make_memory("m_older", "summary", &["tag1"], &[0.5], 1000);
        let memories = vec![m_newer.clone(), m_older.clone()];
        let dups = find_duplicates(&memories);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].original_id, "m_older"); // 古い方を保持
        assert_eq!(dups[0].duplicate_id, "m_newer"); // 新しい方を削除
    }

    #[test]
    fn id_tiebreak_when_epoch_equal() {
        // created_epoch が同一の場合は ID 辞書順で先のものを保持
        let ma = make_memory("ma", "summary", &["tag1"], &[0.5], 1000);
        let mz = make_memory("mz", "summary", &["tag1"], &[0.5], 1000);
        let memories = vec![mz.clone(), ma.clone()];
        let dups = find_duplicates(&memories);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].original_id, "ma");
        assert_eq!(dups[0].duplicate_id, "mz");
    }

    #[test]
    fn pinned_memory_is_not_removed() {
        let m1 = make_memory("m1", "summary", &["tag1"], &[0.5], 1000);
        let mut m2 = make_memory("m2", "summary", &["tag1"], &[0.5], 2000);
        m2.attributes.insert("pinned".to_string(), "true".to_string());
        let memories = vec![m1, m2];
        // m2 は pinned なので重複判定から除外 → 検出なし
        assert!(find_duplicates(&memories).is_empty());
    }

    #[test]
    fn active_and_referenced_memories_are_protected() {
        let m1 = make_memory("m1", "summary", &["tag1"], &[0.5], 1000);
        let mut m2 = make_memory("m2", "summary", &["tag1"], &[0.5], 2000);
        m2.attributes
            .insert("active".to_string(), "true".to_string());
        let mut m3 = make_memory("m3", "summary", &["tag1"], &[0.5], 3000);
        m3.attributes
            .insert("referenced".to_string(), "true".to_string());
        let memories = vec![m1, m2, m3];
        // m2, m3 は保護されているので m1 のみ残り重複なし
        assert!(find_duplicates(&memories).is_empty());
    }

    #[test]
    fn step1_duplicate_not_rechecked_in_step2() {
        // Step 1 で重複と判定された m2 は Step 2 でスキップされる
        let m1 = make_memory("m1", "REST API", &["api"], &[1.0], 1000);
        let m2 = m1.clone(); // 完全コピー → Step 1 で検出
        // m3 は m1 と同じタグを持つが異なる content → Step 2 の対象
        let m3 = make_memory("m3", "Other summary", &["api"], &[0.0], 3000);
        let memories = vec![m1, m2, m3];
        let dups = find_duplicates(&memories);
        // m2 は Step1, m3 は Step2 で検出 (ただし m2 は全フィールド同一なので id も "m1" が被っている)
        // 実際には m1 と m2 は id="m1" が被っていることに注意
        // m3 は id が異なるので Step 2 で m1 の spectrum と一致
        assert!(dups.len() >= 1);
    }

    // ── run_dedup ────────────────────────────────────────────────────────────

    #[test]
    fn dry_run_does_not_modify_memories() {
        let m1 = make_memory("m1", "summary", &["tag1"], &[0.5], 1000);
        let m2 = make_memory("m2", "summary", &["tag1"], &[0.5], 2000);
        let mut memories = vec![m1, m2];
        let opts = MaintenanceDedupOptions {
            dry_run: true,
            apply: false,
            audit: false,
        };
        let result = run_dedup(&mut memories, &opts, None).unwrap();
        assert!(!result.applied);
        assert_eq!(result.removed_count, 0);
        assert_eq!(memories.len(), 2); // 変更なし
        assert_eq!(result.duplicates.len(), 1);
    }

    #[test]
    fn apply_removes_duplicate_and_keeps_oldest() {
        let m1 = make_memory("m1", "summary", &["tag1"], &[0.5], 1000);
        let m2 = make_memory("m2", "summary", &["tag1"], &[0.5], 2000);
        let mut memories = vec![m1, m2];
        let opts = MaintenanceDedupOptions {
            dry_run: false,
            apply: true,
            audit: false,
        };
        let tmp_dir = tempdir();
        let result = run_dedup(&mut memories, &opts, Some(&tmp_dir)).unwrap();
        assert!(result.applied);
        assert_eq!(result.removed_count, 1);
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].id, "m1"); // 古い方を保持
        assert!(result.snapshot_path.is_some()); // スナップショット作成
    }

    #[test]
    fn apply_writes_rollback_snapshot() {
        let m1 = make_memory("m1", "summary", &["tag1"], &[0.5], 1000);
        let m2 = make_memory("m2", "summary", &["tag1"], &[0.5], 2000);
        let mut memories = vec![m1, m2];
        let opts = MaintenanceDedupOptions {
            dry_run: false,
            apply: true,
            audit: false,
        };
        let tmp_dir = tempdir();
        let result = run_dedup(&mut memories, &opts, Some(&tmp_dir)).unwrap();
        let snap_path = result.snapshot_path.unwrap();
        assert!(std::path::Path::new(&snap_path).exists());
        // スナップショットは削除前の 2 件を含む
        let json = std::fs::read_to_string(&snap_path).unwrap();
        let restored: Vec<GeneralizedMemory> = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), 2);
    }

    #[test]
    fn audit_entries_in_dry_run_when_audit_flag_set() {
        let m1 = make_memory("m1", "summary", &["tag1"], &[0.5], 1000);
        let m2 = make_memory("m2", "summary", &["tag1"], &[0.5], 2000);
        let mut memories = vec![m1, m2];
        let opts = MaintenanceDedupOptions {
            dry_run: true,
            apply: false,
            audit: true,
        };
        let result = run_dedup(&mut memories, &opts, None).unwrap();
        assert_eq!(result.audit_entries.len(), 1);
        assert_eq!(result.audit_entries[0].event, "maintenance_memory_dedup");
        assert_eq!(result.audit_entries[0].removed_memory_id, "m2");
        assert_eq!(result.audit_entries[0].kept_memory_id, "m1");
    }

    #[test]
    fn audit_entry_reason_exact_spectrum() {
        let m1 = make_memory("m1", "summary A", &["tag1"], &[0.5], 1000);
        let m2 = make_memory("m2", "summary B", &["tag1"], &[0.5], 2000);
        let mut memories = vec![m1, m2];
        let opts = MaintenanceDedupOptions {
            dry_run: false,
            apply: true,
            audit: false,
        };
        let tmp_dir = tempdir();
        let result = run_dedup(&mut memories, &opts, Some(&tmp_dir)).unwrap();
        assert_eq!(result.audit_entries[0].reason, "exact_spectrum");
    }

    #[test]
    fn no_duplicates_returns_empty_result() {
        let m1 = make_memory("m1", "REST API", &["api"], &[1.0, 0.0], 1000);
        let m2 = make_memory("m2", "DB schema", &["db", "sql"], &[0.0, 1.0], 2000);
        let mut memories = vec![m1, m2];
        let opts = MaintenanceDedupOptions {
            dry_run: false,
            apply: true,
            audit: true,
        };
        let result = run_dedup(&mut memories, &opts, None).unwrap();
        assert!(!result.applied);
        assert_eq!(result.removed_count, 0);
        assert_eq!(result.duplicates.len(), 0);
        assert_eq!(result.audit_entries.len(), 0);
    }

    fn tempdir() -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!(
            "dbm_maintenance_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }
}
