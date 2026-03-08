use memory_space_complex::ComplexField;
use memory_space_core::{MemoryField, MemoryId};
use memory_space_recall::recall_top_k;

pub trait MemoryIndex {
    fn insert(&mut self, memory: MemoryField);

    fn search(&self, query: &ComplexField, k: usize) -> Vec<MemoryId>;
}

#[derive(Clone, Debug, Default)]
pub struct LinearIndex {
    memory_bank: Vec<MemoryField>,
}

impl LinearIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_memory(memory_bank: Vec<MemoryField>) -> Self {
        Self { memory_bank }
    }
}

impl MemoryIndex for LinearIndex {
    fn insert(&mut self, memory: MemoryField) {
        self.memory_bank.push(memory);
    }

    fn search(&self, query: &ComplexField, k: usize) -> Vec<MemoryId> {
        recall_top_k(query, &self.memory_bank, k)
            .into_iter()
            .map(|candidate| candidate.memory_id)
            .collect()
    }
}
