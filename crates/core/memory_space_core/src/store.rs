use crate::{MemoryStore, RecallQuery};

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRecord {
    pub memory_id: u64,
    pub feature_vector: Vec<f64>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryMemoryStore {
    records: Vec<MemoryRecord>,
}

impl InMemoryMemoryStore {
    pub fn with_records(records: Vec<MemoryRecord>) -> Self {
        Self { records }
    }
}

impl MemoryStore for InMemoryMemoryStore {
    fn insert(&mut self, memory: MemoryRecord) {
        self.records.push(memory);
    }

    fn query(&self, _query: &RecallQuery, k: usize) -> Vec<MemoryRecord> {
        self.records.iter().take(k).cloned().collect()
    }
}
