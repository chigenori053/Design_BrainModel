pub mod builder;
pub mod field;
pub mod similarity;

pub use builder::{ConceptVector, build_field, build_field_from_vectors, concept_vector_from_id};
pub use field::{ConceptField, FieldConfig};
pub use similarity::similarity;

#[cfg(test)]
mod tests {
    use concept_engine::ConceptId;

    use crate::{build_field_from_vectors, concept_vector_from_id};

    #[test]
    fn field_build_is_stable() {
        let concepts = vec![
            concept_vector_from_id(ConceptId::from_name("DATABASE"), 8),
            concept_vector_from_id(ConceptId::from_name("CACHE"), 8),
        ];
        let field = build_field_from_vectors(&concepts);
        assert_eq!(field.vector.data.len(), 8);
    }
}
