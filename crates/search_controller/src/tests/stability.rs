use concept_engine::ConceptId;
use memory_space_complex::ComplexField;

use crate::{SearchConfig, SearchController, SearchState};

#[test]
fn stable_ordering() {
    let controller = SearchController::new(SearchConfig {
        beam_width: 5,
        max_depth: 3,
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

    let out = controller.search(initial, &concepts, &[], 2);
    let mut sorted = out.clone();
    sorted.sort_by(|l, r| {
        r.score
            .total_cmp(&l.score)
            .then_with(|| l.depth.cmp(&r.depth))
    });
    assert_eq!(out, sorted);
}
