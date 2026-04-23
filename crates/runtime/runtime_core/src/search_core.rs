use std::sync::{Arc, Mutex};

use architecture_domain::ArchitectureState;
use design_domain::Layer;
use design_grammar::GrammarEngine;
use evaluation_engine::{EvaluationEngine, EvaluationResult};
use memory_graph::DesignExperienceGraph;
use memory_space_core::RecallResult;
use memory_space_phase14::{store_state_experience, InMemoryMemorySpace, MemorySpace, SearchPrior};
use policy_engine::{evaluate_policy, policy_weight_for_action, PolicyStore};
use world_model::{DefaultSimulationEngine, SimulationEngine};
use world_model_core::{Action, WorldState};

use crate::search_domain::{MAX_BEAM, MIN_BEAM, SearchInput, SearchPolicy, ScoredState};

// ── Beam-width control ────────────────────────────────────────────────────────

/// Returns a deterministic, bounded beam width from the policy.
/// Satisfies: MIN_BEAM ≤ result ≤ MAX_BEAM for every input.
pub fn effective_beam_width(policy: &SearchPolicy) -> usize {
    policy.beam_width.max(1).clamp(MIN_BEAM, MAX_BEAM)
}

// ── Internal candidate state ──────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub state_id: u64,
    pub world_state: WorldState,
    pub architecture_state: ArchitectureState,
    pub evaluation_result: Option<EvaluationResult>,
    pub depth: usize,
    /// Base score — evaluation only, no policy bias applied.
    pub base_score: f64,
    pub prior_score: f64,
    pub policy_score: f64,
    pub pareto_rank: usize,
    pub source_action: Option<Action>,
}

impl Candidate {
    fn new(state_id: u64, world_state: WorldState) -> Self {
        Self {
            state_id,
            world_state,
            architecture_state: ArchitectureState::default(),
            evaluation_result: None,
            depth: 0,
            base_score: 0.0,
            prior_score: 1.0,
            policy_score: 0.0,
            pareto_rank: 0,
            source_action: None,
        }
    }
}

// ── Beam search internals ─────────────────────────────────────────────────────

pub(crate) struct BeamSearchState {
    pub memory: Arc<Mutex<InMemoryMemorySpace>>,
    pub policy_store: Arc<Mutex<PolicyStore>>,
    pub experience_graph: Arc<Mutex<DesignExperienceGraph>>,
}

impl Default for BeamSearchState {
    fn default() -> Self {
        Self {
            memory: Arc::new(Mutex::new(InMemoryMemorySpace::with_bootstrap_patterns())),
            policy_store: Arc::new(Mutex::new(PolicyStore::default())),
            experience_graph: Arc::new(Mutex::new(DesignExperienceGraph::default())),
        }
    }
}

pub(crate) struct RawSearchOutput {
    pub beam: Vec<Candidate>,
    pub explored_count: usize,
    pub depth_best_scores: Vec<f64>,
}

/// Run pure beam search without any policy bias in the scoring.
/// Returns `Candidate` list whose `base_score` is:
///   `evaluation * 0.6 + architecture * 0.3 + causal * 0.1`
pub(crate) fn run_search(
    input: &SearchInput,
    beam_width: usize,
    state: &BeamSearchState,
) -> RawSearchOutput {
    let evaluator = architecture_evaluator_fn;
    let evaluation_engine = EvaluationEngine::default();
    let simulator = DefaultSimulationEngine;
    let grammar = GrammarEngine::default();

    let matched_patterns = state
        .memory
        .lock()
        .expect("memory poisoned")
        .recall_patterns(&input.world_state);

    let policy_snapshot = state
        .policy_store
        .lock()
        .expect("policy store poisoned")
        .latest()
        .cloned();

    let mut root_world = input.world_state.clone();
    if let Some(recalled) = input
        .recall
        .as_ref()
        .and_then(|r| input.world_state.recall_seed(r))
    {
        if input
            .recall
            .as_ref()
            .and_then(|r| r.candidates.first())
            .map(|c| c.relevance_score >= 0.8)
            .unwrap_or(false)
        {
            root_world = recalled;
        }
    }

    let mut root = Candidate::new(root_world.state_id, root_world.clone());
    if !assess_candidate(
        &mut root,
        input.recall.as_ref(),
        &evaluation_engine,
        &simulator,
        &grammar,
        evaluator,
    ) {
        return RawSearchOutput {
            beam: vec![],
            explored_count: 0,
            depth_best_scores: vec![],
        };
    }
    root.depth = 0;

    let mut beam = vec![root];
    let mut explored_count = beam.len();
    let mut depth_best_scores = vec![beam[0].base_score];

    for depth in 1..=input.max_depth {
        let mut candidates: Vec<Candidate> = Vec::new();

        for parent in &beam {
            let prior = SearchPrior::from_patterns(
                &parent.world_state,
                &matched_patterns,
                &candidate_actions(parent, depth, input.max_candidates),
            );
            let policy_weights = evaluate_policy(
                &parent.world_state,
                &matched_patterns,
                policy_snapshot.as_ref(),
            );

            for mut child in expand(parent, depth, input.max_candidates, prior, &policy_weights) {
                if assess_candidate(
                    &mut child,
                    input.recall.as_ref(),
                    &evaluation_engine,
                    &simulator,
                    &grammar,
                    evaluator,
                ) {
                    candidates.push(child);
                }
            }
        }

        if candidates.is_empty() {
            break;
        }

        explored_count += candidates.len();
        beam = prune(candidates, beam_width);

        let best = beam
            .iter()
            .map(|c| c.base_score)
            .fold(0.0_f64, f64::max);
        let running_best = depth_best_scores
            .last()
            .copied()
            .map(|prev| prev.max(best))
            .unwrap_or(best);
        depth_best_scores.push(running_best);
    }

    // Record experiences from top results.
    if let Ok(mut memory) = state.memory.lock() {
        for c in beam.iter().take(3) {
            store_state_experience(&mut *memory, &c.world_state, c.base_score);
        }
        if let Ok(mut ps) = state.policy_store.lock() {
            ps.update_policy(memory.experience_store.experiences());
        }
    }
    if let Ok(mut graph) = state.experience_graph.lock() {
        for c in beam.iter().take(3) {
            if let Some(result) = c.evaluation_result {
                graph.record_experience(
                    Default::default(),
                    c.state_id,
                    c.architecture_state.clone(),
                    result,
                );
            }
        }
    }

    RawSearchOutput {
        beam,
        explored_count,
        depth_best_scores,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn architecture_evaluator_fn(c: &Candidate) -> f64 {
    use world_model_core::evaluate_architecture;
    let depth_decay = 1.0 / (1.0 + c.depth as f64 * 0.1);
    let mut vec = evaluate_architecture(&c.world_state.architecture, &c.world_state.constraints);
    if let Some(sim) = &c.world_state.simulation {
        vec.simulation_quality = sim.total();
        vec.constraint_satisfaction =
            ((vec.constraint_satisfaction + sim.constraint_score) / 2.0).clamp(0.0, 1.0);
    }
    (vec.total() * depth_decay).clamp(0.0, 1.0)
}

fn expand(
    parent: &Candidate,
    depth: usize,
    max_candidates: usize,
    prior: SearchPrior,
    policy_weights: &std::collections::HashMap<policy_engine::ActionType, f64>,
) -> Vec<Candidate> {
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
            Candidate {
                state_id: child_id,
                world_state: WorldState { depth, ..next_world },
                architecture_state: ArchitectureState::default(),
                evaluation_result: None,
                depth,
                base_score: 0.0,
                prior_score: prior.weight_for(&action),
                policy_score: policy_weight_for_action(&action, policy_weights),
                pareto_rank: 0,
                source_action: Some(action),
            }
        })
        .collect()
}

