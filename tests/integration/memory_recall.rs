use concept_engine::ConceptId;
use memory_space_api::{ConceptMemorySpace, MemoryEntry};

#[test]
fn concept_based_memory_recall() {
    let query_optimization = ConceptId::from_name("QUERY_OPTIMIZATION");
    let database = ConceptId::from_name("DATABASE");

    let mut memory = ConceptMemorySpace::new();
    memory.insert(MemoryEntry {
        concept: query_optimization,
        vector: vec![1.0, 0.0, 0.0],
    });
    memory.insert(MemoryEntry {
        concept: query_optimization,
        vector: vec![0.95, 0.05, 0.0],
    });
    memory.insert(MemoryEntry {
        concept: database,
        vector: vec![0.0, 1.0, 0.0],
    });

    let concepts = memory.recall_concepts(&[0.99, 0.01, 0.0], 2);
    assert!(!concepts.is_empty());
    assert_eq!(concepts[0].concept, query_optimization);

    let vectors = memory.recall_vectors(query_optimization);
    assert_eq!(vectors.len(), 2);
}
