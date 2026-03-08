use memory_space_complex::ComplexField;

use crate::pruning::prune;
use crate::search_state::SearchState;

#[test]
fn beam_pruning() {
    let states = vec![
        SearchState {
            state_vector: ComplexField::new(vec![]),
            score: 0.1,
            depth: 1,
        },
        SearchState {
            state_vector: ComplexField::new(vec![]),
            score: 0.5,
            depth: 1,
        },
    ];

    let pruned = prune(states, 0.2);
    assert_eq!(pruned.len(), 1);
    assert!(pruned[0].score >= 0.2);
}
