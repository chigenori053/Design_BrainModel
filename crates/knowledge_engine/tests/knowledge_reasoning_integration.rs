use knowledge_engine::{
    KnowledgeEngine, LocalDocumentRetriever, integrate_knowledge_into_semantic_graph,
    knowledge_query_from_semantic_graph,
};
use language_core::semantic_parser;

#[test]
fn knowledge_extends_semantic_graph_with_external_entities() {
    let state = semantic_parser("Build scalable REST API");
    let query = knowledge_query_from_semantic_graph(&state.semantic_graph, &state.source_text);
    let engine = KnowledgeEngine::new(LocalDocumentRetriever);
    let integration = engine.process_query(query);

    let mut semantic_graph = state.semantic_graph.clone();
    integrate_knowledge_into_semantic_graph(&mut semantic_graph, &integration.knowledge_graph);

    assert!(
        semantic_graph
            .concepts
            .values()
            .any(|concept| concept.label == "stateless")
    );
}
