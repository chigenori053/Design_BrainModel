use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph, KnowledgeProvenance,
    KnowledgeRelation, KnowledgeSource, RelationType,
};
use knowledge_lifecycle::{ConflictContext, KnowledgeConflictResolver};
use architecture_domain::ArchitectureState;
use language_core::SemanticGraph;

#[test]
fn conflict_resolution_keeps_relation_with_highest_effective_confidence() {
    let graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity { id: EntityId(1), label: "supports".into() },
            KnowledgeEntity { id: EntityId(2), label: "requires".into() },
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
                relation_type: RelationType::Requires,
                confidence: KnowledgeConfidence::new(0.7, 0.6),
                provenance: KnowledgeProvenance {
                    source: KnowledgeSource::WebSearch,
                    timestamp: 1,
                    usage_count: 1,
                    last_used: 1,
                },
            },
        ],
    };

    let conflicts = KnowledgeConflictResolver.detect(&graph);
    let resolution = KnowledgeConflictResolver.resolve(
        conflicts,
        &ConflictContext {
            semantic_graph: SemanticGraph::default(),
            architecture_context: ArchitectureState::default(),
        },
    );

    assert_eq!(resolution.resolved_count, 1);
    assert_eq!(resolution.resolved_relations[0].relation_type, RelationType::Supports);
}
