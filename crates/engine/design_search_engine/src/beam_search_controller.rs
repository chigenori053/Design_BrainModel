use memory_space_core::RecallResult;
use world_model::{DefaultSimulationEngine, SimulationEngine};
use world_model_core::WorldState;

use crate::architecture_evaluator::{ArchitectureEvaluator, DefaultArchitectureEvaluator};
use crate::pruning::prune_candidates;
use crate::search_config::SearchConfig;
use crate::search_controller::SearchController;
use crate::search_state::SearchState;

/// Beam search implementation of `SearchController`.
/// Deterministic: same input → same search tree → same ranking.
#[derive(Clone, Copy, Debug, Default)]
pub struct BeamSearchController;

impl SearchController for BeamSearchController {
    fn search(
        &self,
        initial_state: WorldState,
        recall: Option<&RecallResult>,
        config: &SearchConfig,
    ) -> Vec<SearchState> {
        let evaluator = DefaultArchitectureEvaluator;
        let simulator = DefaultSimulationEngine;
        let mut root_state = initial_state.clone();
        if let Some(recalled) = recall.and_then(|recall_result| initial_state.recall_seed(recall_result)) {
            if recall
                .and_then(|result| result.candidates.first())
                .map(|candidate| candidate.relevance_score >= 0.8)
                .unwrap_or(false)
            {
                root_state = recalled;
            }
        }

        let mut root = SearchState::new(root_state.state_id, root_state.clone());
        assess_state(&mut root, recall, &evaluator, &simulator);
        root.depth = 0;

        let mut beam = vec![root];

        for depth in 1..=config.max_depth {
            let mut candidates: Vec<SearchState> = Vec::new();

            for parent in &beam {
                let children = expand(parent, depth, config.max_candidates);
                for mut child in children {
                    assess_state(&mut child, recall, &evaluator, &simulator);
                    candidates.push(child);
                }
            }

            if candidates.is_empty() {
                break;
            }

            beam = prune_candidates(candidates, config.beam_width);
        }

        beam
    }
}

/// Deterministic expansion from the action model.
fn expand(parent: &SearchState, depth: usize, max_candidates: usize) -> Vec<SearchState> {
    let unit_ids = parent.world_state.architecture.all_design_unit_ids();
    let mut actions = vec![
        world_model_core::Action::AddDesignUnit {
            name: format!("DesignUnitDepth{depth}"),
        },
        world_model_core::Action::SplitStructure,
        world_model_core::Action::MergeStructure,
    ];

    if !unit_ids.is_empty() {
        actions.push(world_model_core::Action::RemoveDesignUnit);
    }
    if unit_ids.len() >= 2 {
        actions.push(world_model_core::Action::ConnectDependency {
            from: unit_ids[0],
            to: unit_ids[1],
        });
    }

    actions
        .into_iter()
        .take(max_candidates.max(1))
        .enumerate()
        .map(|(index, action)| {
            let child_id = parent
                .state_id
                .wrapping_mul(31)
                .wrapping_add(depth as u64 * 7)
                .wrapping_add(index as u64 + 1);
            let next_world = parent.world_state.apply_action(&action, child_id);

            SearchState {
                state_id: child_id,
                world_state: WorldState {
                    depth,
                    ..next_world
                },
                depth,
                score: 0.0,
                pareto_rank: 0,
            }
        })
        .collect()
}

fn assess_state(
    state: &mut SearchState,
    recall: Option<&RecallResult>,
    evaluator: &impl ArchitectureEvaluator,
    simulator: &impl SimulationEngine,
) {
    let simulation = simulator.simulate(&state.world_state, recall);
    state.world_state.simulation = Some(simulation);
    state.world_state.evaluation = evaluator.evaluate_vector(state);
    state.world_state.score = state.world_state.evaluation.total();
    state.score = evaluator.evaluate(state);
}
