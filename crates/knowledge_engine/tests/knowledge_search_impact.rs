use knowledge_engine::{
    KnowledgeEngine, LocalDocumentRetriever, knowledge_graph_to_constraints,
    knowledge_query_from_semantic_graph,
};
use language_core::semantic_parser;

#[test]
fn knowledge_graph_adds_search_constraints() {
    let state = semantic_parser("Build scalable REST API with API gateway");
    let query = knowledge_query_from_semantic_graph(&state.semantic_graph, &state.source_text);
    let engine = KnowledgeEngine::new(LocalDocumentRetriever);
    let integration = engine.process_query(query);
    let constraints = knowledge_graph_to_constraints(&integration.knowledge_graph);

    assert!(
        constraints
            .iter()
            .any(|constraint| constraint.name == "knowledge_stateless")
    );
}
