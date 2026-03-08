use concept_engine::ConceptId;
use memory_space_complex::ComplexField;
use search_controller::{SearchConfig, SearchController, SearchState};

#[test]
fn intent_alignment_effect() {
    let controller = SearchController::new(SearchConfig::default());
    let initial = SearchState {
        state_vector: ComplexField::new(vec![]),
        score: 0.0,
        depth: 0,
    };
    let concepts = vec![ConceptId::from_name("DATABASE")];

    let low = controller.search(initial.clone(), &concepts, &[], 0);
    let high = controller.search(initial, &concepts, &[], 5);

    let low_score = low.first().map(|s| s.score).unwrap_or(0.0);
    let high_score = high.first().map(|s| s.score).unwrap_or(0.0);

    assert!(high_score >= low_score);
}
