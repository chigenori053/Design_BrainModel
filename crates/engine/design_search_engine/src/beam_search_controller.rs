use std::sync::{Arc, Mutex};

use architecture_domain::ArchitectureState;
use design_domain::Layer;
use design_grammar::GrammarEngine;
use evaluation_engine::EvaluationEngine;
use math_reasoning_engine::{
    DefaultMathematicalReasoningEngine, MathReasoningTrace, MathematicalReasoningEngine,
};
use memory_graph::DesignExperienceGraph;
use memory_space_core::RecallResult;
use memory_space_phase14::{InMemoryMemorySpace, MemorySpace, SearchPrior, store_state_experience};
use policy_engine::{PolicyStore, evaluate_policy, policy_weight_for_action};
use simulation_scheduler::{
    DefaultSimulationScheduler, LightSimulationTrace, SimulationSchedulerConfig,
    SimulationSchedulerTrace, architecture_hash,
};
use world_model::{DefaultSimulationEngine, SimulationEngine};
use world_model_core::{Action, WorldState};

use crate::architecture_evaluator::{ArchitectureEvaluator, DefaultArchitectureEvaluator};
use crate::audit::{AuditCore, AuditDecision, AuditTelemetryEvent};
use crate::pruning::{SearchNodeDiversityPruned, prune_candidates_with_telemetry};
use crate::search_config::SearchConfig;
use crate::search_context::SearchContext;
use crate::search_controller::SearchController;
use crate::search_state::SearchState;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchTrace {
    pub final_beam: Vec<SearchState>,
    pub explored_state_count: usize,
    pub depth_best_scores: Vec<f64>,
    pub diversity_pruned: Vec<SearchNodeDiversityPruned>,
    pub audit_events: Vec<AuditTelemetryEvent>,
    pub scheduler_trace: SimulationSchedulerTrace,
    pub light_simulation_traces: Vec<LightSimulationTrace>,
    pub math_reasoning_traces: Vec<MathReasoningTrace>,
}

/// Beam search implementation of `SearchController`.
/// Deterministic: same input → same search tree → same ranking.
#[derive(Clone, Debug)]
pub struct BeamSearchController {
    pub memory: Arc<Mutex<InMemoryMemorySpace>>,
    pub policy_store: Arc<Mutex<PolicyStore>>,
    pub experience_graph: Arc<Mutex<DesignExperienceGraph>>,
    pub audit_core: AuditCore,
    pub simulation_scheduler: DefaultSimulationScheduler,
}

impl Default for BeamSearchController {
    fn default() -> Self {
        Self {
            memory: Arc::new(Mutex::new(InMemoryMemorySpace::with_bootstrap_patterns())),
            policy_store: Arc::new(Mutex::new(PolicyStore::default())),
            experience_graph: Arc::new(Mutex::new(DesignExperienceGraph::default())),
            audit_core: AuditCore::default(),
            simulation_scheduler: DefaultSimulationScheduler::with_config(
                SimulationSchedulerConfig::default(),
            ),
        }
    }
}

impl SearchController for BeamSearchController {
    fn search(
        &self,
        initial_state: WorldState,
        recall: Option<&RecallResult>,
        config: &SearchConfig,
    ) -> Vec<SearchState> {
        self.search_trace_with_context(initial_state, recall, config, &SearchContext::default())
            .final_beam
    }
}

impl BeamSearchController {
    pub fn search_trace(
        &self,
        initial_state: WorldState,
        recall: Option<&RecallResult>,
        config: &SearchConfig,
    ) -> SearchTrace {
        self.search_trace_with_context(initial_state, recall, config, &SearchContext::default())
    }

