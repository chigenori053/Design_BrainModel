use concept_engine::ConceptId;
use memory_space_api::{ConceptMemorySpace, MemoryEntry};
use reasoning_agent::hypothesis::generate_bound_concept_pairs;
use semantic_dhm::SemanticEngine;

fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; 8];
    for (i, b) in text.bytes().enumerate() {
        v[i % 8] += f32::from(b) / 255.0;
    }
    v
}

#[test]
fn concept_pipeline_end_to_end() {
    let text = "optimize database query performance";

    let mut semantic_engine = SemanticEngine::new();
    let semantic_unit = semantic_engine.text_to_semantic_unit(text, &embed(text));

    assert_ne!(semantic_unit.concept.0, 0);

    let mut memory = ConceptMemorySpace::new();
    memory.insert(MemoryEntry {
        concept: semantic_unit.concept,
        vector: semantic_unit.context_vector.clone(),
    });
    memory.insert(MemoryEntry {
        concept: ConceptId::from_name("CACHE"),
        vector: vec![0.0; 8],
    });

    let recalled = memory.recall_concepts(&semantic_unit.context_vector, 2);
    assert!(!recalled.is_empty());

    let hypotheses = generate_bound_concept_pairs(
        &[semantic_unit.concept, ConceptId::from_name("DATABASE")],
        4,
    );
    assert!(!hypotheses.is_empty());
}

#[test]
fn concept_pipeline_deterministic() {
    let text = "optimize database query";
    let emb = embed(text);

    let mut semantic_engine = SemanticEngine::new();
    let unit1 = semantic_engine.text_to_semantic_unit(text, &emb);
    let unit2 = semantic_engine.text_to_semantic_unit(text, &emb);

    assert_eq!(unit1.concept, unit2.concept);
}
