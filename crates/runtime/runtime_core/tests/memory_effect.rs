use std::sync::Arc;

use architecture_evaluator_core::stable_v03::{
    ArchitectureEvaluator, WeightedArchitectureEvaluator,
};
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use constraint_engine::stable_v03::{
    CompositeConstraintEngine, Constraint, ConstraintEngine, LayerOrderConstraint,
    NoCycleConstraint,
};
use design_search_engine::stable_v03::{DesignSearchEngine, DeterministicBeamSearchEngine};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use code_language_core::stable_v03::{
    CodeGenerator, CodeIRBuilder, DefaultCodeIRBuilder, RustGenerator,
};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};
use world_model::stable_v03::IntentInput;

fn memory_with_pattern() -> Arc<dyn MemoryEngine> {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .build()
        .expect("valid graph");
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "pattern-with-service".to_string(),
        text: "api service db".to_string(),
        tags: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        embedding: Some(vec![1.0, 2.0, 3.0]),
        architecture: Some(architecture),
        relations: vec!["selected".to_string()],
    });
    memory
}

fn constraint_engine() -> Arc<dyn ConstraintEngine> {
    Arc::new(CompositeConstraintEngine::new(vec![
        Arc::new(NoCycleConstraint) as Arc<dyn Constraint>,
        Arc::new(LayerOrderConstraint) as Arc<dyn Constraint>,
    ]))
}

fn evaluator() -> Arc<dyn ArchitectureEvaluator> {
    Arc::new(WeightedArchitectureEvaluator::default())
}

fn mapper() -> Arc<dyn ArchitectureMapper> {
    Arc::new(DefaultArchitectureMapper)
}

fn code_ir_builder() -> Arc<dyn CodeIRBuilder> {
    Arc::new(DefaultCodeIRBuilder)
}

fn generator() -> Arc<dyn CodeGenerator> {
    Arc::new(RustGenerator)
}

#[test]
fn memory_changes_search_result() {
    let input = IntentInput::new("api service db");
    let without_memory = CoreRuntime::new(
        Arc::new(InMemoryEngine::default()) as Arc<dyn MemoryEngine>,
        Arc::new(DeterministicBeamSearchEngine {
            beam_width: 4,
            max_depth: 2,
        }) as Arc<dyn DesignSearchEngine>,
        constraint_engine(),
        evaluator(),
        mapper(),
        code_ir_builder(),
        generator(),
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
        constraint_engine(),
        evaluator(),
        mapper(),
        code_ir_builder(),
        generator(),
    )
    .executor
    .execute(input)
    .expect("runtime should succeed");

    assert_ne!(without_memory, with_memory);
}
