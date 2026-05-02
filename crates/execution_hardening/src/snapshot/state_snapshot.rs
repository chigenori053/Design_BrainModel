use crate::checksum::Checksum;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Opaque identifier for a `StateSnapshot`.
///
/// Format: `snap-{timestamp_ms}-{monotonic_counter}`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotId(String);

impl SnapshotId {
    fn generate() -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let seq = SNAPSHOT_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self(format!("snap-{ts}-{seq}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Raw byte payload of a serialized state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedState(Vec<u8>);

impl SerializedState {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Serialize `value` as JSON bytes.
    pub fn from_json<T: serde::Serialize>(value: &T) -> Result<Self, String> {
        serde_json::to_vec(value)
            .map(Self)
            .map_err(|e| e.to_string())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// An immutable point-in-time snapshot of execution state.
///
/// Spec §7 State Snapshot:
/// - `id`       — unique identifier
/// - `checksum` — blake3 hash of `data` bytes
/// - `data`     — serialized state payload
///
/// Pre-run and post-run snapshots are compared to detect state drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    pub id: SnapshotId,
    pub checksum: Checksum,
    pub data: SerializedState,
    pub timestamp_ms: u64,
}

impl StateSnapshot {
    /// Create a new snapshot from a serialized state payload.
    ///
    /// The `checksum` is computed immediately so that integrity can be
    /// verified at any later point without re-serializing.
    pub fn new(data: SerializedState) -> Self {
        let checksum = Checksum::of(data.as_bytes());
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            id: SnapshotId::generate(),
            checksum,
            data,
            timestamp_ms,
        }
    }

    /// Create a snapshot from an arbitrary JSON-serializable value.
    pub fn from_json<T: serde::Serialize>(value: &T) -> Result<Self, String> {
        SerializedState::from_json(value).map(Self::new)
    }

    /// Verify that `checksum` still matches the stored `data`.
    ///
    /// Returns `false` when the data has been tampered with
    /// (spec §7.3, §10: StateCorruptionError).
    pub fn verify(&self) -> bool {
        Checksum::of(self.data.as_bytes()) == self.checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_succeeds_on_intact_snapshot() {
        let snap = StateSnapshot::new(SerializedState::from_bytes(b"state-data".to_vec()));
        assert!(snap.verify());
    }

    #[test]
    fn verify_fails_after_tampering() {
        let mut snap = StateSnapshot::new(SerializedState::from_bytes(b"original".to_vec()));
        snap.data = SerializedState::from_bytes(b"tampered".to_vec());
        assert!(!snap.verify(), "tampered snapshot must fail verification");
    }

    #[test]
    fn ids_are_unique() {
        let a = StateSnapshot::new(SerializedState::from_bytes(b"x".to_vec()));
        let b = StateSnapshot::new(SerializedState::from_bytes(b"x".to_vec()));
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn from_json_roundtrip() {
        #[derive(serde::Serialize)]
        struct Foo {
            x: u32,
        }
        let snap = StateSnapshot::from_json(&Foo { x: 42 }).unwrap();
        assert!(snap.verify());
    }
}
