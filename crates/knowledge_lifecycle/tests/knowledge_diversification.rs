use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeGraph, KnowledgeProvenance, KnowledgeRelation,
    KnowledgeSource, RelationType, ValidationScore,
};
use knowledge_lifecycle::{KnowledgeLifecycleConfig, KnowledgeLifecycleEngine};

#[test]
fn low_entropy_triggers_diversification() {
    let mut graph = KnowledgeGraph {
        entities: vec![],
        relations: vec![
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
            KnowledgeRelation {
                source: EntityId(2),
                target: EntityId(3),
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

    let state = KnowledgeLifecycleEngine::new(
        KnowledgeLifecycleConfig::default(),
        ValidationScore {
            consistency: 0.8,
            source_reliability: 0.8,
            confidence: 0.8,
        },
        1,
        true,
    )
    .process(&mut graph);

    assert!(state.diversification_triggered);
    assert!(state.exploration_weight > 1.0);
    assert!(state.reinforcement_rate_applied < 0.02);
}
