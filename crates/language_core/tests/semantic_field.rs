use language_core::{ConceptId, SemanticField};

#[test]
fn semantic_field_tracks_activation_deterministically() {
    let mut field = SemanticField::default();
    field.activate(ConceptId(1), 0.333);
    field.activate(ConceptId(2), 0.876);

    assert_eq!(field.activation_of(ConceptId(1)), 0.33);
    assert_eq!(field.activation_of(ConceptId(2)), 0.88);
}
