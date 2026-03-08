use concept_engine::ConceptId;
use memory_space_api::ConceptRecallHit;
use memory_space_complex::ComplexField;
use search_controller::{SearchConfig, SearchController, SearchState};

#[test]
fn beam_search_determinism() {
    let controller = SearchController::new(SearchConfig::default());
    let initial = SearchState {
        state_vector: ComplexField::new(vec![]),
        score: 0.0,
        depth: 0,
    };
    let concepts = vec![
        ConceptId::from_name("DATABASE"),
        ConceptId::from_name("CACHE"),
    ];
    let memory = vec![ConceptRecallHit {
        concept: concepts[0],
        score: 0.8,
    }];

    let a = controller.search(initial.clone(), &concepts, &memory, 1);
    let b = controller.search(initial, &concepts, &memory, 1);

    assert_eq!(a, b);
}
