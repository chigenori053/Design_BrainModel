use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use design_search_engine::stable_v03::{DesignSearchEngine, DeterministicBeamSearchEngine};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use world_model::stable_v03::IntentInput;

fn memory_with_pattern() -> Arc<dyn MemoryEngine> {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("cache", NodeType::Component))
        .add_node(Node::new("db", NodeType::DataStore))
        .build()
        .expect("valid graph");
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "pattern-with-cache".to_string(),
        text: "api cache db".to_string(),
        tags: vec!["api".to_string(), "cache".to_string(), "db".to_string()],
        embedding: Some(vec![1.0, 2.0, 3.0]),
        architecture: Some(architecture),
        relations: vec!["selected".to_string()],
    });
    memory
}

#[test]
fn memory_changes_search_result() {
    let input = IntentInput::new("api cache db");
    let without_memory = CoreRuntime::new(
        Arc::new(InMemoryEngine::default()) as Arc<dyn MemoryEngine>,
        Arc::new(DeterministicBeamSearchEngine {
            beam_width: 4,
            max_depth: 2,
        }) as Arc<dyn DesignSearchEngine>,
    )
    .executor
    .execute(input.clone())
    .expect("runtime should succeed");
    let with_memory = CoreRuntime::new(
        memory_with_pattern(),
        Arc::new(DeterministicBeamSearchEngine {
            beam_width: 4,
            max_depth: 2,
        }) as Arc<dyn DesignSearchEngine>,
    )
    .executor
    .execute(input)
    .expect("runtime should succeed");

    assert_ne!(without_memory, with_memory);
}