    pub fn search_trace_with_context(
        &self,
        initial_state: WorldState,
        recall: Option<&RecallResult>,
        config: &SearchConfig,
        ctx: &SearchContext,
    ) -> SearchTrace {
        let evaluator = DefaultArchitectureEvaluator;
        let evaluation_engine = EvaluationEngine::default();
        let math_engine = DefaultMathematicalReasoningEngine::default();
        let simulator = DefaultSimulationEngine;
        let grammar = GrammarEngine::default();
        let mut scheduler_trace = SimulationSchedulerTrace::default();
        let mut light_simulation_traces = Vec::new();
        let mut math_reasoning_traces = Vec::new();
        let request_text = ctx.intent_text.as_deref().unwrap_or("architecture search");
        let request_audit =
            self.audit_core
                .audit_request(&ctx.audit_context, ctx.feature_access, request_text);
        let mut audit_events = request_audit.telemetry.events.clone();
        if request_audit.decision == AuditDecision::Block {
            return SearchTrace {
                final_beam: Vec::new(),
                explored_state_count: 0,
                depth_best_scores: Vec::new(),
                diversity_pruned: Vec::new(),
                audit_events,
                scheduler_trace,
                light_simulation_traces,
                math_reasoning_traces,
            };
        }
        let effective_config = config.apply_capability_limits(&request_audit.capability_limits);
        let matched_patterns = self
            .memory
            .lock()
            .expect("memory space poisoned")
            .recall_patterns(&initial_state);
        let policy_snapshot = self
            .policy_store
            .lock()
            .expect("policy store poisoned")
            .latest()
            .cloned();
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
        if !prepare_state(
            &mut root,
            &evaluator,
            &evaluation_engine,
            &math_engine,
            &grammar,
            effective_config.experience_bias,
            effective_config.policy_bias,
        ) {
            return SearchTrace {
                audit_events,
                scheduler_trace,
                light_simulation_traces,
                math_reasoning_traces,
                ..SearchTrace::default()
            };
        }
        if let Some(trace) = root.math_reasoning.clone() {
            math_reasoning_traces.push(trace);
        }
        let root_simulation = simulator.simulate(&root.world_state, recall);
        finalize_state_with_simulation(
            &mut root,
            root_simulation,
            &evaluator,
            effective_config.experience_bias,
            effective_config.policy_bias,
        );
        root.depth = 0;
        let root_audit = self.audit_core.audit_architecture(&root.architecture_state);
        audit_events.extend(root_audit.telemetry.events);
        if root_audit.decision == AuditDecision::Block {
            return SearchTrace {
                final_beam: Vec::new(),
                explored_state_count: 0,
                depth_best_scores: Vec::new(),
                diversity_pruned: Vec::new(),
                audit_events,
                scheduler_trace,
                light_simulation_traces,
                math_reasoning_traces,
            };
        }

        let mut beam = vec![root];
        let mut explored_state_count = beam.len();
        let mut depth_best_scores = vec![beam[0].score];
        let mut diversity_pruned = Vec::new();

        for depth in 1..=effective_config.max_depth {
            let mut candidates: Vec<SearchState> = Vec::new();

            for parent in &beam {
                let children = expand(
                    parent,
                    depth,
                    ctx.constrained_candidates(&effective_config),
                    SearchPrior::from_patterns(
                        &parent.world_state,
                        &matched_patterns,
                        &candidate_actions(
                            parent,
                            depth,
                            ctx.constrained_candidates(&effective_config),
                        ),
                    ),
                    evaluate_policy(
                        &parent.world_state,
                        &matched_patterns,
                        policy_snapshot.as_ref(),
                    ),
                );
                for mut child in children {
                    if prepare_state(
                        &mut child,
                        &evaluator,
                        &evaluation_engine,
                        &math_engine,
                        &grammar,
                        effective_config.experience_bias,
                        effective_config.policy_bias,
                    ) {
                        if let Some(trace) = child.math_reasoning.clone() {
                            math_reasoning_traces.push(trace.clone());
                            if trace.result.validity_score < 0.45
                                || !trace.result.constraint_satisfied
                            {
                                continue;
                            }
                        }
                        let architecture_audit = self
                            .audit_core
                            .audit_architecture(&child.architecture_state);
                        audit_events.extend(architecture_audit.telemetry.events);
                        if architecture_audit.decision == AuditDecision::Block {
                            continue;
                        }
                        child.score = (child.score + ctx.score_bias(&child)).clamp(0.0, 1.0);
                        child.world_state.score =
                            (child.world_state.score + ctx.score_bias(&child)).clamp(0.0, 1.0);
                        candidates.push(child);
                    }
                }
            }

            if candidates.is_empty() {
                break;
            }

            explored_state_count += candidates.len();
            let scheduled_batch = self.simulation_scheduler.rank_candidates(
                candidates
                    .iter()
                    .map(|candidate| candidate.architecture_state.clone())
                    .collect(),
            );
            merge_scheduler_trace(&mut scheduler_trace, &scheduled_batch.trace);
            light_simulation_traces.extend(scheduled_batch.light_traces.clone());

            let mut candidate_map: std::collections::HashMap<String, Vec<SearchState>> =
                std::collections::HashMap::new();
            for candidate in candidates {
                candidate_map
                    .entry(architecture_hash(&candidate.architecture_state))
                    .or_default()
                    .push(candidate);
            }
            let mut finalized_candidates = Vec::new();
            for scheduled in scheduled_batch.scheduled {
                if let Some(group) = candidate_map.get_mut(&scheduled.architecture_hash) {
                    if !group.is_empty() {
                        let mut state = group.remove(0);
                        finalize_state_with_simulation(
                            &mut state,
                            scheduled.simulation_result,
                            &evaluator,
                            effective_config.experience_bias,
                            effective_config.policy_bias,
                        );
                        finalized_candidates.push(state);
                    }
                }
            }
            if finalized_candidates.is_empty() {
                break;
            }
            let prune_outcome = prune_candidates_with_telemetry(
                finalized_candidates,
                ctx.constrained_beam_width(&effective_config),
                effective_config.diversity_threshold,
            );
            diversity_pruned.extend(prune_outcome.diversity_pruned);
            beam = prune_outcome.selected;
            let best_score = beam.iter().map(|state| state.score).fold(0.0_f64, f64::max);
            let running_best = depth_best_scores
                .last()
                .copied()
                .map(|previous| previous.max(best_score))
                .unwrap_or(best_score);
            depth_best_scores.push(running_best);
        }

        if let Ok(mut memory) = self.memory.lock() {
            for state in beam.iter().take(3) {
                store_state_experience(&mut *memory, &state.world_state, state.score);
            }
            if let Ok(mut policy_store) = self.policy_store.lock() {
                policy_store.update_policy(memory.experience_store.experiences());
            }
        }
        if let Ok(mut graph) = self.experience_graph.lock() {
            for state in beam.iter().take(3) {
                if let Some(result) = state.evaluation_result {
                    graph.record_experience(
                        Default::default(),
                        state.state_id,
                        state.architecture_state.clone(),
                        result,
                    );
                }
            }
        }

        SearchTrace {
            final_beam: beam,
            explored_state_count,
            depth_best_scores,
            diversity_pruned,
            audit_events,
            scheduler_trace,
            light_simulation_traces,
            math_reasoning_traces,
        }
    }
}

