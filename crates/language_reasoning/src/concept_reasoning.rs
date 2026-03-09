use language_core::{ConceptId, SemanticGraph};

use crate::semantic_inference::{apply_inference, inferred_concepts_for};

pub fn expand_concepts(graph: &SemanticGraph) -> SemanticGraph {
    let mut expanded = graph.clone();
    let concept_ids = expanded
        .concepts
        .keys()
        .copied()
        .collect::<Vec<ConceptId>>();
    for concept_id in concept_ids {
        for concept in inferred_concepts_for(concept_id) {
            expanded.add_concept(concept);
        }
        for relation in apply_inference(&mut expanded, concept_id) {
            expanded.add_relation(relation);
        }
    }
    expanded
}
