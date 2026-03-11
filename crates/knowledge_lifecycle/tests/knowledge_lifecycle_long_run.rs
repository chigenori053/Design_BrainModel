use knowledge_engine::{
    default_reliability_for_source, EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph,
    KnowledgeProvenance, KnowledgeRelation, KnowledgeSource, RelationType, ValidationScore,
};
use knowledge_lifecycle::{KnowledgeLifecycleConfig, KnowledgeLifecycleEngine};

#[test]
fn long_run_lifecycle_stays_within_revision_thresholds() {
    let mut relations = Vec::new();
    for source in 1..=5 {
        for target in 1..=5 {
            if source == target {
                continue;
            }
            let idx = relations.len() % 4;
            relations.push(relation(
                EntityId(source),
                EntityId(target),
                match idx {
                    0 => RelationType::Supports,
                    1 => RelationType::Requires,
                    2 => RelationType::Constrains,
                    _ => RelationType::Recommends,
                },
                match idx {
                    0 => KnowledgeSource::LocalDocument,
                    1 => KnowledgeSource::ExperienceDerived,
                    2 => KnowledgeSource::Inferred,
                    _ => KnowledgeSource::WebSearch,
                },
            ));
        }
    }
    let mut graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity { id: EntityId(1), label: "rest".into() },
            KnowledgeEntity { id: EntityId(2), label: "service".into() },
            KnowledgeEntity { id: EntityId(3), label: "cache".into() },
            KnowledgeEntity { id: EntityId(4), label: "gateway".into() },
            KnowledgeEntity { id: EntityId(5), label: "worker".into() },
        ],
        relations,
    };
    let baseline = graph.relations.len();
    let mut final_state = None;

    for cycle in 1..=1000 {
        graph.relations.push(relation(
            EntityId(1),
            EntityId(2),
            RelationType::Supports,
            KnowledgeSource::LocalDocument,
        ));
        graph.relations.push(relation(
            EntityId(2),
            EntityId(3),
            RelationType::Requires,
            KnowledgeSource::ExperienceDerived,
        ));
        graph.relations.push(relation(
            EntityId(3),
            EntityId(4),
            RelationType::Constrains,
            KnowledgeSource::Inferred,
        ));
        graph.relations.push(relation(
            EntityId(4),
            EntityId(5),
            RelationType::Recommends,
            KnowledgeSource::WebSearch,
        ));
        let engine = KnowledgeLifecycleEngine::new(
            KnowledgeLifecycleConfig {
                current_cycle: cycle,
                decay_rate: 0.001,
                prune_confidence_threshold: 0.05,
                ..KnowledgeLifecycleConfig::default()
            },
            ValidationScore {
                consistency: 0.8,
                source_reliability: 0.8,
                confidence: 0.8,
            },
            4,
            true,
        );
        final_state = Some(engine.process(&mut graph));
    }

    let state = final_state.expect("final state");
    assert!(state.lifecycle_metrics.entropy > 0.6);
    assert!(state.lifecycle_metrics.average_confidence <= 0.9);
    assert!(state.turnover_metrics.turnover_rate < 0.3);
    assert!(graph.relations.len() <= ((baseline as f64) * 1.2).ceil() as usize + 1);
}

fn relation(
    source: EntityId,
    target: EntityId,
    relation_type: RelationType,
    source_kind: KnowledgeSource,
) -> KnowledgeRelation {
    let reliability = default_reliability_for_source(&source_kind);
    KnowledgeRelation {
        source,
        target,
        relation_type,
        confidence: KnowledgeConfidence::new(1.0, reliability),
        provenance: KnowledgeProvenance {
            source: source_kind,
            timestamp: 1,
            usage_count: 1,
            last_used: 1,
        },
    }
}