/// Deterministic expansion from the action model.
fn expand(
    parent: &SearchState,
    depth: usize,
    max_candidates: usize,
    prior: SearchPrior,
    policy_weights: std::collections::HashMap<policy_engine::ActionType, f64>,
) -> Vec<SearchState> {
    candidate_actions(parent, depth, max_candidates)
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
                architecture_state: ArchitectureState::default(),
                evaluation_result: None,
                depth,
                score: 0.0,
                prior_score: prior.weight_for(&action),
                policy_score: policy_weight_for_action(&action, &policy_weights),
                pareto_rank: 0,
                source_action: Some(action),
                grammar_validation: None,
                math_reasoning: None,
            }
        })
        .collect()
}

fn candidate_actions(parent: &SearchState, depth: usize, max_candidates: usize) -> Vec<Action> {
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

    actions.truncate(max_candidates.max(1));
    actions
}

fn prepare_state(
    state: &mut SearchState,
    evaluator: &impl ArchitectureEvaluator,
    evaluation_engine: &EvaluationEngine,
    math_engine: &impl MathematicalReasoningEngine,
    grammar: &GrammarEngine,
    experience_bias: f64,
    policy_bias: f64,
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

    state.architecture_state = ArchitectureState::from_architecture(
        &state.world_state.architecture,
        state.world_state.constraints.clone(),
    );
    state.math_reasoning = Some(math_engine.analyze_with_trace(&state.architecture_state));
    let evaluation_result = evaluation_engine.evaluate(&state.architecture_state);
    state.evaluation_result = Some(evaluation_result);
    state.world_state.evaluation = evaluator.evaluate_vector(state);
    let mathematical_validity = state
        .math_reasoning
        .as_ref()
        .map(|trace| trace.result.validity_score as f64)
        .unwrap_or(0.5);
    state.world_state.score = (evaluation_result.total_score * 0.6
        + state.world_state.evaluation.total() * 0.2
        + mathematical_validity * 0.1
        + causal_score * 0.1
        + (state.prior_score - 1.0).max(0.0) * experience_bias)
        .clamp(0.0, 1.0);
    state.score = (evaluation_result.total_score * 0.6
        + evaluator.evaluate(state) * 0.2
        + mathematical_validity * 0.1
        + causal_score * 0.1
        + (state.prior_score - 1.0).max(0.0) * experience_bias)
        .clamp(0.0, 1.0);
    state.world_state.score =
        (state.world_state.score + state.policy_score * policy_bias).clamp(0.0, 1.0);
    state.score = (state.score + state.policy_score * policy_bias).clamp(0.0, 1.0);
    true
}

