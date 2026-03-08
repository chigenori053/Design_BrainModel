use concept_engine::ConceptId;
use memory_space_complex::ComplexField;
use search_controller::{SearchConfig, SearchController, SearchState};

#[test]
fn search_pruning_effectiveness() {
    let strict = SearchController::new(SearchConfig {
        beam_width: 5,
        max_depth: 4,
        pruning_threshold: 0.9,
    });
    let loose = SearchController::new(SearchConfig {
        beam_width: 5,
        max_depth: 4,
        pruning_threshold: 0.0,
    });

    let initial = SearchState {
        state_vector: ComplexField::new(vec![]),
        score: 0.0,
        depth: 0,
    };
    let concepts = vec![
        ConceptId::from_name("DATABASE"),
        ConceptId::from_name("CACHE"),
        ConceptId::from_name("QUERY"),
    ];

    let strict_out = strict.search(initial.clone(), &concepts, &[], 1);
    let loose_out = loose.search(initial, &concepts, &[], 1);

    assert!(strict_out.len() <= loose_out.len());
}
