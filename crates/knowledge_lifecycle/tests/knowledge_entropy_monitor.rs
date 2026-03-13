use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeGraph, KnowledgeProvenance, KnowledgeRelation,
    KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgeEntropyMonitor;

#[test]
fn entropy_monitor_detects_diversity_collapse() {
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
                relation_type: RelationType::Supports,
                confidence: KnowledgeConfidence::new(0.7, 0.9),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::LocalDocument,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
        ],
    };
    let monitor = KnowledgeEntropyMonitor {
        entropy_threshold: 0.6,
    };
    let entropy = monitor.calculate(&graph);

    assert!(monitor.is_collapse(&entropy));
}
