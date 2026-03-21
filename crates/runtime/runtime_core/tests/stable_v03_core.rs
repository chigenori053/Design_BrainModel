use std::sync::Arc;

use architecture_evaluator_core::stable_v03::{
    ArchitectureEvaluator, EvaluationMetrics, EvaluationResult, WeightedArchitectureEvaluator,
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
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use runtime_core::stable_v03::{CoreError, RuntimeResult};
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
        relations: vec!["depends_on".to_string()],
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
fn invalid_input_is_rejected() {
    let runtime = CoreRuntime::new(
        seeded_memory(),
        Arc::new(DeterministicBeamSearchEngine::default()),
        constraint_engine(),
        evaluator(),
        mapper(),
        code_ir_builder(),
        generator(),
    );
    let result = runtime.executor.execute(IntentInput::new("   "));

    assert_eq!(result, Err(CoreError::InvalidInput));
}

#[test]
fn runtime_trace_is_deterministic_for_same_input() {
    let input = IntentInput::new("api service db");
    let expected: RuntimeResult = CoreRuntime::new(
        seeded_memory(),
        Arc::new(DeterministicBeamSearchEngine::default()),
        constraint_engine(),
        evaluator(),
        mapper(),
        code_ir_builder(),
        generator(),
    )
    .executor
    .execute(input.clone())
    .expect("runtime should succeed");

    for _ in 0..100 {
        let actual = CoreRuntime::new(
            seeded_memory(),
            Arc::new(DeterministicBeamSearchEngine::default()),
            constraint_engine(),
            evaluator(),
            mapper(),
            code_ir_builder(),
            generator(),
        )
        .executor
        .execute(input.clone())
        .expect("runtime should succeed");
        assert_eq!(actual.trace, expected.trace);
    }
}

struct FixedSearchEngine {
    candidates: Vec<ArchitectureCandidate>,
}

impl DesignSearchEngine for FixedSearchEngine {
    fn search(&self, _input: ReasoningInput) -> Vec<ArchitectureCandidate> {
        self.candidates.clone()
    }
}

struct ScoreByNodeCountEvaluator;

impl ArchitectureEvaluator for ScoreByNodeCountEvaluator {
    fn evaluate(&self, graph: &architecture_ir::stable_v03::ArchitectureGraph) -> EvaluationResult {
        EvaluationResult {
            score: 10.0 - graph.nodes().len() as f64,
            metrics: EvaluationMetrics {
                modularity: 0.0,
                coupling: 0.0,
                cohesion: 0.0,
                complexity: graph.nodes().len() as f64,
            },
        }
    }
}

#[test]
fn selector_picks_highest_scored_candidate() {
    let simple = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    let complex = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "db", RelationType::DependsOn))
        .build()
        .expect("valid graph");
    let runtime = CoreRuntime::new(
        Arc::new(InMemoryEngine::default()),
        Arc::new(FixedSearchEngine {
            candidates: vec![
                ArchitectureCandidate {
                    id: "complex".to_string(),
                    architecture: complex,
                    score: 0.5,
                    depth: 0,
                },
                ArchitectureCandidate {
                    id: "simple".to_string(),
                    architecture: simple.clone(),
                    score: 0.4,
                    depth: 0,
                },
            ],
        }),
        Arc::new(CompositeConstraintEngine::default()),
        Arc::new(ScoreByNodeCountEvaluator),
        mapper(),
        code_ir_builder(),
        generator(),
    );

    let result = runtime
        .executor
        .execute(IntentInput::new("api service db"))
        .expect("runtime should succeed");

    assert_eq!(result.architecture, simple);
    assert_eq!(result.design.nodes().len(), 2);
    assert!(!result.files.is_empty());
    assert!(!result.test_suites.is_empty());
    assert!(
        result
            .project_layout
            .files
            .iter()
            .any(|file| file.path.contains("/tests/test_"))
    );
    assert!(!result.execution_plan.test_plan.test_files.is_empty());
}
