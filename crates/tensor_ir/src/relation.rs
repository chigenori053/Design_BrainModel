use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::predicate::PredicateId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    Memory,
    Inferred,
    LLMGenerated,
    Symbolic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub subject: EntityId,
    pub predicate: PredicateId,
    pub object: EntityId,
    pub weight: f32,
    pub provenance: Provenance,
}

impl Relation {
    pub fn new(
        subject: EntityId,
        predicate: PredicateId,
        object: EntityId,
        weight: f32,
        provenance: Provenance,
    ) -> Self {
        Self {
            subject,
            predicate,
            object,
            weight: weight.clamp(0.0, 1.0),
            provenance,
        }
    }

    pub fn score(&self, rule_confidence: f32, context_score: f32) -> f32 {
        self.weight * rule_confidence * context_score
    }
}
