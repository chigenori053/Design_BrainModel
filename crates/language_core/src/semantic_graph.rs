use std::collections::BTreeMap;

use crate::concept_memory::{Concept, ConceptId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum RelationType {
    Uses,
    Requires,
    Constrains,
    Pattern,
    Clarifies,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticRelation {
    pub source: ConceptId,
    pub relation: RelationType,
    pub target: ConceptId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticGraph {
    pub concepts: BTreeMap<ConceptId, Concept>,
    pub relations: Vec<SemanticRelation>,
}

impl SemanticGraph {
    pub fn add_concept(&mut self, concept: Concept) {
        self.concepts.insert(concept.concept_id, concept);
    }

    pub fn add_relation(&mut self, relation: SemanticRelation) {
        if !self.relations.contains(&relation) {
            self.relations.push(relation);
            self.relations
                .sort_by_key(|edge| (edge.source, edge.target, edge.relation));
        }
    }
}
