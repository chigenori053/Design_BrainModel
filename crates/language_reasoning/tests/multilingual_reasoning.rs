use language_core::{language_search, semantic_parser};
use language_reasoning::meaning_reasoning_search;

#[test]
fn multilingual_reasoning_preserves_semantics() {
    let mut state = semantic_parser("Construir API REST escalable");
    state.semantic_graph = meaning_reasoning_search(state.semantic_graph.clone());
    let searched = language_search(state);
    let labels = searched
        .semantic_graph
        .concepts
        .values()
        .map(|concept| concept.label.as_str())
        .collect::<Vec<_>>();

    assert!(labels.contains(&"api"));
    assert!(labels.contains(&"rest"));
    assert!(labels.contains(&"scalable"));
    assert!(labels.contains(&"stateless"));
    assert!(labels.contains(&"layered_architecture"));
    assert!(searched.generated_sentence.is_some());
}
