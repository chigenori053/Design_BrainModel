use architecture_domain::ArchitectureState;
use design_domain::{Architecture, DesignUnit, Layer};
use evaluation_engine::EvaluationEngine;
use knowledge_engine::{KnowledgeGraph, KnowledgeValidator};
use language_core::semantic_parser;
use memory_graph::DesignExperienceGraph;

#[test]
fn memory_graph_records_problem_architecture_evaluation_triplets() {
    let mut graph = DesignExperienceGraph::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let architecture_state = ArchitectureState::from_architecture(&architecture, Vec::new());
    let evaluation = EvaluationEngine::default().evaluate(&architecture_state);
    let meaning = semantic_parser("Build scalable REST API").to_meaning_graph();
    let knowledge_graph = KnowledgeGraph::default();
    let validator = KnowledgeValidator;
    let validation = validator.validate(&knowledge_graph, &[]);

    graph.record_experience_with_knowledge(
        meaning.clone(),
        Some(knowledge_graph.clone()),
        Some(validation),
        101,
        architecture_state.clone(),
        evaluation,
    );
    graph.record_experience_with_knowledge(
        meaning,
        Some(knowledge_graph),
        Some(validation),
        102,
        architecture_state,
        evaluation,
    );

    assert_eq!(graph.problems.len(), 2);
    assert_eq!(graph.knowledges.len(), 2);
    assert_eq!(graph.inferred_knowledges.len(), 2);
    assert_eq!(graph.stabilized_knowledges.len(), 2);
    assert_eq!(graph.lifecycle_states.len(), 2);
    assert_eq!(graph.lifecycle_metrics.len(), 2);
    assert_eq!(graph.architectures.len(), 2);
    assert_eq!(graph.evaluations.len(), 2);
    assert_eq!(graph.edges.len(), 2);
}
