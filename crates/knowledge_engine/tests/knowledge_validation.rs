use knowledge_engine::{
    KnowledgeDocument, KnowledgeMetadata, KnowledgeParser, KnowledgeSource, KnowledgeValidator,
};

#[test]
fn validator_scores_consistent_graph_with_reliable_sources() {
    let parser = KnowledgeParser;
    let validator = KnowledgeValidator;
    let documents = vec![KnowledgeDocument {
        source: KnowledgeSource::LocalDocument,
        content: "REST API should remain stateless. API gateway requires service discovery."
            .to_string(),
        metadata: KnowledgeMetadata {
            title: "REST notes".to_string(),
            source_uri: "local://rest".to_string(),
            reliability_hint: 0.95,
        },
    }];
    let graph = parser.parse_documents(&documents);
    let score = validator.validate(&graph, &documents);

    assert!(score.consistency > 0.0);
    assert!(score.source_reliability > 0.8);
    assert!(score.confidence > 0.5);
}
