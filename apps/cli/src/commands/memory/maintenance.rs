//! `dbm memory maintenance` サブコマンド
//!
//! DBM-MEMORY-MAINTENANCE-DEDUP-SPEC v0.1 に基づく Memory Sanitation ツール。
//!
//! # 使い方
//!
//! ```text
//! dbm memory maintenance dedup --dry-run  [--store <path>]
//! dbm memory maintenance dedup --apply    [--store <path>]
//! dbm memory maintenance dedup --audit    [--store <path>]
//! ```

use memory_persistence::{MaintenanceDedupOptions, PersistentMemoryStore, run_dedup};

use crate::command::{CommandError, Output};
use crate::session::AgentSession;

/// `memory maintenance` サブコマンドのエントリポイント。
///
/// 形式: `maintenance dedup [--dry-run | --apply | --audit] [--store <path>]`
pub fn handle_maintenance(
    args: &[String],
    _session: &mut AgentSession,
) -> Result<Output, CommandError> {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("");

    match subcmd {
        "dedup" => handle_dedup(&args[1..]),
        "" => Ok(Output::text(concat!(
            "memory maintenance: subcommand required.\n",
            "Available: dedup\n",
            "\n",
            "Usage:\n",
            "  memory maintenance dedup --dry-run   # 重複を検出して表示 (変更なし)\n",
            "  memory maintenance dedup --apply     # 重複を削除 (ロールバックスナップショット作成)\n",
            "  memory maintenance dedup --audit     # 監査ログ付きで表示",
        ).to_string())),
        other => Err(CommandError::UnknownSubcommand {
            command: "memory maintenance".to_string(),
            subcommand: other.to_string(),
        }),
    }
}

fn handle_dedup(args: &[String]) -> Result<Output, CommandError> {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let apply = args.iter().any(|a| a == "--apply");
    let audit = args.iter().any(|a| a == "--audit");

    // --store <path> パース
    let store_path = parse_flag_value(args, "--store");

    // フラグ未指定時はヘルプを表示
    if !dry_run && !apply && !audit {
        return Ok(Output::text(concat!(
            "memory maintenance dedup: mode flag required.\n",
            "\n",
            "Usage:\n",
            "  memory maintenance dedup --dry-run   # 重複を検出して表示 (変更なし)\n",
            "  memory maintenance dedup --apply     # 重複を削除 (ロールバックスナップショット作成)\n",
            "  memory maintenance dedup --audit     # 監査ログ付きドライラン\n",
            "\n",
            "Options:\n",
            "  --store <path>  永続化ストアのパス (省略時: .dbm/memory_store.json)",
        ).to_string()));
    }

    let effective_store_path = store_path.unwrap_or_else(|| ".dbm/memory_store.json".to_string());
    let store_file = std::path::Path::new(&effective_store_path);

    // ストアを読み込む (ファイルが存在しない場合は空のストアで続行)
    let store = PersistentMemoryStore::load_or_new(store_file)
        .map_err(|e| CommandError::ExecutionError(format!("ストア読み込みエラー: {e}")))?;

    if store.memory_count() == 0 {
        return Ok(Output::text(format!(
            "memory maintenance dedup: ストアが空です。対象メモリなし。\nストア: {effective_store_path}"
        )));
    }

    let opts = MaintenanceDedupOptions {
        dry_run,
        apply,
        audit: audit || dry_run, // dry-run 時は暗黙的に audit ログを含める
    };

    // スナップショット保存先
    let snapshot_dir = std::path::PathBuf::from(".dbm/snapshots");

    // メモリ一覧を取り出して重複排除を実行
    let mut memories: Vec<_> = store.list().to_vec();
    let result = run_dedup(&mut memories, &opts, Some(&snapshot_dir))
        .map_err(|e| CommandError::ExecutionError(format!("重複排除エラー: {e}")))?;

    // apply 時はストアに反映して保存
    if result.applied {
        // 現状: 削除後のメモリ一覧を JSON で直接書き戻す
        // (PersistentMemoryStore は memories の直接設定 API を未公開のため)
        // TODO: PersistentMemoryStore に replace_memories(&[GeneralizedMemory]) を追加する
        write_memories_to_store(store_file, &memories, &result)
            .map_err(|e| CommandError::ExecutionError(format!("ストア書き込みエラー: {e}")))?;
    }

    // 出力を組み立てる
    let output = format_dedup_output(&result, &effective_store_path, dry_run, apply, audit);
    Ok(Output::text(output))
}

