use language_core::ConceptId;
use language_reasoning::infer_semantic_relations;

#[test]
fn semantic_inference_expands_rest_api_concepts() {
    let relations = infer_semantic_relations(ConceptId(2));

    assert!(relations.len() >= 2);
    assert!(
        relations
            .iter()
            .any(|relation| relation.target == ConceptId(11))
    );
    assert!(
        relations
            .iter()
            .any(|relation| relation.target == ConceptId(12))
    );
}
