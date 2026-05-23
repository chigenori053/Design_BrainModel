use std::io;
use std::path::Path;

use crate::holographic_store::{HolographicVectorStore, MemoryEntry};

pub trait LegacyMemoryStore {
    fn append(&self, entry: &MemoryEntry) -> io::Result<()>;
    fn entries(&self) -> io::Result<Vec<MemoryEntry>>;
    fn entry_count(&self) -> io::Result<u64>;
}

#[derive(Debug)]
pub struct HolographicVectorStoreAdapter {
    inner: HolographicVectorStore,
}

impl HolographicVectorStoreAdapter {
    pub fn open(path: impl AsRef<Path>, dimension: u32) -> io::Result<Self> {
        Ok(Self {
            inner: HolographicVectorStore::open(path, dimension)?,
        })
    }

    pub fn inner(&self) -> &HolographicVectorStore {
        &self.inner
    }
}

impl LegacyMemoryStore for HolographicVectorStoreAdapter {
    fn append(&self, entry: &MemoryEntry) -> io::Result<()> {
        self.inner.append(entry)
    }

    fn entries(&self) -> io::Result<Vec<MemoryEntry>> {
        self.inner.entries()
    }

    fn entry_count(&self) -> io::Result<u64> {
        self.inner.entry_count()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{HolographicVectorStoreAdapter, LegacyMemoryStore};
    use crate::holographic_store::{HolographicVectorStore, MemoryEntry};

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{name}_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn adapter_opens_and_preserves_entries() {
        let path = temp_path("adapter_preserves_entries");
        {
            let adapter = HolographicVectorStoreAdapter::open(&path, 4).expect("open adapter");
            adapter
                .append(&MemoryEntry {
                    id: 7,
                    depth: 3,
                    timestamp: 11,
                    vector: vec![0.1, 0.2, 0.3, 0.4],
                })
                .expect("append");
        }

        let reopened = HolographicVectorStoreAdapter::open(&path, 4).expect("reopen adapter");
        let entries = reopened.entries().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 7);
        assert_eq!(entries[0].depth, 3);
        assert_eq!(entries[0].timestamp, 11);
        assert_eq!(entries[0].vector, vec![0.1, 0.2, 0.3, 0.4]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn adapter_append_roundtrip_matches_legacy_store_behavior() {
        let legacy_path = temp_path("adapter_legacy_roundtrip_legacy");
        let adapter_path = temp_path("adapter_legacy_roundtrip_adapter");
        let entry = MemoryEntry {
            id: 1,
            depth: 2,
            timestamp: 3,
            vector: vec![0.5, 0.6, 0.7, 0.8],
        };

        let legacy = HolographicVectorStore::open(&legacy_path, 4).expect("open legacy");
        legacy.append(&entry).expect("append legacy");

        let adapter = HolographicVectorStoreAdapter::open(&adapter_path, 4).expect("open adapter");
        adapter.append(&entry).expect("append adapter");

        assert_eq!(
            adapter.entries().expect("adapter entries"),
            legacy.entries().expect("legacy entries")
        );
        assert_eq!(
            adapter.entry_count().expect("adapter count"),
            legacy.entry_count().expect("legacy count")
        );

        let _ = std::fs::remove_file(legacy_path);
        let _ = std::fs::remove_file(adapter_path);
    }
}
