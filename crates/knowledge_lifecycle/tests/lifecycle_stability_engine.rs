use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeGraph, KnowledgeProvenance, KnowledgeRelation,
    KnowledgeSource, RelationType,
};
use knowledge_lifecycle::{KnowledgeEntropy, KnowledgeTurnoverMetrics, LifecycleStabilityEngine};

#[test]
fn lifecycle_stability_engine_reports_expected_metrics() {
    let graph = KnowledgeGraph {
        entities: vec![],
        relations: vec![KnowledgeRelation {
            source: EntityId(1),
            target: EntityId(2),
            relation_type: RelationType::Constrains,
            confidence: KnowledgeConfidence::new(0.8, 0.75),
            provenance: KnowledgeProvenance {
                source: KnowledgeSource::Inferred,
                timestamp: 1,
                usage_count: 1,
                last_used: 1,
            },
        }],
    };

    let metrics = LifecycleStabilityEngine.analyze(
        &graph,
        &KnowledgeEntropy { entropy_score: 0.7 },
        1,
        1,
        &KnowledgeTurnoverMetrics {
            added_relations: 0,
            removed_relations: 0,
            turnover_rate: 0.2,
        },
        12,
    );

    assert!((metrics.entropy - 0.7).abs() < 1e-9);
    assert!((metrics.average_confidence - (0.8_f64 * 0.75_f64).sqrt()).abs() < 1e-9);
    assert!((metrics.pruning_rate - 1.0).abs() < 1e-9);
    assert!((metrics.reinforcement_rate - 1.0).abs() < 1e-9);
    assert!((metrics.turnover_rate - 0.2).abs() < 1e-9);
    assert_eq!(metrics.half_life, 12);
}
