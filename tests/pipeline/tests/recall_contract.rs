use memory_engine::{InMemoryEngine, MemoryEngine, MemoryRecord, RecallConfig, RecallInput};
use pipeline_tests::{extract_fn_body, read_workspace_file};
use world_model::stable_v03::IntentState;

#[test]
fn recall_returns_only_normalized_memory_candidates_in_stable_order() {
    let engine = InMemoryEngine::default();
    engine.store(MemoryRecord {
        id: "b".to_string(),
        text: "api service cache".to_string(),
        tags: vec!["api".to_string(), "cache".to_string()],
        embedding: None,
        architecture: None,
        relations: Vec::new(),
    });
    engine.store(MemoryRecord {
        id: "a".to_string(),
        text: "api service db".to_string(),
        tags: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        embedding: None,
        architecture: None,
        relations: Vec::new(),
    });

    let input = RecallInput {
        intent: IntentState {
            raw: "api service db".to_string(),
            tokens: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        },
        limit: 5,
    };
    let config = RecallConfig {
        top_k: 2,
        threshold: 0.1,
    };

    let first = engine.recall_candidates(input.clone(), config.clone());
    let second = engine.recall_candidates(input, config);

    assert_eq!(first, second);
    assert!(first.iter().all(|candidate| candidate.is_valid()));
    assert!(first.windows(2).all(|pair| {
        pair[0].score > pair[1].score
            || (pair[0].score == pair[1].score && pair[0].id <= pair[1].id)
    }));
}

#[test]
fn recall_applies_threshold_and_top_k() {
    let engine = InMemoryEngine::default();
    for id in ["one", "two", "three"] {
        engine.store(MemoryRecord {
            id: id.to_string(),
            text: format!("{id} api service"),
            tags: vec!["api".to_string(), id.to_string()],
            embedding: None,
            architecture: None,
            relations: Vec::new(),
        });
    }

    let candidates = engine.recall_candidates(
        RecallInput {
            intent: IntentState {
                raw: "api".to_string(),
                tokens: vec!["api".to_string()],
            },
            limit: 5,
        },
        RecallConfig {
            top_k: 2,
            threshold: 0.5,
        },
    );

    assert!(candidates.len() <= 2);
    assert!(candidates.iter().all(|candidate| candidate.score >= 0.5));
}

#[test]
fn recall_layer_does_not_generate_hypotheses() {
    let source = read_workspace_file("crates/memory_engine/src/lib.rs");
    let body = extract_fn_body(&source, "pub fn recall_candidates(");

    assert!(!body.contains("Hypothesis"));
}
