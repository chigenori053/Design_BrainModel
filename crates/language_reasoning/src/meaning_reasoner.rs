use language_core::SemanticGraph;

pub fn meaning_reasoning_search(initial_graph: SemanticGraph) -> SemanticGraph {
    crate::reasoning_search::search(initial_graph)
}