fn candidate_actions(parent: &Candidate, depth: usize, max_candidates: usize) -> Vec<Action> {
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
        Action::AddDesignUnit { name: unit_name, layer },
        Action::SplitStructure,
        Action::MergeStructure,
    ];
    if !unit_ids.is_empty() {
        actions.push(Action::RemoveDesignUnit);
    }
    if unit_ids.len() >= 2 {
        actions.push(Action::ConnectDependency {
            from: unit_ids[0],
            to: unit_ids[1],
        });
    }
    actions.truncate(max_candidates.max(1));
    actions
}

fn assess_candidate(
    c: &mut Candidate,
    recall: Option<&RecallResult>,
    evaluation_engine: &EvaluationEngine,
    simulator: &DefaultSimulationEngine,
    grammar: &GrammarEngine,
    arch_eval: fn(&Candidate) -> f64,
) -> bool {
    let validation = grammar.validate_world_state(&c.world_state);
    if !validation.valid {
        return false;
    }

    let causal_graph = c.world_state.architecture.causal_graph();
    if !causal_graph.validate().valid {
        return false;
    }
    let causal_score = score_causal_closure(&causal_graph);

    let simulation = simulator.simulate(&c.world_state, recall);
    c.world_state.simulation = Some(simulation);
    c.architecture_state = ArchitectureState::from_architecture(
        &c.world_state.architecture,
        c.world_state.constraints.clone(),
    );
    let eval_result = evaluation_engine.evaluate(&c.architecture_state);
    c.evaluation_result = Some(eval_result);

    let arch_score = arch_eval(c);
    c.world_state.evaluation =
        world_model_core::evaluate_architecture(&c.world_state.architecture, &c.world_state.constraints);

    // Base score: pure evaluation without policy bias.
    c.base_score = (eval_result.total_score * 0.6 + arch_score * 0.3 + causal_score * 0.1)
        .clamp(0.0, 1.0);
    c.world_state.score = c.base_score;
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

// ── Pareto-aware pruning ──────────────────────────────────────────────────────

fn prune(mut candidates: Vec<Candidate>, beam_width: usize) -> Vec<Candidate> {
    // Assign Pareto ranks.
    let n = candidates.len();
    let mut ranks = vec![0usize; n];
    for i in 0..n {
        let dominated_by = (0..n)
            .filter(|&j| j != i && dominates(&candidates[j], &candidates[i]))
            .count();
        ranks[i] = dominated_by;
        candidates[i].pareto_rank = dominated_by;
    }

    // Sort: lower Pareto rank first, then higher base_score, then state_id for determinism.
    candidates.sort_by(|a, b| {
        a.pareto_rank
            .cmp(&b.pareto_rank)
            .then_with(|| b.base_score.total_cmp(&a.base_score))
            .then_with(|| a.state_id.cmp(&b.state_id))
    });
    candidates.truncate(beam_width.max(1));
    candidates
}

fn dominates(a: &Candidate, b: &Candidate) -> bool {
    let ao = a.world_state.evaluation.objectives();
    let bo = b.world_state.evaluation.objectives();
    let mut better = false;
    for (av, bv) in ao.iter().zip(bo.iter()) {
        if av < bv {
            return false;
        }
        if av > bv {
            better = true;
        }
    }
    better
}

// ── Conversion to public type ─────────────────────────────────────────────────

pub(crate) fn candidate_to_scored(c: Candidate) -> ScoredState {
    ScoredState {
        world_state: c.world_state,
        score: c.base_score,
        base_score: c.base_score,
        prior_score: c.prior_score,
        policy_score: c.policy_score,
        depth: c.depth,
    }
}
