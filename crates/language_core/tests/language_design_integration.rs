use language_core::{language_search, semantic_graph_to_constraints, semantic_parser};

#[test]
fn language_output_maps_to_design_constraints() {
    let searched = language_search(semantic_parser("Build a scalable REST API"));
    let constraints = semantic_graph_to_constraints(&searched);

    assert!(
        constraints
            .iter()
            .any(|constraint| constraint.name == "api_layers")
    );
    assert!(
        constraints
            .iter()
            .any(|constraint| constraint.name == "scalable")
    );
}
