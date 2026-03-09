use language_core::{SemanticGraph, SemanticRelation};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReasoningState {
    pub semantic_graph: SemanticGraph,
    pub inferred_relations: Vec<SemanticRelation>,
    pub reasoning_score: f64,
}

impl ReasoningState {
    pub fn new(semantic_graph: SemanticGraph) -> Self {
        Self {
            semantic_graph,
            inferred_relations: Vec::new(),
            reasoning_score: 0.0,
        }
    }
}
