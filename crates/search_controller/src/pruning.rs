use crate::search_state::SearchState;

pub fn prune(states: Vec<SearchState>, threshold: f64) -> Vec<SearchState> {
    states
        .into_iter()
        .filter(|state| state.score >= threshold)
        .collect()
}
