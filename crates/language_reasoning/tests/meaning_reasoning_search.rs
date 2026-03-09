use language_core::semantic_parser;
use language_reasoning::meaning_reasoning_search;

#[test]
fn meaning_reasoning_search_adds_inferred_structure() {
    let parsed = semantic_parser("Build scalable REST API");
    let expanded = meaning_reasoning_search(parsed.semantic_graph.clone());

    assert!(expanded.relations.len() > parsed.semantic_graph.relations.len());
    assert!(
        expanded
            .concepts
            .values()
            .any(|concept| concept.label == "layered_architecture")
    );
}
