use architecture_domain::ArchitectureState;
use knowledge_engine::{
    EntityId, KnowledgeConfidence, KnowledgeProvenance, KnowledgeRelation, KnowledgeSource,
    RelationType,
};
use knowledge_lifecycle::{ConflictContext, KnowledgeConflict, KnowledgeConflictResolver};
use language_core::SemanticGraph;

#[test]
fn context_aware_conflict_resolution_prefers_contextual_match() {
    let conflicts = vec![KnowledgeConflict {
        lhs: KnowledgeRelation {
            source: EntityId(1),
            target: EntityId(2),
            relation_type: RelationType::Supports,
            confidence: KnowledgeConfidence::new(0.7, 0.9),
            provenance: KnowledgeProvenance {
                source: KnowledgeSource::LocalDocument,
                timestamp: 1,
                usage_count: 1,
                last_used: 1,
            },
        },
        rhs: KnowledgeRelation {
            source: EntityId(1),
            target: EntityId(2),
            relation_type: RelationType::Requires,
            confidence: KnowledgeConfidence::new(0.7, 0.9),
            provenance: KnowledgeProvenance {
                source: KnowledgeSource::WebSearch,
                timestamp: 1,
                usage_count: 1,
                last_used: 1,
            },
        },
    }];

    let resolved = KnowledgeConflictResolver.resolve(
        conflicts,
        &ConflictContext {
            semantic_graph: SemanticGraph::default(),
            architecture_context: ArchitectureState::default(),
        },
    );

    assert_eq!(resolved.resolved_count, 1);
}
