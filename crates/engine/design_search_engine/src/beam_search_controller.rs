use design_domain::Layer;
use design_grammar::GrammarEngine;
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
        let grammar = GrammarEngine::default();
        let mut root_state = initial_state.clone();
        if let Some(recalled) =
            recall.and_then(|recall_result| initial_state.recall_seed(recall_result))
        {
            if recall
                .and_then(|result| result.candidates.first())
                .map(|candidate| candidate.relevance_score >= 0.8)
                .unwrap_or(false)
            {
                root_state = recalled;
            }
        }

        let mut root = SearchState::new(root_state.state_id, root_state.clone());
        if !assess_state(&mut root, recall, &evaluator, &simulator, &grammar) {
            return Vec::new();
        }
        root.depth = 0;

        let mut beam = vec![root];

        for depth in 1..=config.max_depth {
            let mut candidates: Vec<SearchState> = Vec::new();

            for parent in &beam {
                let children = expand(parent, depth, config.max_candidates);
                for mut child in children {
                    if assess_state(&mut child, recall, &evaluator, &simulator, &grammar) {
                        candidates.push(child);
                    }
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
    let layer = match depth % 4 {
        1 => Layer::Ui,
        2 => Layer::Service,
        3 => Layer::Repository,
        _ => Layer::Database,
    };
    let unit_name = match layer {
        Layer::Ui => format!("ControllerDepth{depth}"),
        Layer::Service => format!("ServiceDepth{depth}"),
        Layer::Repository => format!("RepositoryDepth{depth}"),
        Layer::Database => format!("DatabaseDepth{depth}"),
    };
    let mut actions = vec![
        world_model_core::Action::AddDesignUnit {
            name: unit_name,
            layer,
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
                grammar_validation: None,
            }
        })
        .collect()
}

fn assess_state(
    state: &mut SearchState,
    recall: Option<&RecallResult>,
    evaluator: &impl ArchitectureEvaluator,
    simulator: &impl SimulationEngine,
    grammar: &GrammarEngine,
) -> bool {
    let validation = grammar.validate_world_state(&state.world_state);
    state.grammar_validation = Some(validation.clone());
    if !validation.valid {
        return false;
    }

    let causal_graph = state.world_state.architecture.causal_graph();
    let causal_validation = causal_graph.validate();
    if !causal_validation.valid {
        return false;
    }
    let causal_score = score_causal_closure(&causal_graph);

    let simulation = simulator.simulate(&state.world_state, recall);
    state.world_state.simulation = Some(simulation);
    state.world_state.evaluation = evaluator.evaluate_vector(state);
    state.world_state.score =
        (state.world_state.evaluation.total() + causal_score * 0.1).clamp(0.0, 1.0);
    state.score = (evaluator.evaluate(state) + causal_score * 0.1).clamp(0.0, 1.0);
    true
}

fn score_causal_closure(graph: &design_domain::CausalGraph) -> f64 {
    let node_count = graph.nodes().count();
    if node_count <= 1 {
        return 0.0;
    }

    let reachable = graph
        .closure_map()
        .values()
        .map(|closure| closure.len())
        .sum::<usize>() as f64;
    let max_reachable = (node_count * (node_count - 1)) as f64;
    (reachable / max_reachable).clamp(0.0, 1.0)
}
