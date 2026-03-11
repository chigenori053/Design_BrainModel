use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeProvenance, KnowledgeRelation, KnowledgeSource,
    RelationType,
};
use knowledge_lifecycle::generate_embedding;

#[test]
fn embedding_generation_produces_dense_vector() {
    let embedding = generate_embedding(&KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Requires,
        confidence: KnowledgeConfidence::new(0.8, 0.9),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::LocalDocument,
            timestamp: 1,
            usage_count: 1,
            last_used: 1,
        },
    });

    assert_eq!(embedding.vector.len(), 4);
}
