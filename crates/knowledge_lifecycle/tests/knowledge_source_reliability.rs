use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeGraph, KnowledgeProvenance, KnowledgeRelation,
    KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgeSourceReliabilityEvaluator;

#[test]
fn source_reliability_evaluator_collects_reliability_by_source() {
    let graph = KnowledgeGraph {
        entities: vec![],
        relations: vec![
            KnowledgeRelation {
                source: EntityId(1),
                target: EntityId(2),
                relation_type: RelationType::Supports,
                confidence: KnowledgeConfidence::new(0.7, 0.9),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::LocalDocument,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
            KnowledgeRelation {
                source: EntityId(2),
                target: EntityId(3),
                relation_type: RelationType::Requires,
                confidence: KnowledgeConfidence::new(0.5, 0.6),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::WebSearch,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
        ],
    };

    let reliabilities = KnowledgeSourceReliabilityEvaluator.evaluate(&graph);

    assert_eq!(reliabilities.len(), 2);
    assert!(reliabilities.iter().any(|entry| entry.reliability_score == 0.9));
    assert!(reliabilities.iter().any(|entry| entry.reliability_score == 0.6));
}
