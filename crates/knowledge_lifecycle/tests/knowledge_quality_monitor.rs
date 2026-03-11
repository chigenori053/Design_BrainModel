use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph, KnowledgeProvenance,
    KnowledgeRelation, KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgeQualityMonitor;

#[test]
fn quality_monitor_reports_graph_health() {
    let graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity {
                id: EntityId(1),
                label: "a".to_string(),
            },
            KnowledgeEntity {
                id: EntityId(2),
                label: "b".to_string(),
            },
        ],
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
                source: EntityId(1),
                target: EntityId(2),
                relation_type: RelationType::Constrains,
                confidence: KnowledgeConfidence::new(0.4, 0.6),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::WebSearch,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
        ],
    };

    let metrics = KnowledgeQualityMonitor.analyze(&graph);

    assert_eq!(metrics.node_count, 2);
    assert_eq!(metrics.edge_count, 2);
    assert!(metrics.conflict_rate > 0.0);
    let expected = (((0.8_f64 * 0.9_f64).sqrt()) + ((0.4_f64 * 0.6_f64).sqrt())) / 2.0;
    assert!((metrics.average_confidence - expected).abs() < 1e-9);
}
