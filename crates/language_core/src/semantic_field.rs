use std::collections::HashMap;

use crate::concept_memory::ConceptId;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticField {
    pub activation: HashMap<ConceptId, f64>,
}

impl SemanticField {
    pub fn activate(&mut self, concept_id: ConceptId, value: f64) {
        let quantized = ((value * 100.0).round() / 100.0).clamp(0.0, 1.0);
        self.activation.insert(concept_id, quantized);
    }

    pub fn activation_of(&self, concept_id: ConceptId) -> f64 {
        self.activation.get(&concept_id).copied().unwrap_or(0.0)
    }
}
