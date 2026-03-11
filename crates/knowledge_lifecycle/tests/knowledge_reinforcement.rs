use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeProvenance, KnowledgeRelation, KnowledgeSource,
    RelationType,
};
use knowledge_lifecycle::{KnowledgeReinforcementEngine, ReinforcementConfig};

#[test]
fn reinforcement_engine_boosts_high_value_knowledge() {
    let engine = KnowledgeReinforcementEngine {
        config: ReinforcementConfig {
            reinforcement_rate: 0.02,
            max_confidence: 0.9,
        },
        evaluation_threshold: 0.75,
        frequent_usage_threshold: 3,
        evaluation_score: 0.9,
        architecture_usage_count: 1,
        consistent_inference: false,
    };
    let mut relation = KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Recommends,
        confidence: KnowledgeConfidence::new(0.5, 0.85),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::ExperienceDerived,
            timestamp: 2,
            usage_count: 2,
            last_used: 2,
        },
    };

    let reinforced = engine.reinforce(&mut relation);

    assert!(reinforced);
    assert!(relation.confidence.inference_confidence > 0.5);
}
