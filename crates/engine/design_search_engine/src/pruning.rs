use crate::{rank_candidates, search_state::SearchState};

/// Retain only the top `beam_width` candidates by Pareto-aware ranking.
pub fn prune_candidates(states: Vec<SearchState>, beam_width: usize) -> Vec<SearchState> {
    let mut ranked = rank_candidates(states);
    ranked.truncate(beam_width.max(1));
    ranked
        .into_iter()
        .map(|candidate| candidate.state)
        .collect()
}
