use crate::search_state::SearchState;

/// A search candidate with its final ranking score.
#[derive(Clone, Debug, PartialEq)]
pub struct RankedCandidate {
    pub state: SearchState,
    pub score: f64,
    pub pareto_rank: usize,
}

fn dominates(lhs: &SearchState, rhs: &SearchState) -> bool {
    let lhs_objectives = lhs.world_state.evaluation.objectives();
    let rhs_objectives = rhs.world_state.evaluation.objectives();
    let mut better_in_one = false;

    for (l, r) in lhs_objectives.iter().zip(rhs_objectives.iter()) {
        if l < r {
            return false;
        }
        if l > r {
            better_in_one = true;
        }
    }

    better_in_one
}

/// Rank candidates by Pareto dominance first, then scalar score for determinism.
pub fn rank_candidates(states: Vec<SearchState>) -> Vec<RankedCandidate> {
    let mut ranked: Vec<RankedCandidate> = states
        .into_iter()
        .map(|state| {
            let pareto_rank = 0;
            let score = state.score;
            RankedCandidate {
                state,
                score,
                pareto_rank,
            }
        })
        .collect();

    for index in 0..ranked.len() {
        let dominated_by = ranked
            .iter()
            .enumerate()
            .filter(|(other_index, other)| *other_index != index && dominates(&other.state, &ranked[index].state))
            .count();
        ranked[index].pareto_rank = dominated_by;
        ranked[index].state.pareto_rank = dominated_by;
    }

    ranked.sort_by(|lhs, rhs| {
        lhs.pareto_rank
            .cmp(&rhs.pareto_rank)
            .then_with(|| {
                rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| lhs.state.state_id.cmp(&rhs.state.state_id))
            })
    });

    ranked
}
