use knowledge_engine::{
    KnowledgeDocument, KnowledgeEngine, KnowledgeMetadata, KnowledgeSource, LocalDocumentRetriever,
    UnifiedKnowledgeRank, build_knowledge_patterns, feature_from_content,
    knowledge_entries_to_memory_records, rank_unified_knowledge, snapshot_documents,
    verify_snapshot,
};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, RecallInput};
use world_model::stable_v03::IntentState;

#[test]
fn snapshot_replay_same_hash_same_feature() {
    let documents = vec![KnowledgeDocument {
        source: KnowledgeSource::WebSearch,
        content: "REST API should remain stateless. API gateway requires service discovery."
            .to_string(),
        metadata: KnowledgeMetadata {
            title: "REST API web result".to_string(),
            source_uri: "web://knowledge/rest-api".to_string(),
            reliability_hint: 0.72,
        },
    }];

    let first = snapshot_documents(&documents, 10);
    let second = snapshot_documents(&documents, 10);

    assert_eq!(first, second);
    assert!(verify_snapshot(&first[0]));
    assert_eq!(
        first[0].feature,
        feature_from_content(&documents[0].content)
    );
}

#[test]
fn tampered_snapshot_is_rejected() {
    let mut entry = snapshot_documents(
        &[KnowledgeDocument {
            source: KnowledgeSource::LocalDocument,
            content: "f(x)=x^2 maps to Function(Polynomial).".to_string(),
            metadata: KnowledgeMetadata::default(),
        }],
        1,
    )
    .remove(0);

    entry.raw_content = "f(x)=x^3 maps to Function(Polynomial).".to_string();

    assert!(!verify_snapshot(&entry));
}

#[test]
fn knowledge_snapshot_integrates_into_memory_retrieval() {
    let documents = vec![KnowledgeDocument {
        source: KnowledgeSource::WebSearch,
        content: "REST API should remain stateless. API gateway requires service discovery."
            .to_string(),
        metadata: KnowledgeMetadata::default(),
    }];
    let entries = snapshot_documents(&documents, 1);
    let records = knowledge_entries_to_memory_records(&entries);
    let memory = InMemoryEngine::default();
    for record in records {
        memory.store(record);
    }

    let recalled = memory.recall(RecallInput {
        intent: IntentState {
            raw: "api gateway service discovery".to_string(),
            tokens: vec![
                "api".to_string(),
                "gateway".to_string(),
                "service".to_string(),
                "discovery".to_string(),
            ],
        },
        limit: 3,
    });

    assert_eq!(recalled.records.len(), 1);
    assert!(recalled.records[0].record.id.starts_with("knowledge:web:"));
}

#[test]
fn knowledge_patterns_are_deterministic() {
    let documents = vec![
        KnowledgeDocument {
            source: KnowledgeSource::WebSearch,
            content: "REST API should remain stateless.".to_string(),
            metadata: KnowledgeMetadata::default(),
        },
        KnowledgeDocument {
            source: KnowledgeSource::WebSearch,
            content: "REST service should remain stateless.".to_string(),
            metadata: KnowledgeMetadata::default(),
        },
    ];
    let first = build_knowledge_patterns(&snapshot_documents(&documents, 3));
    let mut reversed = documents.clone();
    reversed.reverse();
    let second = build_knowledge_patterns(&snapshot_documents(&reversed, 3));

    assert_eq!(first, second);
    assert_eq!(first[0].source_type, "web");
    assert_eq!(first[0].support_count, 2);
}

#[test]
fn unified_ranking_uses_total_order() {
    let ranked = rank_unified_knowledge(vec![
        UnifiedKnowledgeRank {
            id: "knowledge-a".to_string(),
            source_priority: 3,
            score: 0.9,
            confidence: 0.8,
            timestamp: 2,
        },
        UnifiedKnowledgeRank {
            id: "memory-a".to_string(),
            source_priority: 1,
            score: 0.9,
            confidence: 0.8,
            timestamp: 2,
        },
        UnifiedKnowledgeRank {
            id: "pattern-a".to_string(),
            source_priority: 2,
            score: 0.9,
            confidence: 0.8,
            timestamp: 1,
        },
    ]);

    assert_eq!(ranked[0].id, "memory-a");
    assert_eq!(ranked[1].id, "pattern-a");
    assert_eq!(ranked[2].id, "knowledge-a");
}

#[test]
fn knowledge_engine_exposes_snapshot_apply_layer() {
    let engine = KnowledgeEngine::new(LocalDocumentRetriever);
    let entries = engine.snapshot_documents(
        &[KnowledgeDocument {
            source: KnowledgeSource::LocalDocument,
            content: "Triangle geometry normalizes to mesh IR.".to_string(),
            metadata: KnowledgeMetadata::default(),
        }],
        4,
    );

    assert_eq!(entries[0].source, "local");
    assert!(
        entries[0]
            .feature
            .structure
            .contains(&"geometry->ir".to_string())
    );
}
