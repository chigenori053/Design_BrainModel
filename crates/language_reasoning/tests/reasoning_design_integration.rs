use language_core::{language_search, semantic_parser};
use language_reasoning::{meaning_reasoning_search, reasoning_graph_to_constraints};

#[test]
fn reasoning_graph_maps_to_design_constraints() {
    let mut state = semantic_parser("Build scalable REST API");
    state.semantic_graph = meaning_reasoning_search(state.semantic_graph.clone());
    let constraints = reasoning_graph_to_constraints(&state.semantic_graph);
    let searched = language_search(state);

    assert!(
        constraints
            .iter()
            .any(|constraint| constraint.name == "stateless")
    );
    assert!(
        constraints
            .iter()
            .any(|constraint| constraint.name == "layered_architecture")
    );
    assert!(
        searched
            .generated_sentence
            .as_deref()
            .unwrap_or_default()
            .contains("layered_architecture")
    );
}
