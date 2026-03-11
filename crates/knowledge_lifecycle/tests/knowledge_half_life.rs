use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeGraph, KnowledgeProvenance, KnowledgeRelation,
    KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgeHalfLifeMonitor;

#[test]
fn half_life_is_median_survival_cycles() {
    let graph = KnowledgeGraph {
        entities: vec![],
        relations: vec![
            relation(2),
            relation(4),
            relation(8),
        ],
    };

    let half_life = KnowledgeHalfLifeMonitor { current_cycle: 10 }.calculate(&graph);

    assert_eq!(half_life.half_life, 6);
    assert_eq!(half_life.survival_cycles, 8);
}

fn relation(timestamp: u64) -> KnowledgeRelation {
    KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Supports,
        confidence: KnowledgeConfidence::new(0.8, 0.9),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::LocalDocument,
            timestamp,
            usage_count: 1,
            last_used: timestamp,
        },
    }
}
