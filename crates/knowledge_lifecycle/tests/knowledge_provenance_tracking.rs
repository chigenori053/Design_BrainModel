use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeProvenance, KnowledgeRelation, KnowledgeSource,
    RelationType,
};
use knowledge_lifecycle::KnowledgeProvenanceTracker;

#[test]
fn provenance_tracker_updates_usage_history() {
    let tracker = KnowledgeProvenanceTracker { current_cycle: 8 };
    let mut relation = KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Supports,
        confidence: KnowledgeConfidence::new(0.8, 0.9),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::LocalDocument,
            timestamp: 3,
            usage_count: 1,
            last_used: 3,
        },
    };

    tracker.record(&mut relation);

    assert_eq!(relation.provenance.usage_count, 2);
    assert_eq!(relation.provenance.last_used, 8);
    assert_eq!(relation.provenance.timestamp, 3);
}
