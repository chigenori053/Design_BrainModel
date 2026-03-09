use language_core::semantic_parser;
use language_reasoning::meaning_reasoning_search;

#[test]
fn reasoning_is_deterministic_for_same_input() {
    let parsed = semantic_parser("Build scalable REST API");
    let baseline = meaning_reasoning_search(parsed.semantic_graph.clone());

    for _ in 0..20 {
        let candidate = meaning_reasoning_search(parsed.semantic_graph.clone());
        assert_eq!(candidate, baseline);
    }
}
