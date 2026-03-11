use knowledge_engine::{
    KnowledgeEngine, KnowledgeSource, LocalDocumentRetriever, WebSearchRetriever,
    knowledge_query_from_semantic_graph,
};
use language_core::semantic_parser;

#[test]
fn retriever_returns_documents_for_rest_api_query() {
    let state = semantic_parser("Build scalable REST API");
    let query = knowledge_query_from_semantic_graph(&state.semantic_graph, &state.source_text);
    let engine = KnowledgeEngine::new(LocalDocumentRetriever);
    let out = engine.process_query(query);

    assert!(!out.documents.is_empty());
    assert!(
        out.documents
            .iter()
            .all(|doc| matches!(doc.source, KnowledgeSource::LocalDocument))
    );
}

#[test]
fn web_search_retriever_uses_seeded_knowledge() {
    let state = semantic_parser("Build scalable API gateway");
    let query = knowledge_query_from_semantic_graph(&state.semantic_graph, &state.source_text);
    let engine = KnowledgeEngine::new(WebSearchRetriever::default());
    let out = engine.process_query(query);

    assert!(!out.documents.is_empty());
    assert!(
        out.documents
            .iter()
            .any(|doc| matches!(doc.source, KnowledgeSource::WebSearch))
    );
}
