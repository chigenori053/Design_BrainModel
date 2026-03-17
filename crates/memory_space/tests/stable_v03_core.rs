use std::sync::Arc;
use std::thread;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use memory_space_phase14::stable_v03::{
    InMemoryEngine, MemoryEngine, MemoryQuery, MemoryRecord, RecallInput,
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
