use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeProvenance, KnowledgeRelation, KnowledgeSource,
    RelationType,
};
use knowledge_lifecycle::{KnowledgeReinforcementEngine, ReinforcementConfig};

#[test]
fn reinforcement_cap_limits_confidence_growth() {
    let mut relation = KnowledgeRelation {
        source: EntityId(1),
        target: EntityId(2),
        relation_type: RelationType::Supports,
        confidence: KnowledgeConfidence::new(0.94, 0.9),
        provenance: KnowledgeProvenance {
            source: KnowledgeSource::LocalDocument,
            timestamp: 1,
            usage_count: 1,
            last_used: 1,
        },
    };
    let engine = KnowledgeReinforcementEngine {
        config: ReinforcementConfig {
            reinforcement_rate: 0.5,
            max_confidence: 0.9,
        },
        evaluation_threshold: 0.75,
        frequent_usage_threshold: 3,
        evaluation_score: 0.9,
        architecture_usage_count: 1,
        consistent_inference: false,
    };

    engine.reinforce(&mut relation);

    assert!(relation.confidence.inference_confidence <= 0.9);
}
