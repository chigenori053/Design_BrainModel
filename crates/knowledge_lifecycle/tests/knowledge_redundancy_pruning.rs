use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph, KnowledgeProvenance,
    KnowledgeRelation, KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgeSemanticCluster;

#[test]
fn redundancy_pruning_keeps_highest_confidence_relation() {
    let mut graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity { id: EntityId(1), label: "a".into() },
            KnowledgeEntity { id: EntityId(2), label: "b".into() },
        ],
        relations: vec![
            KnowledgeRelation {
                source: EntityId(1),
                target: EntityId(2),
                relation_type: RelationType::Supports,
                confidence: KnowledgeConfidence::new(0.4, 0.6),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::WebSearch,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
            KnowledgeRelation {
                source: EntityId(1),
                target: EntityId(2),
                relation_type: RelationType::Supports,
                confidence: KnowledgeConfidence::new(0.8, 0.9),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::LocalDocument,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
        ],
    };

    let detector = KnowledgeSemanticCluster::default();
    assert_eq!(detector.cluster(&graph).len(), 1);
    let pruned = detector.prune(&mut graph);

    assert_eq!(pruned, 1);
    assert_eq!(graph.relations.len(), 1);
    assert!((graph.relations[0].confidence.effective_confidence - (0.8_f64 * 0.9_f64).sqrt()).abs() < 1e-9);
}
