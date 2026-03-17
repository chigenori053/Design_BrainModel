use std::sync::Arc;

use architecture_evaluator_core::stable_v03::{ArchitectureEvaluator, WeightedArchitectureEvaluator};
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use constraint_engine::stable_v03::{
    CompositeConstraintEngine, Constraint, ConstraintEngine, LayerOrderConstraint,
    NoCycleConstraint,
};
use design_search_engine::stable_v03::{ArchitectureCandidate, DesignSearchEngine, SearchInput};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use code_language_core::stable_v03::{
    CodeGenerator, CodeIRBuilder, DefaultCodeIRBuilder, RustGenerator, TargetLanguage,
};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};
use world_model::stable_v03::IntentInput;

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

fn framework_memory() -> Arc<dyn MemoryEngine> {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "fastapi-hint".to_string(),
        text: "api python fastapi".to_string(),
        tags: vec!["lang:python".to_string(), "framework:fastapi".to_string()],
        embedding: None,
        architecture: None,
        relations: vec!["framework:fastapi".to_string()],
    });
    memory
}

struct FixedSearchEngine;

impl DesignSearchEngine for FixedSearchEngine {
    fn search(&self, _input: SearchInput) -> Vec<ArchitectureCandidate> {
        let architecture = ArchitectureGraphBuilder::new()
            .add_node(Node::new("api", NodeType::Interface))
            .add_node(Node::new("service", NodeType::Service))
            .add_edge(Edge::new("api", "service", RelationType::Calls))
            .build()
            .expect("valid graph");
        vec![ArchitectureCandidate {
            id: "fixed".to_string(),
            architecture,
            score: 1.0,
            depth: 0,
        }]
    }
}

#[test]
fn memory_changes_generation_context() {
    let input = IntentInput::new("api service db");
    let without_memory = CoreRuntime::new(
        Arc::new(InMemoryEngine::default()) as Arc<dyn MemoryEngine>,
        Arc::new(FixedSearchEngine) as Arc<dyn DesignSearchEngine>,
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
        framework_memory(),
        Arc::new(FixedSearchEngine) as Arc<dyn DesignSearchEngine>,
        constraint_engine(),
        evaluator(),
        mapper(),
        code_ir_builder(),
        generator(),
    )
    .executor
    .execute(input)
    .expect("runtime should succeed");

    assert!(!without_memory.generation_contexts.is_empty());
    assert!(!with_memory.generation_contexts.is_empty());
    assert_ne!(without_memory.generation_contexts, with_memory.generation_contexts);
    assert_eq!(
        with_memory.generation_contexts[0].language_profile.language,
        TargetLanguage::Python
    );
    assert!(with_memory.files.iter().any(|file| file.path.ends_with(".py")));
}
