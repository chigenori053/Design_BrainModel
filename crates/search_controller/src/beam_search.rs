use crate::config::SearchConfig;
use crate::pruning::prune;
use crate::search_state::SearchState;

pub fn run_beam_search<FExpand, FScore>(
    initial: SearchState,
    config: SearchConfig,
    mut expand: FExpand,
    mut score: FScore,
) -> Vec<SearchState>
where
    FExpand: FnMut(&SearchState) -> Vec<SearchState>,
    FScore: FnMut(&SearchState) -> f64,
{
    let mut beam = vec![initial];

    for _ in 0..config.max_depth {
        let mut candidates = Vec::new();
        for state in &beam {
            let next = expand(state)
                .into_iter()
                .map(|mut s| {
                    s.score = score(&s);
                    s
                })
                .collect::<Vec<_>>();
            candidates.extend(next);
        }

        let mut pruned = prune(candidates, config.pruning_threshold);
        pruned.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.depth.cmp(&rhs.depth))
                .then_with(|| {
                    lhs.state_vector
                        .data
                        .len()
                        .cmp(&rhs.state_vector.data.len())
                })
        });
        pruned.truncate(config.beam_width.max(1));

        if pruned.is_empty() {
            break;
        }
        beam = pruned;
    }

    beam
}
