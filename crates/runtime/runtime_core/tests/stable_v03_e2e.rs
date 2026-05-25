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
use design_search_engine::stable_v03::{
    ArchitectureCandidate, DesignSearchEngine, DeterministicBeamSearchEngine, ReasoningInput,
};
use memory_engine::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};
use world_model::stable_v03::IntentInput;

fn seeded_memory() -> Arc<dyn MemoryEngine> {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "db", RelationType::DependsOn))
        .build()
        .expect("valid graph");
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "seed-pattern".to_string(),
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
fn runtime_execute_produces_architecture_without_panic() {
    let runtime = CoreRuntime::new(
        seeded_memory(),
        Arc::new(DeterministicBeamSearchEngine::default()) as Arc<dyn DesignSearchEngine>,
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

    assert!(!result.architecture.nodes().is_empty());
    assert!(!result.design.nodes().is_empty());
    assert!(!result.files.is_empty());
    assert!(!result.generation_contexts.is_empty());
    assert!(!result.project_layout.files.is_empty());
    assert!(!result.execution_plan.test_plan.test_commands.is_empty());
    assert!(result.trace.candidate_count > 0);
}

struct InvalidFirstSearchEngine;

impl DesignSearchEngine for InvalidFirstSearchEngine {
    fn search(&self, _input: ReasoningInput) -> Vec<ArchitectureCandidate> {
        let invalid = ArchitectureGraphBuilder::new()
            .add_node(Node::new("api", NodeType::Interface))
            .add_node(Node::new("service", NodeType::Service))
            .add_edge(Edge::new("service", "api", RelationType::Calls))
            .build()
            .expect("valid graph");
        let valid = ArchitectureGraphBuilder::new()
            .add_node(Node::new("api", NodeType::Interface))
            .add_node(Node::new("service", NodeType::Service))
            .add_node(Node::new("db", NodeType::DataStore))
            .add_edge(Edge::new("api", "service", RelationType::Calls))
            .add_edge(Edge::new("service", "db", RelationType::DependsOn))
            .build()
            .expect("valid graph");
        vec![
            ArchitectureCandidate {
                id: "invalid".to_string(),
                architecture: invalid,
                score: 0.9,
                depth: 0,
            },
            ArchitectureCandidate {
                id: "valid".to_string(),
                architecture: valid,
                score: 0.8,
                depth: 0,
            },
        ]
    }
}

#[test]
fn invalid_structure_is_removed_before_selection() {
    let runtime = CoreRuntime::new(
        Arc::new(InMemoryEngine::default()),
        Arc::new(InvalidFirstSearchEngine),
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

    assert_eq!(result.architecture.nodes().len(), 3);
    assert!(result.architecture.node(&"db".into()).is_some());
    assert_eq!(result.design.nodes().len(), 3);
    assert!(!result.files.is_empty());
    assert!(!result.generation_contexts.is_empty());
    assert!(result.project_layout.manifest_path.ends_with("toml"));
}
