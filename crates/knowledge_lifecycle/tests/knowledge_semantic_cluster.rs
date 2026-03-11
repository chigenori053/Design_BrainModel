use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph, KnowledgeProvenance,
    KnowledgeRelation, KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgeSemanticCluster;

#[test]
fn semantic_cluster_groups_semantically_duplicate_relations() {
    let graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity { id: EntityId(1), label: "api".into() },
            KnowledgeEntity { id: EntityId(2), label: "gateway".into() },
        ],
        relations: vec![
            relation(0.5, 0.6),
            relation(0.9, 0.9),
        ],
    };

    let clusters = KnowledgeSemanticCluster::default().cluster(&graph);

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].relations.len(), 2);
}

fn relation(inference: f64, reliability: f64) -> KnowledgeRelation {
    KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Supports,
        confidence: KnowledgeConfidence::new(inference, reliability),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::LocalDocument,
            timestamp: 1,
            usage_count: 1,
            last_used: 1,
        },
    }
}
