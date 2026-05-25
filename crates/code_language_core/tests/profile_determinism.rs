use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use code_language_core::stable_v03::{DefaultProfileResolver, ProfileResolver};
use memory_engine::{InMemoryEngine, MemoryEngine, MemoryRecord};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};

fn sample_unit() -> unified_design_ir::ImplementationUnit {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    DefaultArchitectureMapper
        .map(&architecture)
        .to_implementation_units()
        .into_iter()
        .next()
        .expect("unit")
}

#[test]
fn profile_resolution_is_deterministic() {
    let unit = sample_unit();
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "python-fastapi".to_string(),
        text: "api service python fastapi".to_string(),
        tags: vec!["lang:python".to_string(), "framework:fastapi".to_string()],
        embedding: None,
        architecture: None,
        relations: vec!["framework:fastapi".to_string()],
    });

    let mut results = Vec::new();
    for _ in 0..1000 {
        results.push(DefaultProfileResolver.resolve(&unit, memory.as_ref()));
    }

    for result in &results[1..] {
        assert_eq!(&results[0], result);
    }
}
