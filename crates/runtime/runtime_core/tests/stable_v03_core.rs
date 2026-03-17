use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::stable_v03::{CoreError, RuntimeResult};
use runtime_core::CoreRuntime;
use world_model::stable_v03::IntentInput;

fn seeded_memory() -> Arc<dyn MemoryEngine> {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .build()
        .expect("valid graph");
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "seed-pattern".to_string(),
        text: "api db service".to_string(),
        tags: vec!["api".to_string(), "db".to_string()],
        embedding: Some(vec![1.0, 2.0]),
        architecture: Some(architecture),
        relations: vec!["depends_on".to_string()],
    });
    memory
}

fn search_engine() -> Arc<dyn design_search_engine::stable_v03::DesignSearchEngine> {
    Arc::new(DeterministicBeamSearchEngine::default())
}

#[test]
fn invalid_input_is_rejected() {
    let runtime = CoreRuntime::new(seeded_memory(), search_engine());
    let result = runtime.executor.execute(IntentInput::new("   "));

    assert_eq!(result, Err(CoreError::InvalidInput));
}

#[test]
fn runtime_trace_is_deterministic_for_same_input() {
    let input = IntentInput::new("api service db");
    let expected: RuntimeResult = CoreRuntime::new(seeded_memory(), search_engine())
        .executor
        .execute(input.clone())
        .expect("runtime should succeed");

    for _ in 0..100 {
        let actual = CoreRuntime::new(seeded_memory(), search_engine())
            .executor
            .execute(input.clone())
            .expect("runtime should succeed");
        assert_eq!(actual.trace, expected.trace);
    }
}
