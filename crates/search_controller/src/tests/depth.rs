use concept_engine::ConceptId;
use memory_space_complex::ComplexField;

use crate::{SearchConfig, SearchController, SearchState};

#[test]
fn depth_bound() {
    let controller = SearchController::new(SearchConfig {
        beam_width: 3,
        max_depth: 2,
        pruning_threshold: 0.0,
    });
    let initial = SearchState {
        state_vector: ComplexField::new(vec![]),
        score: 0.0,
        depth: 0,
    };
    let concepts = vec![ConceptId::from_name("A"), ConceptId::from_name("B")];

    let out = controller.search(initial, &concepts, &[], 0);
    assert!(out.iter().all(|s| s.depth <= 2));
}
