use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use design_search_engine::stable_v03::{DesignSearchEngine, DeterministicBeamSearchEngine};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use world_model::stable_v03::IntentInput;

fn seeded_memory() -> Arc<dyn MemoryEngine> {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("repository", NodeType::Component))
        .build()
        .expect("valid graph");
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "seed-pattern".to_string(),
        text: "api repository".to_string(),
        tags: vec!["api".to_string(), "repository".to_string()],
        embedding: Some(vec![1.0, 2.0]),
        architecture: Some(architecture),
        relations: vec!["selected".to_string()],
    });
    memory
}

#[test]
fn runtime_execute_produces_architecture_without_panic() {
    let runtime = CoreRuntime::new(
        seeded_memory(),
        Arc::new(DeterministicBeamSearchEngine::default()) as Arc<dyn DesignSearchEngine>,
    );
    let result = runtime
        .executor
        .execute(IntentInput::new("api service repository"))
        .expect("runtime should succeed");

    assert!(!result.architecture.nodes().is_empty());
    assert!(result.trace.candidate_count > 0);
}
