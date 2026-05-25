use std::sync::Arc;

use architecture_evaluator_core::stable_v03::{
    ArchitectureEvaluator, WeightedArchitectureEvaluator,
};
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use code_language_core::stable_v03::{
    CodeGenerator, CodeIRBuilder, DefaultCodeIRBuilder, RustGenerator,
};
use constraint_engine::stable_v03::{
    CompositeConstraintEngine, Constraint, ConstraintEngine, LayerOrderConstraint,
    NoCycleConstraint,
};
use design_search_engine::stable_v03::{ArchitectureCandidate, DesignSearchEngine, ReasoningInput};
use memory_engine::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};
use world_model::stable_v03::IntentInput;

fn seeded_memory() -> Arc<dyn MemoryEngine> {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "phase4-python".to_string(),
        text: "api service python fastapi".to_string(),
        tags: vec!["lang:python".to_string(), "framework:fastapi".to_string()],
        embedding: None,
        architecture: None,
        relations: vec!["framework:fastapi".to_string()],
    });
    memory
}

struct FixedSearchEngine;

impl DesignSearchEngine for FixedSearchEngine {
    fn search(&self, _input: ReasoningInput) -> Vec<ArchitectureCandidate> {
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
fn runtime_full_pipeline_is_stable() {
    let runtime = CoreRuntime::new(
        seeded_memory(),
        Arc::new(FixedSearchEngine) as Arc<dyn DesignSearchEngine>,
        constraint_engine(),
        evaluator(),
        mapper(),
        code_ir_builder(),
        generator(),
    );

    let result = runtime
        .executor
        .execute(IntentInput::new("api service db"))
        .expect("runtime should succeed");

    assert!(!result.files.is_empty());
    assert!(result.project_layout.is_valid());
    assert!(!result.execution_plan.run_plan.run_commands.is_empty());
    assert!(!result.execution_plan.test_plan.test_commands.is_empty());
    assert!(!result.generation_contexts.is_empty());
}
