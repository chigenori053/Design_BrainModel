use std::collections::HashMap;

use crate::concept::{Concept, ConceptId};

#[derive(Clone, Debug, Default)]
pub struct ConceptRegistry {
    concepts: HashMap<ConceptId, Concept>,
}

impl ConceptRegistry {
    pub fn get(&self, id: ConceptId) -> Option<&Concept> {
        self.concepts.get(&id)
    }

    pub fn register(&mut self, concept: Concept) {
        self.concepts.insert(concept.id, concept);
    }

    pub fn values(&self) -> impl Iterator<Item = &Concept> {
        self.concepts.values()
    }

    pub fn len(&self) -> usize {
        self.concepts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.concepts.is_empty()
    }

    pub fn find_similar(&self, embedding: &[f32]) -> Option<ConceptId> {
        const SIMILARITY_THRESHOLD: f32 = 0.92;

        self.concepts
            .values()
            .filter_map(|concept| {
                let score = cosine_similarity(&concept.embedding, embedding)?;
                Some((concept.id, score))
            })
            .filter(|(_, score)| *score >= SIMILARITY_THRESHOLD)
            .max_by(|lhs, rhs| lhs.1.total_cmp(&rhs.1))
            .map(|(id, _)| id)
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
    use crate::concept::{Concept, ConceptCategory, ConceptId};

    use super::ConceptRegistry;

    #[test]
    fn registry_insert_and_lookup() {
        let mut registry = ConceptRegistry::default();
        let id = ConceptId::from_name("database");
        registry.register(Concept {
            id,
            name: "DATABASE".to_string(),
            embedding: vec![1.0, 0.0],
            category: ConceptCategory::Component,
        });
        assert!(registry.get(id).is_some());
    }

    #[test]
    fn similarity_search_returns_top_match() {
        let mut registry = ConceptRegistry::default();
        let id_a = ConceptId::from_name("database");
        let id_b = ConceptId::from_name("cache");

        registry.register(Concept {
            id: id_a,
            name: "DATABASE".to_string(),
            embedding: vec![1.0, 0.0],
            category: ConceptCategory::Component,
        });
        registry.register(Concept {
            id: id_b,
            name: "CACHE".to_string(),
            embedding: vec![0.0, 1.0],
            category: ConceptCategory::Component,
        });

        let out = registry.find_similar(&[0.99, 0.01]);
        assert_eq!(out, Some(id_a));
    }
}
