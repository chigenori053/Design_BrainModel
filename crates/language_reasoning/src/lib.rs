pub mod concept_reasoning;
pub mod meaning_reasoner;
pub mod reasoning_actions;
pub mod reasoning_evaluator;
pub mod reasoning_search;
pub mod reasoning_state;
pub mod semantic_inference;

pub use concept_reasoning::expand_concepts;
pub use knowledge_engine::KnowledgeConfidence;
pub use knowledge_engine::KnowledgeGraph;
pub use knowledge_engine::knowledge_query_from_semantic_graph;
pub use meaning_reasoner::meaning_reasoning_search;
pub use reasoning_actions::ReasoningAction;
pub use reasoning_evaluator::{ReasoningEvaluator, ReasoningScore};
pub use reasoning_search::reasoning_graph_to_constraints;
pub use reasoning_state::ReasoningState;
pub use semantic_inference::infer_semantic_relations;

pub fn knowledge_reasoning_effective_confidence(graph: &KnowledgeGraph) -> f64 {
    if graph.relations.is_empty() {
        0.0
    } else {
        graph
            .relations
            .iter()
            .map(|relation| relation.confidence.effective_confidence)
            .sum::<f64>()
            / graph.relations.len() as f64
    }
}
