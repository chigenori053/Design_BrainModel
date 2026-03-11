use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeGraph, KnowledgeProvenance, KnowledgeRelation,
    KnowledgeSource, RelationType, ValidationScore,
};
use knowledge_lifecycle::{KnowledgeLifecycleConfig, KnowledgeLifecycleEngine};

#[test]
fn lifecycle_metrics_are_updated_after_processing() {
    let mut graph = KnowledgeGraph {
        entities: vec![],
        relations: vec![KnowledgeRelation {
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
        }],
    };

    let state = KnowledgeLifecycleEngine::new(
        KnowledgeLifecycleConfig {
            current_cycle: 2,
            decay_rate: 0.001,
            ..KnowledgeLifecycleConfig::default()
        },
        ValidationScore {
            consistency: 0.8,
            source_reliability: 0.8,
            confidence: 0.8,
        },
        1,
        true,
    )
    .process(&mut graph);

    assert!(state.lifecycle_metrics.average_confidence > 0.0);
    assert!(state.lifecycle_metrics.half_life > 0);
}
