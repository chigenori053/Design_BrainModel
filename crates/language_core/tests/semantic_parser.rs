use language_core::semantic_parser;

#[test]
fn semantic_parser_extracts_intent_and_pattern() {
    let state = semantic_parser("Build a scalable REST API");

    assert_eq!(
        state.intent.as_ref().map(|intent| intent.name.as_str()),
        Some("Build API")
    );
    assert!(
        state
            .semantic_graph
            .concepts
            .values()
            .any(|concept| concept.label == "rest")
    );
    assert!(
        state
            .semantic_graph
            .concepts
            .values()
            .any(|concept| concept.label == "api")
    );
}