fn finalize_state_with_simulation(
    state: &mut SearchState,
    simulation: world_model_core::SimulationResult,
    evaluator: &impl ArchitectureEvaluator,
    experience_bias: f64,
    policy_bias: f64,
) {
    let causal_score = score_causal_closure(&state.world_state.architecture.causal_graph());
    let mathematical_validity = state
        .math_reasoning
        .as_ref()
        .map(|trace| trace.result.validity_score as f64)
        .unwrap_or(0.5);
    state.world_state.simulation = Some(simulation);
    state.world_state.evaluation = evaluator.evaluate_vector(state);
    if let Some(evaluation_result) = state.evaluation_result {
        state.world_state.score = (evaluation_result.total_score * 0.6
            + state.world_state.evaluation.total() * 0.2
            + mathematical_validity * 0.1
            + causal_score * 0.1
            + (state.prior_score - 1.0).max(0.0) * experience_bias)
            .clamp(0.0, 1.0);
        state.score = (evaluation_result.total_score * 0.6
            + evaluator.evaluate(state) * 0.2
            + mathematical_validity * 0.1
            + causal_score * 0.1
            + (state.prior_score - 1.0).max(0.0) * experience_bias)
            .clamp(0.0, 1.0);
    }
    state.world_state.score =
        (state.world_state.score + state.policy_score * policy_bias).clamp(0.0, 1.0);
    state.score = (state.score + state.policy_score * policy_bias).clamp(0.0, 1.0);
}

fn merge_scheduler_trace(
    aggregate: &mut SimulationSchedulerTrace,
    current: &SimulationSchedulerTrace,
) {
    aggregate.filtered_candidates += current.filtered_candidates;
    aggregate.knowledge_evaluated += current.knowledge_evaluated;
    aggregate.light_simulated += current.light_simulated;
    aggregate.scheduled_candidates += current.scheduled_candidates;
    aggregate.cache_hits += current.cache_hits;
    aggregate.cache_misses += current.cache_misses;
    aggregate.full_simulations += current.full_simulations;
    aggregate
        .telemetry_events
        .extend(current.telemetry_events.clone());
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
