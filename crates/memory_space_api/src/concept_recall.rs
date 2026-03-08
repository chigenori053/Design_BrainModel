use concept_engine::ConceptId;

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEntry {
    pub concept: ConceptId,
    pub vector: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConceptRecallHit {
    pub concept: ConceptId,
    pub score: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ConceptMemorySpace {
    entries: Vec<MemoryEntry>,
}

impl ConceptMemorySpace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, entry: MemoryEntry) {
        self.entries.push(entry);
    }

    pub fn recall_concepts(&self, query: &[f32], top_k: usize) -> Vec<ConceptRecallHit> {
        if top_k == 0 {
            return Vec::new();
        }

        let mut scored = self
            .entries
            .iter()
            .filter_map(|entry| {
                let score = cosine_similarity(query, &entry.vector)?;
                Some(ConceptRecallHit {
                    concept: entry.concept,
                    score,
                })
            })
            .collect::<Vec<_>>();

        scored.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.concept.0.cmp(&rhs.concept.0))
        });
        scored.dedup_by(|lhs, rhs| lhs.concept == rhs.concept);
        scored.truncate(top_k);
        scored
    }

    pub fn recall_vectors(&self, concept: ConceptId) -> Vec<&[f32]> {
        self.entries
            .iter()
            .filter(|entry| entry.concept == concept)
            .map(|entry| entry.vector.as_slice())
            .collect()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || b.is_empty() {
        return None;
    }

    let len = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;

    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }

    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return None;
    }

    Some((dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use concept_engine::ConceptId;

    use super::{ConceptMemorySpace, MemoryEntry};

    #[test]
    fn concept_recall_returns_top_k() {
        let mut memory = ConceptMemorySpace::new();
        let database = ConceptId::from_name("database");
        let cache = ConceptId::from_name("cache");

        memory.insert(MemoryEntry {
            concept: database,
            vector: vec![1.0, 0.0],
        });
        memory.insert(MemoryEntry {
            concept: cache,
            vector: vec![0.0, 1.0],
        });

        let out = memory.recall_concepts(&[0.99, 0.01], 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].concept, database);
    }

    #[test]
    fn vector_recall_filters_by_concept() {
        let mut memory = ConceptMemorySpace::new();
        let database = ConceptId::from_name("database");
        let cache = ConceptId::from_name("cache");

        memory.insert(MemoryEntry {
            concept: database,
            vector: vec![1.0, 0.0],
        });
        memory.insert(MemoryEntry {
            concept: database,
            vector: vec![0.8, 0.2],
        });
        memory.insert(MemoryEntry {
            concept: cache,
            vector: vec![0.0, 1.0],
        });

        let recalled = memory.recall_vectors(database);
        assert_eq!(recalled.len(), 2);
    }
}
