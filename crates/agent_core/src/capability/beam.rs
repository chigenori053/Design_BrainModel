use core_types::ObjectiveVector;
use hybrid_vm::HybridVM;
use memory_space::DesignState;

use crate::{BeamSearch, DepthFront, SearchMode, SearchResult, SOFT_PARETO_TEMPERATURE};

impl<'a> BeamSearch<'a> {
    pub fn search(&self, initial_state: &DesignState) -> Vec<DesignState> {
        self.search_with_mode(initial_state, SearchMode::Auto)
            .final_frontier
    }

    pub fn search_with_mode(&self, initial_state: &DesignState, mode: SearchMode) -> SearchResult {
        if self.config.beam_width == 0 || self.config.max_depth == 0 {
            return SearchResult {
                final_frontier: vec![initial_state.clone()],
                depth_fronts: vec![DepthFront {
                    depth: 0,
                    state_ids: vec![initial_state.id],
                }],
            };
        }

        let mut frontier = vec![initial_state.clone()];
        let mut all_depths = Vec::new();
        for depth in 0..self.config.max_depth {
            let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
            for state in &frontier {
                for rule in HybridVM::applicable_rules(self.shm, state) {
                    let new_state = crate::apply_atomic(rule, state);
                    let obj = self.evaluator.evaluate(&new_state);
                    candidates.push((new_state, obj));
                }
            }
            if candidates.is_empty() {
                break;
            }

            let (normalized, _) = crate::normalize_by_depth(candidates, self.config.norm_alpha);
            let front_states = crate::capability::selection::soft_front_rank(normalized, SOFT_PARETO_TEMPERATURE);
            frontier = front_states
                .into_iter()
                .take(self.config.beam_width)
                .map(|(state, _)| state)
                .collect();

            all_depths.push(DepthFront {
                depth: depth + 1,
                state_ids: frontier.iter().map(|state| state.id).collect(),
            });
            if frontier.is_empty() {
                break;
            }
        }

        let depth_fronts = match mode {
            SearchMode::Auto => all_depths.last().cloned().into_iter().collect(),
            SearchMode::Manual => all_depths,
        };
        SearchResult {
            final_frontier: frontier,
            depth_fronts,
        }
    }
}
