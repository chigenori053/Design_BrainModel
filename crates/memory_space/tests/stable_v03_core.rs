use std::sync::Arc;
use std::thread;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use memory_space_phase14::stable_v03::{
    InMemoryEngine, MemoryEngine, MemoryQuery, MemoryRecord, MemoryRelation, RecallConfig,
    RecallInput,
};
use world_model::stable_v03::IntentState;

#[test]
fn store_recall_retrieve_is_consistent() {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .build()
        .expect("valid graph");
    let engine = InMemoryEngine::default();
    engine.store(MemoryRecord {
        id: "pattern-api".to_string(),
        text: "api service template".to_string(),
        tags: vec!["api".to_string(), "service".to_string()],
        embedding: Some(vec![1.0, 0.0]),
        architecture: Some(architecture),
        relations: vec!["depends_on".to_string()],
    });

    let recalled = engine.recall(RecallInput {
        intent: IntentState {
            raw: "api service".to_string(),
            tokens: vec!["api".to_string(), "service".to_string()],
        },
        limit: 3,
    });
    let retrieved = engine.retrieve(MemoryQuery {
        text: "api service".to_string(),
        tags: vec!["service".to_string()],
        limit: 3,
    });

    assert_eq!(recalled.records.len(), 1);
    assert_eq!(recalled.records[0].record.id, "pattern-api");
    assert_eq!(retrieved[0].id, "pattern-api");
}

#[test]
fn memory_engine_is_thread_safe_for_parallel_recall() {
    let engine = Arc::new(InMemoryEngine::default());
    engine.store(MemoryRecord {
        id: "seed".to_string(),
        text: "api service".to_string(),
        tags: vec!["api".to_string(), "service".to_string()],
        embedding: None,
        architecture: None,
        relations: Vec::new(),
    });

    let handles = (0..8)
        .map(|_| {
            let engine = Arc::clone(&engine);
            thread::spawn(move || {
                engine.recall(RecallInput {
                    intent: IntentState {
                        raw: "api service".to_string(),
                        tokens: vec!["api".to_string(), "service".to_string()],
                    },
                    limit: 2,
                })
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let recalled = handle.join().expect("thread should not panic");
        assert_eq!(recalled.records.len(), 1);
    }
}

#[test]
fn recall_candidates_and_graph_snapshot_support_phase6_memory_shape() {
    let engine = InMemoryEngine::default();
    engine.store(MemoryRecord {
        id: "seed".to_string(),
        text: "api service db".to_string(),
        tags: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        embedding: Some(vec![1.0, 0.5, 0.25]),
        architecture: None,
        relations: vec!["selected".to_string()],
    });
    engine.store_edge("seed", "db-template", MemoryRelation::Similarity);

    let candidates = engine.recall_candidates(
        RecallInput {
            intent: IntentState {
                raw: "api db".to_string(),
                tokens: vec!["api".to_string(), "db".to_string()],
            },
            limit: 5,
        },
        RecallConfig {
            top_k: 3,
            threshold: 0.1,
        },
    );
    let snapshot = engine.graph_snapshot();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, "seed");
    assert!(snapshot.nodes.iter().any(|node| node.id == "seed"));
    assert_eq!(snapshot.edges.len(), 1);
}

#[test]
fn recall_cache_records_hits_and_misses_deterministically() {
    let engine = InMemoryEngine::default();
    engine.store(MemoryRecord {
        id: "seed".to_string(),
        text: "api service".to_string(),
        tags: vec!["api".to_string(), "service".to_string()],
        embedding: None,
        architecture: None,
        relations: Vec::new(),
    });

    let input = RecallInput {
        intent: IntentState {
            raw: "api service".to_string(),
            tokens: vec!["api".to_string(), "service".to_string()],
        },
        limit: 2,
    };

    let first = engine.recall(input.clone());
    let second = engine.recall(input);
    let stats = engine.cache_stats();

    assert_eq!(first, second);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 1);
}
