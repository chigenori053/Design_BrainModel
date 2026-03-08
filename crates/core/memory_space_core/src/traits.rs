use crate::{MemoryRecord, RecallQuery};

pub trait MemoryStore {
    fn insert(&mut self, memory: MemoryRecord);

    fn query(&self, query: &RecallQuery, k: usize) -> Vec<MemoryRecord>;
}
