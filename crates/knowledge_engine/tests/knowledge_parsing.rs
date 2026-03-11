use knowledge_engine::{KnowledgeDocument, KnowledgeMetadata, KnowledgeParser, KnowledgeSource};

#[test]
fn parser_extracts_entities_and_relations() {
    let parser = KnowledgeParser;
    let graph = parser.parse(KnowledgeDocument {
        source: KnowledgeSource::LocalDocument,
        content: "REST API should remain stateless. API gateway requires service discovery."
            .to_string(),
        metadata: KnowledgeMetadata {
            title: "REST notes".to_string(),
            source_uri: "local://rest".to_string(),
            reliability_hint: 0.9,
        },
    });

    assert!(graph.entities.iter().any(|entity| entity.label == "rest"));
    assert!(
        graph
            .entities
            .iter()
            .any(|entity| entity.label == "stateless")
    );
    assert!(!graph.relations.is_empty());
}
