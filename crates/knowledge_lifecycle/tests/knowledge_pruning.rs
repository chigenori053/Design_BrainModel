use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph, KnowledgeProvenance,
    KnowledgeRelation, KnowledgeSource, RelationType,
};
use knowledge_lifecycle::KnowledgePruningEngine;

#[test]
fn pruning_engine_removes_low_confidence_and_stale_relations() {
    let mut graph = KnowledgeGraph {
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
                confidence: KnowledgeConfidence::new(0.1, 0.9),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::LocalDocument,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
            KnowledgeRelation {
                source: EntityId(2),
                target: EntityId(1),
                relation_type: RelationType::Requires,
                confidence: KnowledgeConfidence::new(0.9, 0.75),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::Inferred,
                    timestamp: 6,
                    usage_count: 1,
                    last_used: 6,
                },
            },
        ],
    };

    let engine = KnowledgePruningEngine {
        confidence_threshold: 0.2,
        unused_cycles: 5,
        current_cycle: 7,
    };

    let pruned = engine.prune(&mut graph);

    assert_eq!(pruned, 1);
    assert_eq!(graph.relations.len(), 1);
}
