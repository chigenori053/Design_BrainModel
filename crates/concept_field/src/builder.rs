use concept_engine::{Concept, ConceptId, ConceptRegistry};
use memory_space_complex::{ComplexField, normalize};
use memory_space_core::Complex64;

use crate::field::{ConceptField, FieldConfig};

#[derive(Clone, Debug)]
pub struct ConceptVector {
    pub concept: ConceptId,
    pub vector: ComplexField,
    pub weight: f32,
}

pub fn build_field(concepts: &[Concept], registry: &ConceptRegistry) -> ConceptField {
    let vectors = concepts
        .iter()
        .map(|concept| {
            let source = registry.get(concept.id).unwrap_or(concept);
            let vector =
                embedding_to_complex(&source.embedding, FieldConfig::default().reasoning_dim);
            ConceptVector {
                concept: source.id,
                vector,
                weight: 1.0,
            }
        })
        .collect::<Vec<_>>();

    build_field_from_vectors(&vectors)
}

pub fn build_field_from_vectors(concepts: &[ConceptVector]) -> ConceptField {
    if concepts.is_empty() {
        return ConceptField {
            vector: ComplexField::new(Vec::new()),
            config: FieldConfig::default(),
        };
    }

    let max_dim = concepts
        .iter()
        .map(|concept| concept.vector.data.len())
        .max()
        .unwrap_or(0);

    let mut merged = vec![Complex64::new(0.0, 0.0); max_dim];
    for concept in concepts {
        let weight = concept.weight.max(0.0);
        for (idx, value) in concept.vector.data.iter().enumerate() {
            merged[idx] += *value * weight;
        }
    }

    let mut field = ComplexField::new(merged);
    normalize(&mut field);
    ConceptField {
        vector: field,
        config: FieldConfig::default(),
    }
}

pub fn concept_vector_from_id(concept: ConceptId, dim: usize) -> ConceptVector {
    let mut data = Vec::with_capacity(dim.max(1));
    let base = concept.0.max(1);
    for i in 0..dim.max(1) {
        let x = ((base >> (i % 16)) & 0xFF) as f32 / 255.0;
        let y = ((base >> ((i + 8) % 16)) & 0xFF) as f32 / 255.0;
        data.push(Complex64::new(x, y));
    }
    let mut vector = ComplexField::new(data);
    normalize(&mut vector);
    ConceptVector {
        concept,
        vector,
        weight: 1.0,
    }
}

fn embedding_to_complex(embedding: &[f32], dim: usize) -> ComplexField {
    let mut out = Vec::with_capacity(dim.max(1));
    for i in 0..dim.max(1) {
        let re = embedding.get(i).copied().unwrap_or(0.0);
        let im = embedding
            .get((i + 1) % embedding.len().max(1))
            .copied()
            .unwrap_or(0.0);
        out.push(Complex64::new(re, im));
    }
    let mut vector = ComplexField::new(out);
    normalize(&mut vector);
    vector
}
