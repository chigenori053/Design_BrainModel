use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph, KnowledgeProvenance,
    KnowledgeRelation, KnowledgeSource, RelationType,
};
use knowledge_lifecycle::SemanticClusterEngine;

#[test]
fn semantic_cluster_uses_similarity_threshold() {
    let graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity {
                id: EntityId(1),
                label: "a".into(),
            },
            KnowledgeEntity {
                id: EntityId(2),
                label: "b".into(),
            },
        ],
        relations: vec![
            relation(EntityId(1), EntityId(2), 0.8),
            relation(EntityId(1), EntityId(2), 0.79),
        ],
    };

    let clusters = SemanticClusterEngine {
        similarity_threshold: 0.85,
    }
    .cluster(&graph);

    assert_eq!(clusters.len(), 1);
}

fn relation(source: EntityId, target: EntityId, inference: f64) -> KnowledgeRelation {
    KnowledgeRelation {
        source,
        target,
        relation_type: RelationType::Supports,
        confidence: KnowledgeConfidence::new(inference, 0.9),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::LocalDocument,
            timestamp: 1,
            usage_count: 1,
            last_used: 1,
        },
    }
}