/// 重複排除の実行後にストアファイルを更新する。
///
/// PersistentMemoryStore が memories の直接操作 API を持たないため、
/// スナップショット形式のサブセット (memories のみ) を差し替えて書き直す。
fn write_memories_to_store(
    store_file: &std::path::Path,
    memories: &[memory_persistence::GeneralizedMemory],
    _result: &memory_persistence::MaintenanceDedupResult,
) -> std::io::Result<()> {
    // 既存のストアを読み込んで memories だけ入れ替える
    let json_orig = std::fs::read_to_string(store_file).unwrap_or_else(|_| "{}".to_string());

    let mut snapshot: serde_json::Value = serde_json::from_str(&json_orig)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let memories_json = serde_json::to_value(memories)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    if let Some(obj) = snapshot.as_object_mut() {
        obj.insert("memories".to_string(), memories_json);
        // saved_epoch を更新
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        obj.insert(
            "saved_epoch".to_string(),
            serde_json::Value::Number(now.into()),
        );
    }

    let updated_json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    // アトミック書き込み
    let tmp = store_file.with_extension("json.tmp");
    std::fs::write(&tmp, &updated_json)?;
    std::fs::rename(&tmp, store_file)?;

    Ok(())
}

fn format_dedup_output(
    result: &memory_persistence::MaintenanceDedupResult,
    store_path: &str,
    dry_run: bool,
    _apply: bool,
    _audit: bool,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push("=== DBM Memory Maintenance: Dedup ===".to_string());
    lines.push(format!("ストア: {store_path}"));

    let mode = if dry_run {
        "DRY RUN (変更なし)"
    } else if result.applied {
        "APPLY (適用済み)"
    } else {
        "SCAN"
    };
    lines.push(format!("モード: {mode}"));
    lines.push(String::new());

    if result.duplicates.is_empty() {
        lines.push("重複なし。ストアはクリーンです。".to_string());
        return lines.join("\n");
    }

    lines.push(format!("検出された重複: {} 件", result.duplicates.len()));

    for (i, dup) in result.duplicates.iter().enumerate() {
        let reason = match &dup.duplicate_reason {
            memory_persistence::DuplicateReason::ExactSerializedHash => {
                "exact_duplicate (全フィールド一致)"
            }
            memory_persistence::DuplicateReason::ExactSpectrumHash => {
                "exact_spectrum (タグ完全一致)"
            }
        };
        lines.push(format!(
            "  [{:02}] 保持: {}  削除: {}  理由: {}",
            i + 1,
            dup.original_id,
            dup.duplicate_id,
            reason
        ));
    }

    lines.push(String::new());

    if result.applied {
        lines.push(format!("削除件数: {}", result.removed_count));
        if let Some(snap) = &result.snapshot_path {
            lines.push(format!("ロールバックスナップショット: {snap}"));
        }
    } else {
        lines.push("（dry-run のため変更は行われていません）".to_string());
        lines.push("適用するには: memory maintenance dedup --apply".to_string());
    }

    if !result.audit_entries.is_empty() {
        lines.push(String::new());
        lines.push("--- 監査ログ ---".to_string());
        for entry in &result.audit_entries {
            let audit_json = serde_json::to_string(entry).unwrap_or_default();
            lines.push(format!("  {audit_json}"));
        }
    }

    lines.join("\n")
}

fn parse_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

// ── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    #[test]
    fn maintenance_no_subcmd_shows_help() {
        let mut session = AgentSession::new();
        let out = handle_maintenance(&[], &mut session).unwrap();
        assert!(out.message.contains("dedup"));
        assert!(out.message.contains("dry-run"));
    }

    #[test]
    fn maintenance_unknown_subcmd_returns_error() {
        let mut session = AgentSession::new();
        let err = handle_maintenance(&["unknown".to_string()], &mut session).unwrap_err();
        assert!(matches!(err, CommandError::UnknownSubcommand { .. }));
    }

    #[test]
    fn dedup_no_flags_shows_usage() {
        let mut session = AgentSession::new();
        let out = handle_maintenance(&["dedup".to_string()], &mut session).unwrap();
        assert!(out.message.contains("--dry-run"));
        assert!(out.message.contains("--apply"));
        assert!(out.message.contains("--audit"));
    }

    #[test]
    fn dedup_dry_run_on_empty_store() {
        let tmp = tempfile_path("maintenance_test_empty.json");
        let mut session = AgentSession::new();
        let args = vec![
            "dedup".to_string(),
            "--dry-run".to_string(),
            "--store".to_string(),
            tmp.display().to_string(),
        ];
        let out = handle_maintenance(&args, &mut session).unwrap();
        assert!(
            out.message.contains("空")
                || out.message.contains("empty")
                || out.message.contains("なし")
        );
    }

    fn tempfile_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{}_{}.json",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
