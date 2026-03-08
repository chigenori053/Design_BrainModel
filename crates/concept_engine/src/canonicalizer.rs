use crate::concept::{Concept, ConceptCategory, ConceptId, normalize_concept_name};
use crate::concept_registry::ConceptRegistry;

#[derive(Clone, Debug, Default)]
pub struct Canonicalizer {
    registry: ConceptRegistry,
}

impl Canonicalizer {
    pub fn new(registry: ConceptRegistry) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> &ConceptRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut ConceptRegistry {
        &mut self.registry
    }

    pub fn canonicalize(&mut self, text: &str, embedding: &[f32]) -> ConceptId {
        let normalized_name = normalize_concept_name(text);
        let candidate_id = ConceptId::from_name(&normalized_name);

        if self.registry.get(candidate_id).is_some() {
            return candidate_id;
        }

        if let Some(similar) = self.registry.find_similar(embedding) {
            return similar;
        }

        let concept = Concept {
            id: candidate_id,
            name: normalized_name,
            embedding: embedding.to_vec(),
            category: infer_category(text),
        };
        self.registry.register(concept);
        candidate_id
    }
}

fn infer_category(text: &str) -> ConceptCategory {
    let lower = text.to_ascii_lowercase();

    if lower.contains("optimiz") || lower.contains("improve") || lower.contains("reduce") {
        return ConceptCategory::Action;
    }
    if lower.contains("latency") || lower.contains("throughput") || lower.contains("cost") {
        return ConceptCategory::Property;
    }
    if lower.contains("must") || lower.contains("constraint") || lower.contains("禁止") {
        return ConceptCategory::Constraint;
    }
    if lower.contains("domain") || lower.contains("system") || lower.contains("platform") {
        return ConceptCategory::Domain;
    }

    ConceptCategory::Component
}

#[cfg(test)]
mod tests {
    use crate::concept_registry::ConceptRegistry;

    use super::Canonicalizer;

    #[test]
    fn canonicalization_merges_similar_texts() {
        let mut canonicalizer = Canonicalizer::new(ConceptRegistry::default());
        let id_a = canonicalizer.canonicalize("optimize query", &[1.0, 0.0, 0.0]);
        let id_b = canonicalizer.canonicalize("query optimization", &[0.99, 0.01, 0.0]);

        assert_eq!(id_a, id_b);
        assert_eq!(canonicalizer.registry().len(), 1);
    }
}
