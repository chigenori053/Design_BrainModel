use crate::{ArchitectureScore, SearchState, score_dominates};

pub trait ParetoOptimizer {
    fn select(&self, candidates: Vec<SearchState>) -> Vec<SearchState>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ParetoSetOptimizer;

impl ParetoOptimizer for ParetoSetOptimizer {
    fn select(&self, candidates: Vec<SearchState>) -> Vec<SearchState> {
        let mut frontier = candidates
            .iter()
            .enumerate()
            .filter(|(idx, candidate)| {
                !candidates.iter().enumerate().any(|(other_idx, other)| {
                    idx != &other_idx && dominates(&other.score, &candidate.score)
                })
            })
            .map(|(_, candidate)| candidate.clone())
            .collect::<Vec<_>>();

        frontier.sort_by(|lhs, rhs| {
            rhs.score
                .desirability()
                .total_cmp(&lhs.score.desirability())
                .then_with(|| lhs.depth.cmp(&rhs.depth))
                .then_with(|| lhs.state_id.cmp(&rhs.state_id))
        });
        frontier
    }
}

fn dominates(left: &ArchitectureScore, right: &ArchitectureScore) -> bool {
    score_dominates(left, right)
}
