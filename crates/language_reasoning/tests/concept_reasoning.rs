use language_core::semantic_parser;
use language_reasoning::expand_concepts;

#[test]
fn concept_reasoning_adds_architecture_concepts() {
    let parsed = semantic_parser("Build scalable REST API");
    let expanded = expand_concepts(&parsed.semantic_graph);
    let labels = expanded
        .concepts
        .values()
        .map(|concept| concept.label.as_str())
        .collect::<Vec<_>>();

    assert!(labels.contains(&"stateless"));
    assert!(labels.contains(&"layered_architecture"));
}
