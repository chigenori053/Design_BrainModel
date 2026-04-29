//! DBM IR Sync Layer  (DBM-IR-SYNC-SPEC v1.0)
//!
//! Ensures that the in-memory IR / `TransactionIR` never diverges from the
//! on-disk file state.  The source of truth is always the file system; the IR
//! is treated as derived / cached state.
//!
//! Phases implemented here:
//!   Phase 1 – IR Snapshotting  (`hash_content`, `hash_file`)
//!   Phase 2 – Drift Detection  (`is_drift`)
//!   Phase 3 – Strict-mode error/violation message constants
//!   Phase 7 – Telemetry        (`IrSyncTelemetry`)
//!
//! Phases 4 (REPL integration), 5 (diff invalidation), and 6 (apply guard)
//! are implemented in `nl/executor.rs` where they interact with
//! `ConversationState` and `TransactionIR`.

use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;

// ── Phase 1: stable file-content hash (FNV-1a) ───────────────────────────────

/// Returns the FNV-1a 64-bit hash of `content`.
///
/// FNV-1a is stable, deterministic, and requires no external dependencies.
/// It is suitable for change-detection (drift detection); it is **not**
/// intended for security use.
///
/// Algorithm: for each byte, `hash = (hash XOR byte) * FNV_PRIME`.
pub fn hash_content(content: &str) -> u64 {
    const PRIME: u64 = 1_099_511_628_211;
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    content
        .bytes()
        .fold(OFFSET, |h, b| (h ^ b as u64).wrapping_mul(PRIME))
}

/// Reads `path` from disk and returns its FNV-1a hash.
///
/// Returns `Err` when the file cannot be read.
pub fn hash_file(path: &Path) -> Result<u64, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("ir_sync: cannot read {}: {e}", path.display()))?;
    Ok(hash_content(&content))
}

// ── Phase 2: drift detection ──────────────────────────────────────────────────

/// Returns `Ok(true)` when the on-disk content of `path` has changed since
/// `recorded_hash` was captured, meaning the IR snapshot is stale (drifted).
///
/// Returns `Err` only when the file cannot be read.
pub fn is_drift(path: &Path, recorded_hash: u64) -> Result<bool, String> {
    Ok(hash_file(path)? != recorded_hash)
}

// ── Phase 3: canonical error / violation messages ─────────────────────────────

/// Strict-mode error text returned to the user when drift is detected before
/// an apply.  Instructs the user to run `analyze` to refresh the IR.
pub const DRIFT_ERROR: &str = "ERROR: IR out of sync with file system. Run 'analyze' to refresh.";

/// Violation message used in panics / assertions when an apply guard fires
/// internally (i.e., the drift check is bypassed and a stale diff reaches the
/// write path).
pub const APPLY_VIOLATION: &str = "Violation: applying diff on stale IR";

// ── Phase 7: telemetry ────────────────────────────────────────────────────────

/// Snapshot of IR sync state emitted at reload / drift-check points.
///
/// Hashes are formatted as lowercase hex strings so that they can be safely
/// embedded in JSON without precision loss.
#[derive(Debug, Clone, Serialize)]
pub struct IrSyncTelemetry {
    /// Hash recorded in the `TransactionIR` when the diff was generated.
    /// `None` when no transaction is active (e.g., after `reload`).
    pub ir_hash: Option<String>,
    /// Current on-disk file hash.
    pub file_hash: String,
    /// `true` when `ir_hash != file_hash` (file has changed since snapshot).
    pub drift: bool,
    /// Unix timestamp (seconds) of this telemetry snapshot.
    pub last_sync_unix: Option<u64>,
    /// Number of sync operations performed in this session.
    pub sync_count: u64,
}

impl IrSyncTelemetry {
    /// Build a telemetry snapshot from the stored IR hash and the current
    /// on-disk file hash.
    pub fn build(ir_hash: Option<u64>, file_hash: u64, sync_count: u64) -> Self {
        let last_sync_unix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .ok();
        Self {
            ir_hash: ir_hash.map(|h| format!("{h:016x}")),
            file_hash: format!("{file_hash:016x}"),
            drift: ir_hash.is_some_and(|h| h != file_hash),
            last_sync_unix,
            sync_count,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_content_is_deterministic() {
        let s = "use std::collections::HashSet;\n\nfn main() {}";
        assert_eq!(hash_content(s), hash_content(s));
    }

    #[test]
    fn hash_content_differs_for_different_inputs() {
        assert_ne!(hash_content("foo"), hash_content("bar"));
    }

    #[test]
    fn hash_content_empty_string_is_offset_basis() {
        // FNV-1a of an empty input is the offset basis unchanged.
        const OFFSET: u64 = 14_695_981_039_346_656_037;
        assert_eq!(hash_content(""), OFFSET);
    }

    #[test]
    fn is_drift_false_when_hash_matches() {
        let path = std::env::temp_dir().join("ir_sync_no_drift.rs");
        std::fs::write(&path, "fn main() {}").unwrap();
        let hash = hash_file(&path).unwrap();
        assert!(!is_drift(&path, hash).unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn is_drift_true_after_file_change() {
        let path = std::env::temp_dir().join("ir_sync_drift.rs");
        std::fs::write(&path, "fn main() {}").unwrap();
        let hash = hash_file(&path).unwrap();
        std::fs::write(&path, "fn main() { /* changed */ }").unwrap();
        assert!(is_drift(&path, hash).unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn telemetry_drift_false_when_hashes_match() {
        let t = IrSyncTelemetry::build(Some(42), 42, 0);
        assert!(!t.drift);
        assert_eq!(t.ir_hash.as_deref(), Some("000000000000002a"));
        assert_eq!(t.file_hash, "000000000000002a");
    }

    #[test]
    fn telemetry_drift_true_when_hashes_differ() {
        let t = IrSyncTelemetry::build(Some(1), 2, 3);
        assert!(t.drift);
        assert_eq!(t.sync_count, 3);
    }

    #[test]
    fn telemetry_no_ir_hash_is_not_drift() {
        // When no transaction is active (ir_hash = None), drift is reported
        // as false so that a reload always succeeds.
        let t = IrSyncTelemetry::build(None, 99, 0);
        assert!(!t.drift);
        assert!(t.ir_hash.is_none());
    }
}
