use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeProvenance, KnowledgeRelation, KnowledgeSource,
    RelationType,
};
use knowledge_lifecycle::KnowledgeAgingEngine;

#[test]
fn aging_engine_decays_confidence_for_old_knowledge() {
    let engine = KnowledgeAgingEngine {
        decay_rate: 0.1,
        current_cycle: 10,
    };
    let mut relation = KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Requires,
        confidence: KnowledgeConfidence::new(1.0, 0.6),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::WebSearch,
            timestamp: 0,
            usage_count: 1,
            last_used: 0,
        },
    };

    engine.decay_confidence(&mut relation);

    assert!(relation.confidence.effective_confidence < 1.0);
    assert!(relation.confidence.effective_confidence > 0.0);
}
