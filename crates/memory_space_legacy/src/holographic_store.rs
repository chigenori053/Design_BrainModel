use std::io;
use std::path::Path;

use crate::memory_entry::MemoryEntry;
use crate::store_adapter::{HolographicVectorStoreAdapter, LegacyMemoryStore};

/// Compatibility layer. Prefer [`HolographicVectorStoreAdapter`] and
/// [`crate::store_adapter::LegacyMemoryStore`] for new code.
#[derive(Debug)]
pub struct HolographicVectorStore {
    inner: HolographicVectorStoreAdapter,
}

impl HolographicVectorStore {
    pub fn open(path: impl AsRef<Path>, dimension: u32) -> io::Result<Self> {
        Ok(Self {
            inner: HolographicVectorStoreAdapter::open(path, dimension)?,
        })
    }

    pub fn path(&self) -> &std::path::Path {
        self.inner.path()
    }

    pub fn dimension(&self) -> u32 {
        self.inner.dimension()
    }

    pub fn append(&self, entry: &MemoryEntry) -> io::Result<()> {
        self.inner.append(entry)
    }

    pub fn entries(&self) -> io::Result<Vec<MemoryEntry>> {
        self.inner.entries()
    }

    pub fn entry_count(&self) -> io::Result<u64> {
        self.inner.entry_count()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::memory_entry::MemoryEntry;
    use crate::store_adapter::{HolographicVectorStoreAdapter, LegacyMemoryStore};

    use super::HolographicVectorStore;

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
    fn store_appends_and_reads_entries() {
        let path = temp_path("holographic_store_test");
        let store = HolographicVectorStore::open(&path, 4).expect("open");
        store
            .append(&MemoryEntry {
                id: 1,
                depth: 2,
                timestamp: 3,
                vector: vec![0.1, 0.2, 0.3, 0.4],
            })
            .expect("append");
        let items = store.entries().expect("read");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[0].depth, 2);
        assert_eq!(items[0].vector.len(), 4);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn deprecated_holographic_store_delegates_to_adapter() {
        let path = temp_path("holographic_delegates_to_adapter");
        let entry = MemoryEntry {
            id: 10,
            depth: 3,
            timestamp: 50,
            vector: vec![0.1, 0.2, 0.3, 0.4],
        };

        let store = HolographicVectorStore::open(&path, 4).expect("open legacy");
        store.append(&entry).expect("append via legacy");

        let adapter = HolographicVectorStoreAdapter::open(&path, 4).expect("open adapter");
        let entries = adapter.entries().expect("read via adapter");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry);

        let _ = std::fs::remove_file(path);
    }
}
