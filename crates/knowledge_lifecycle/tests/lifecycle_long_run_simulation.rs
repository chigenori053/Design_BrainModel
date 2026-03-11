use knowledge_engine::{
    default_reliability_for_source, EntityId, KnowledgeConfidence, KnowledgeEntity, KnowledgeGraph,
    KnowledgeProvenance, KnowledgeRelation, KnowledgeSource, RelationType, ValidationScore,
};
use knowledge_lifecycle::{KnowledgeLifecycleConfig, KnowledgeLifecycleEngine};

#[test]
fn lifecycle_remains_stable_over_1000_cycles() {
    let mut graph = KnowledgeGraph {
        entities: vec![
            KnowledgeEntity { id: EntityId(1), label: "rest".into() },
            KnowledgeEntity { id: EntityId(2), label: "stateless".into() },
            KnowledgeEntity { id: EntityId(3), label: "cache".into() },
            KnowledgeEntity { id: EntityId(4), label: "service".into() },
        ],
        relations: vec![
            build_relation(EntityId(1), EntityId(2), RelationType::Constrains, KnowledgeSource::LocalDocument),
            build_relation(EntityId(3), EntityId(4), RelationType::Recommends, KnowledgeSource::ExperienceDerived),
            build_relation(EntityId(4), EntityId(1), RelationType::Requires, KnowledgeSource::Inferred),
            build_relation(EntityId(1), EntityId(2), RelationType::Supports, KnowledgeSource::WebSearch),
        ],
    };
    let initial_size = graph.relations.len();
    let mut final_state = None;

    for cycle in 1..=1000 {
        let mut cycle_graph = graph.clone();
        for (relation_type, knowledge_source) in [
            (RelationType::Supports, KnowledgeSource::LocalDocument),
            (RelationType::Requires, KnowledgeSource::ExperienceDerived),
            (RelationType::Constrains, KnowledgeSource::Inferred),
            (RelationType::Recommends, KnowledgeSource::WebSearch),
        ] {
            cycle_graph.relations.push(build_relation(
                EntityId(1),
                EntityId(2),
                relation_type,
                knowledge_source,
            ));
        }
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
        final_state = Some(engine.process(&mut cycle_graph));
        graph = cycle_graph;
    }

    let state = final_state.expect("final state");
    assert!(state.lifecycle_metrics.entropy > 0.6);
    assert!(state.lifecycle_metrics.average_confidence <= 0.9 + 1e-9);
    assert!(state.turnover_metrics.turnover_rate < 0.3);
    assert!(graph.relations.len() <= ((initial_size as f64) * 1.2).ceil() as usize + 1);
}

fn build_relation(
    source: EntityId,
    target: EntityId,
    relation_type: RelationType,
    knowledge_source: KnowledgeSource,
) -> KnowledgeRelation {
    let reliability = default_reliability_for_source(&knowledge_source);
    KnowledgeRelation {
        source,
        target,
        relation_type,
        confidence: KnowledgeConfidence::new(1.0, reliability),
        provenance: KnowledgeProvenance {
            source: knowledge_source,
            timestamp: 1,
            usage_count: 1,
            last_used: 1,
        },
    }
}
