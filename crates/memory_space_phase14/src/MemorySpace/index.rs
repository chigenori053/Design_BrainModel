use std::collections::BTreeMap;

use super::types::MemoryId;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryIndex {
    embedding_index: BTreeMap<MemoryId, Vec<f32>>,
    graph_index: BTreeMap<MemoryId, Vec<MemoryId>>,
    hash_index: BTreeMap<String, MemoryId>,
}

impl MemoryIndex {
    pub fn index_embedding(&mut self, node_id: MemoryId, embedding: Vec<f32>) {
        self.embedding_index.insert(node_id, embedding);
    }

    pub fn index_graph_neighbors(&mut self, node_id: MemoryId, neighbors: Vec<MemoryId>) {
        self.graph_index.insert(node_id, neighbors);
    }

    pub fn index_hash(&mut self, hash: String, node_id: MemoryId) {
        self.hash_index.insert(hash, node_id);
    }

    pub fn embedding(&self, node_id: MemoryId) -> Option<&[f32]> {
        self.embedding_index.get(&node_id).map(Vec::as_slice)
    }

    pub fn neighbors(&self, node_id: MemoryId) -> &[MemoryId] {
        self.graph_index
            .get(&node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn resolve_hash(&self, hash: &str) -> Option<MemoryId> {
        self.hash_index.get(hash).copied()
    }
}
