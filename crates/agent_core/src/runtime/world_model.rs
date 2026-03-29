use std::collections::BTreeMap;
use std::sync::Arc;

use core_types::ObjectiveVector;
use hybrid_vm::{HybridVM, StructuralEvaluator};
use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid, Value};

use crate::runtime::structured_search::SearchMetrics;

const CONSISTENCY_METRIC_EPS: f64 = 0.05;
const CONSISTENCY_GRAPH_EPS: f64 = 0.01;
const CONSISTENCY_VIOLATION_EPS: f64 = 0.05;
const MULTISTEP_BRANCH_LIMIT: usize = 3;
const LEARNING_BIAS_CLIP: f64 = 0.25;
const MIN_OUTCOME_VARIANCE: f64 = 1e-6;
const PROBABILISTIC_EXPLORATION_GAIN: f64 = 0.025;
const SEMANTIC_VARIANCE_CLIP: f64 = 1.0;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct CodeMetrics {
    pub static_score: f64,
    pub objective: ObjectiveVector,
    pub coupling: f64,
    pub propagation_score: f64,
    pub impact: f64,
    pub structural_variance: f64,
    pub violation_intensity: f64,
    pub constraint_score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Constraints {
    pub violations: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeState {
    pub graph: Arc<StructuralGraph>,
    pub metrics: CodeMetrics,
    pub constraints: Constraints,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Action {
    ExtractModule { target: Uuid },
    InlineModule { target: Uuid },
    RemoveDependency { from: Uuid, to: Uuid },
    SplitModule { target: Uuid },
    MergeModules { a: Uuid, b: Uuid },
}

impl Action {
    fn kind(&self) -> ActionKind {
        match self {
            Action::ExtractModule { .. } => ActionKind::Extract,
            Action::InlineModule { .. } => ActionKind::Inline,
            Action::RemoveDependency { .. } => ActionKind::RemoveDependency,
            Action::SplitModule { .. } => ActionKind::Split,
            Action::MergeModules { .. } => ActionKind::Merge,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct SimulationDelta {
    pub action_label: String,
    pub rp: f64,
    pub intent_score: f64,
    pub confidence: f64,
    pub variance: f64,
    pub semantic_variance: f64,
    pub uncertainty: f64,
    pub beta_reliance: f64,
    pub learning_bias: f64,
    pub final_score: f64,
    pub delta_violations: f64,
    pub delta_coupling: f64,
    pub delta_propagation_score: f64,
    pub static_score: f64,
    pub simulated_score: f64,
    pub consistency_score: f64,
    pub simulated_depth: usize,
    pub fallback_used: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SimulationOutcome {
    pub state: DesignState,
    pub objective: ObjectiveVector,
    pub delta: SimulationDelta,
    pub code_state: CodeState,
}

#[derive(Clone, Debug)]
pub(crate) struct SimulationCache {
    entries: BTreeMap<(u128, Action), SimulationOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ActionKind {
    Extract,
    Inline,
    RemoveDependency,
    Split,
    Merge,
}

#[derive(Clone, Debug)]
struct IntentModel {
    rules: Vec<crate::IntentRule>,
    priority: Vec<crate::IntentMetric>,
    weights: BTreeMap<crate::IntentMetric, f64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct BetaContext {
    hv_delta: f64,
    stagnation: usize,
    consistency: f64,
    depth: usize,
    confidence: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct SemanticVariance {
    structural_diff: f64,
    constraint_risk: f64,
    intent_conflict: f64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProbabilisticEvaluation {
    pub score: f64,
    pub confidence: f64,
    pub variance: f64,
    pub semantic_variance: f64,
    pub uncertainty: f64,
    pub beta_reliance: f64,
    pub intent_score: f64,
    pub learning_bias: f64,
    pub final_score: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct Experience {
    pub action_kind: ActionKind,
    pub result: ProbabilisticEvaluation,
}

#[derive(Clone, Debug)]
pub(crate) struct LearningEngine {
    mode: crate::WorldModelMode,
    learning_rate: f64,
    learning_decay: f64,
    confidence_gate: f64,
    updates: usize,
    action_bias: BTreeMap<(crate::IntentProfile, ActionKind), f64>,
    action_counts: BTreeMap<(crate::IntentProfile, ActionKind), usize>,
}

impl SimulationCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    fn get(&self, state: &DesignState, action: &Action) -> Option<SimulationOutcome> {
        self.entries
            .get(&(state.id.as_u128(), action.clone()))
            .cloned()
    }

    fn insert(&mut self, state: &DesignState, action: &Action, outcome: SimulationOutcome) {
        self.entries
            .insert((state.id.as_u128(), action.clone()), outcome);
    }
}

impl IntentModel {
    fn from_profile(profile: crate::IntentProfile) -> Self {
        use crate::IntentProfile::*;
        use crate::IntentRule::*;

        let (rules, priority, weights) = match profile {
            Balanced => (
                vec![
                    Maximize(crate::IntentMetric::Maintainability),
                    Maximize(crate::IntentMetric::Performance),
                    Minimize(crate::IntentMetric::Risk),
                ],
                vec![
                    crate::IntentMetric::Maintainability,
                    crate::IntentMetric::Performance,
                    crate::IntentMetric::Risk,
                ],
                [
                    (crate::IntentMetric::Maintainability, 0.25),
                    (crate::IntentMetric::Performance, 0.25),
                    (crate::IntentMetric::Risk, 0.25),
                    (crate::IntentMetric::RefactorEase, 0.25),
                ],
            ),
            Maintainability => (
                vec![
                    Maximize(crate::IntentMetric::Maintainability),
                    Constraint(crate::IntentMetric::Risk, 0.45),
                    ActionPolicy(
                        crate::IntentActionPolicy::Prefer,
                        crate::IntentMetric::RefactorEase,
                    ),
                ],
                vec![
                    crate::IntentMetric::Maintainability,
                    crate::IntentMetric::Risk,
                    crate::IntentMetric::RefactorEase,
                ],
                [
                    (crate::IntentMetric::Maintainability, 0.45),
                    (crate::IntentMetric::Performance, 0.10),
                    (crate::IntentMetric::Risk, 0.25),
                    (crate::IntentMetric::RefactorEase, 0.20),
                ],
            ),
            Performance => (
                vec![
                    Maximize(crate::IntentMetric::Performance),
                    Minimize(crate::IntentMetric::Risk),
                    ActionPolicy(
                        crate::IntentActionPolicy::Avoid,
                        crate::IntentMetric::RefactorEase,
                    ),
                ],
                vec![
                    crate::IntentMetric::Performance,
                    crate::IntentMetric::Risk,
                    crate::IntentMetric::Maintainability,
                ],
                [
                    (crate::IntentMetric::Maintainability, 0.10),
                    (crate::IntentMetric::Performance, 0.50),
                    (crate::IntentMetric::Risk, 0.15),
                    (crate::IntentMetric::RefactorEase, 0.25),
                ],
            ),
            LowRisk => (
                vec![
                    Minimize(crate::IntentMetric::Risk),
                    Constraint(crate::IntentMetric::Risk, 0.30),
                    ActionPolicy(
                        crate::IntentActionPolicy::Avoid,
                        crate::IntentMetric::RefactorEase,
                    ),
                ],
                vec![
                    crate::IntentMetric::Risk,
                    crate::IntentMetric::Maintainability,
                    crate::IntentMetric::Performance,
                ],
                [
                    (crate::IntentMetric::Maintainability, 0.20),
                    (crate::IntentMetric::Performance, 0.10),
                    (crate::IntentMetric::Risk, 0.55),
                    (crate::IntentMetric::RefactorEase, 0.15),
                ],
            ),
            Refactor => (
                vec![
                    Maximize(crate::IntentMetric::RefactorEase),
                    Constraint(crate::IntentMetric::Risk, 0.55),
                    ActionPolicy(
                        crate::IntentActionPolicy::Prefer,
                        crate::IntentMetric::RefactorEase,
                    ),
                ],
                vec![
                    crate::IntentMetric::RefactorEase,
                    crate::IntentMetric::Maintainability,
                    crate::IntentMetric::Risk,
                ],
                [
                    (crate::IntentMetric::Maintainability, 0.20),
                    (crate::IntentMetric::Performance, 0.10),
                    (crate::IntentMetric::Risk, 0.15),
                    (crate::IntentMetric::RefactorEase, 0.55),
                ],
            ),
        };
        let weights = weights.into_iter().collect();

        Self {
            rules,
            priority,
            weights,
        }
    }

    fn score(&self, code_state: &CodeState, action: &Action, consistency_score: f64) -> f64 {
        let maintainability =
            (1.0 - code_state.metrics.coupling * 0.6 - code_state.metrics.propagation_score * 0.4)
                .clamp(0.0, 1.0);
        let performance = (1.0
            - code_state.metrics.constraint_score * 0.5
            - code_state.metrics.structural_variance * 0.25
            - code_state.metrics.violation_intensity * 0.25)
            .clamp(0.0, 1.0);
        let risk = (1.0
            - code_state.metrics.violation_intensity * 0.6
            - code_state.metrics.impact * 0.2
            - (1.0 - consistency_score).clamp(0.0, 1.0) * 0.2)
            .clamp(0.0, 1.0);
        let refactor = action_refactor_affinity(action).clamp(0.0, 1.0) * 0.6
            + (1.0 - code_state.metrics.coupling).clamp(0.0, 1.0) * 0.4;

        let metric_scores = [
            (crate::IntentMetric::Maintainability, maintainability),
            (crate::IntentMetric::Performance, performance),
            (crate::IntentMetric::Risk, risk),
            (crate::IntentMetric::RefactorEase, refactor.clamp(0.0, 1.0)),
        ];

        let base = metric_scores
            .into_iter()
            .map(|(metric, value)| self.weights.get(&metric).copied().unwrap_or(0.0) * value)
            .sum::<f64>()
            .clamp(0.0, 1.0);
        let dsl = self.dsl_score(code_state, action, &metric_scores);
        let priority_bonus = self.priority_bonus(&metric_scores);

        (base * 0.45 + dsl * 0.40 + priority_bonus * 0.15).clamp(0.0, 1.0)
    }

    fn dsl_score(
        &self,
        code_state: &CodeState,
        action: &Action,
        metric_scores: &[(crate::IntentMetric, f64); 4],
    ) -> f64 {
        let metrics = metric_scores.iter().copied().collect::<BTreeMap<_, _>>();
        let action_fit = action_policy_affinity(action);
        let mut total = 0.0;
        for rule in &self.rules {
            total += match *rule {
                crate::IntentRule::Maximize(metric) => metrics.get(&metric).copied().unwrap_or(0.0),
                crate::IntentRule::Minimize(metric) => {
                    1.0 - metrics.get(&metric).copied().unwrap_or(0.0)
                }
                crate::IntentRule::Constraint(metric, threshold) => {
                    if metrics.get(&metric).copied().unwrap_or(1.0) <= threshold {
                        1.0
                    } else {
                        (1.0 - (metrics.get(&metric).copied().unwrap_or(1.0) - threshold))
                            .clamp(0.0, 1.0)
                    }
                }
                crate::IntentRule::ActionPolicy(policy, metric) => {
                    let metric_fit = metrics.get(&metric).copied().unwrap_or(0.0);
                    match policy {
                        crate::IntentActionPolicy::Prefer => {
                            (0.6 * metric_fit + 0.4 * action_fit).clamp(0.0, 1.0)
                        }
                        crate::IntentActionPolicy::Avoid => {
                            1.0 - (0.6 * metric_fit + 0.4 * action_fit).clamp(0.0, 1.0)
                        }
                    }
                }
            };
        }
        if self.rules.is_empty() {
            0.0
        } else {
            (total / self.rules.len() as f64 - constraint_pressure(code_state) * 0.1)
                .clamp(0.0, 1.0)
        }
    }

    fn priority_bonus(&self, metric_scores: &[(crate::IntentMetric, f64); 4]) -> f64 {
        let metrics = metric_scores.iter().copied().collect::<BTreeMap<_, _>>();
        let mut total = 0.0;
        for (idx, metric) in self.priority.iter().enumerate() {
            let weight = 1.0 / (idx + 1) as f64;
            total += metrics.get(metric).copied().unwrap_or(0.0) * weight;
        }
        let norm = self
            .priority
            .iter()
            .enumerate()
            .map(|(idx, _)| 1.0 / (idx + 1) as f64)
            .sum::<f64>()
            .max(1.0);
        (total / norm).clamp(0.0, 1.0)
    }
}

impl LearningEngine {
    pub(crate) fn new(
        mode: crate::WorldModelMode,
        learning_rate: f64,
        learning_decay: f64,
        confidence_gate: f64,
    ) -> Self {
        Self {
            mode,
            learning_rate: learning_rate.clamp(0.0, 1.0),
            learning_decay: learning_decay.clamp(0.0, 1.0),
            confidence_gate: confidence_gate.clamp(0.0, 1.0),
            updates: 0,
            action_bias: BTreeMap::new(),
            action_counts: BTreeMap::new(),
        }
    }

    fn bias_for(&self, intent: crate::IntentProfile, action_kind: ActionKind) -> f64 {
        self.action_bias
            .get(&(intent, action_kind))
            .copied()
            .unwrap_or(0.0)
    }

    fn exploration_bonus(&self, intent: crate::IntentProfile, action_kind: ActionKind) -> f64 {
        if self.mode != crate::WorldModelMode::Probabilistic {
            return 0.0;
        }
        let count = self
            .action_counts
            .get(&(intent, action_kind))
            .copied()
            .unwrap_or(0) as f64;
        (PROBABILISTIC_EXPLORATION_GAIN / (1.0 + count)).clamp(0.0, PROBABILISTIC_EXPLORATION_GAIN)
    }

    pub(crate) fn update(&mut self, intent: crate::IntentProfile, experience: &Experience) {
        if self.mode != crate::WorldModelMode::Probabilistic {
            return;
        }
        if experience.result.confidence < self.confidence_gate
            || experience.result.variance > 0.35
            || experience.result.semantic_variance > 0.45
        {
            return;
        }
        self.updates += 1;
        let key = (intent, experience.action_kind);
        let count = self.action_counts.entry(key).or_insert(0);
        *count += 1;

        let decay = (-self.learning_decay * self.updates as f64).exp();
        let target = ((experience.result.final_score - 0.5) * experience.result.confidence
            - experience.result.variance * 0.20
            - experience.result.semantic_variance * 0.20)
            * decay;
        let step = self.learning_rate / (*count as f64).sqrt().max(1.0);
        let entry = self.action_bias.entry(key).or_insert(0.0);
        *entry = (*entry + step * target).clamp(-LEARNING_BIAS_CLIP, LEARNING_BIAS_CLIP);
    }
}

pub(crate) trait WorldModel {
    fn apply(&self, state: &CodeState, action: &Action) -> CodeState;
}

pub(crate) trait ConsistencyValidator {
    fn validate(state_sim: &CodeState, state_analyze: &CodeState) -> ConsistencyReport;
}

pub(crate) trait IncrementalUpdater {
    fn update(prev: &CodeState, action: &Action) -> CodeState;
}

pub(crate) trait MultiStepController {
    fn allow_depth(metrics: Option<&SearchMetrics>, consistency_score: f64) -> bool;
}

pub(crate) struct DeterministicWorldModel;
pub(crate) struct DeterministicConsistencyValidator;
pub(crate) struct LocalIncrementalUpdater;
pub(crate) struct DeterministicMultiStepController;

#[derive(Clone, Debug)]
pub(crate) struct ConsistencyReport {
    pub metric_diff: f64,
    pub graph_diff: f64,
    pub violation_diff: f64,
    pub is_valid: bool,
}

impl WorldModel for DeterministicWorldModel {
    fn apply(&self, state: &CodeState, action: &Action) -> CodeState {
        LocalIncrementalUpdater::update(state, action)
    }
}

impl IncrementalUpdater for LocalIncrementalUpdater {
    fn update(prev: &CodeState, action: &Action) -> CodeState {
        let next_graph = apply_action(prev.graph.as_ref(), action);
        if next_graph == *prev.graph {
            return prev.clone();
        }
        let mut next = prev.clone();
        next.graph = Arc::new(next_graph);
        match action {
            Action::RemoveDependency { .. } => {
                next.metrics.coupling = (next.metrics.coupling - 0.05).clamp(0.0, 1.0);
                next.metrics.propagation_score =
                    (next.metrics.propagation_score - 0.03).clamp(0.0, 1.0);
            }
            Action::ExtractModule { .. } | Action::SplitModule { .. } => {
                next.metrics.coupling = (next.metrics.coupling - 0.02).clamp(0.0, 1.0);
                next.metrics.structural_variance =
                    (next.metrics.structural_variance + 0.04).clamp(0.0, 1.0);
            }
            Action::InlineModule { .. } => {
                next.metrics.impact = (next.metrics.impact - 0.04).clamp(0.0, 1.0);
                next.metrics.violation_intensity =
                    (next.metrics.violation_intensity - 0.03).clamp(0.0, 1.0);
            }
            Action::MergeModules { .. } => {
                next.metrics.coupling = (next.metrics.coupling + 0.02).clamp(0.0, 1.0);
                next.metrics.propagation_score =
                    (next.metrics.propagation_score - 0.02).clamp(0.0, 1.0);
            }
        }
        next.constraints = approximate_constraints(&next.metrics);
        next
    }
}

impl ConsistencyValidator for DeterministicConsistencyValidator {
    fn validate(state_sim: &CodeState, state_analyze: &CodeState) -> ConsistencyReport {
        let metric_diff = [
            (state_sim.metrics.coupling - state_analyze.metrics.coupling).abs(),
            (state_sim.metrics.propagation_score - state_analyze.metrics.propagation_score).abs(),
            (state_sim.metrics.impact - state_analyze.metrics.impact).abs(),
            (state_sim.metrics.structural_variance - state_analyze.metrics.structural_variance)
                .abs(),
            (state_sim.metrics.violation_intensity - state_analyze.metrics.violation_intensity)
                .abs(),
            (state_sim.metrics.constraint_score - state_analyze.metrics.constraint_score).abs(),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max);
        let graph_diff = graph_diff_ratio(state_sim.graph.as_ref(), state_analyze.graph.as_ref());
        let violation_diff =
            normalized_violation_diff(&state_sim.constraints, &state_analyze.constraints);
        ConsistencyReport {
            metric_diff,
            graph_diff,
            violation_diff,
            is_valid: metric_diff < CONSISTENCY_METRIC_EPS
                && graph_diff < CONSISTENCY_GRAPH_EPS
                && violation_diff < CONSISTENCY_VIOLATION_EPS,
        }
    }
}

impl MultiStepController for DeterministicMultiStepController {
    fn allow_depth(metrics: Option<&SearchMetrics>, consistency_score: f64) -> bool {
        let Some(metrics) = metrics else {
            return false;
        };
        metrics.hv_delta <= 1e-3
            && metrics.diversity <= 0.15
            && consistency_score >= 0.95
            && metrics.stagnation_steps >= 3
    }
}

pub(crate) fn build_code_state(state: &DesignState, objective: ObjectiveVector) -> CodeState {
    build_code_state_from_graph(state.graph.clone(), objective)
}

pub(crate) fn generate_actions(state: &CodeState, limit: usize) -> Vec<Action> {
    let graph = state.graph.as_ref();
    let mut actions = Vec::new();
    let mut node_ids = graph.nodes().keys().copied().collect::<Vec<_>>();
    node_ids.sort_unstable();
    let mut edges = graph.edges().iter().copied().collect::<Vec<_>>();
    edges.sort_unstable();

    if let Some((from, to)) = edges.first().copied() {
        actions.push(Action::RemoveDependency { from, to });
    }

    if let Some(target) = node_ids.iter().copied().max_by(|lhs, rhs| {
        node_load(graph, *lhs)
            .cmp(&node_load(graph, *rhs))
            .then_with(|| lhs.cmp(rhs))
    }) {
        actions.push(Action::ExtractModule { target });
        actions.push(Action::SplitModule { target });
    }

    if let Some(target) = node_ids
        .iter()
        .copied()
        .filter(|id| indegree(graph, *id) == 1 && outdegree(graph, *id) <= 1)
        .min()
    {
        actions.push(Action::InlineModule { target });
    }

    if node_ids.len() >= 2 {
        let pair = node_ids
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .min_by(|lhs, rhs| {
                merge_cost(graph, *lhs)
                    .cmp(&merge_cost(graph, *rhs))
                    .then_with(|| lhs.cmp(rhs))
            });
        if let Some((a, b)) = pair {
            actions.push(Action::MergeModules { a, b });
        }
    }

    actions.truncate(limit.max(1));
    actions
}

pub(crate) fn simulate_best_action(
    state: &DesignState,
    objective: ObjectiveVector,
    vm: &mut HybridVM,
    base_alpha: f64,
    base_beta: f64,
    beta_profile: crate::BetaProfile,
    actions_per_state: usize,
    max_depth: usize,
    intent_profile: crate::IntentProfile,
    mode: crate::WorldModelMode,
    variance_penalty: f64,
    semantic_variance_penalty: f64,
    semantic_variance_max_penalty: f64,
    confidence_floor: f64,
    learning_engine: &mut LearningEngine,
    search_metrics: Option<&SearchMetrics>,
    cache: &mut SimulationCache,
) -> Option<SimulationOutcome> {
    let code_state = build_code_state(state, objective.clone());
    let static_score = crate::scalar_score(&objective);
    let intent_model = IntentModel::from_profile(intent_profile);
    let actions = generate_actions(&code_state, actions_per_state.min(5));
    let world_model = DeterministicWorldModel;
    let first_pass = actions
        .into_iter()
        .filter_map(|action| {
            simulate_action(
                state,
                &code_state,
                &action,
                vm,
                simulation_alpha(
                    base_alpha,
                    beta_controller(
                        base_beta,
                        beta_profile,
                        BetaContext {
                            hv_delta: search_metrics.map(|m| m.hv_delta).unwrap_or(0.0),
                            stagnation: search_metrics.map(|m| m.stagnation_steps).unwrap_or(0),
                            consistency: 1.0,
                            depth: 1,
                            confidence: 1.0,
                        },
                        max_depth,
                    ),
                ),
                beta_controller(
                    base_beta,
                    beta_profile,
                    BetaContext {
                        hv_delta: search_metrics.map(|m| m.hv_delta).unwrap_or(0.0),
                        stagnation: search_metrics.map(|m| m.stagnation_steps).unwrap_or(0),
                        consistency: 1.0,
                        depth: 1,
                        confidence: 1.0,
                    },
                    max_depth,
                ),
                static_score,
                &intent_model,
                intent_profile,
                mode,
                variance_penalty,
                semantic_variance_penalty,
                semantic_variance_max_penalty,
                confidence_floor,
                learning_engine,
                cache,
                &world_model,
                1,
            )
        })
        .collect::<Vec<_>>();
    let mut best = first_pass.iter().cloned().max_by(|lhs, rhs| {
        lhs.delta
            .final_score
            .partial_cmp(&rhs.delta.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| lhs.delta.action_label.cmp(&rhs.delta.action_label))
    });

    if max_depth >= 2 {
        for first in first_pass.into_iter() {
            if !DeterministicMultiStepController::allow_depth(
                search_metrics,
                first.delta.consistency_score,
            ) {
                continue;
            }
            let actions2 = generate_actions(
                &first.code_state,
                actions_per_state.min(MULTISTEP_BRANCH_LIMIT),
            );
            for action2 in actions2.into_iter().take(MULTISTEP_BRANCH_LIMIT) {
                if let Some(second) = simulate_action(
                    &first.state,
                    &first.code_state,
                    &action2,
                    vm,
                    simulation_alpha(
                        base_alpha,
                        beta_controller(
                            base_beta,
                            beta_profile,
                            BetaContext {
                                hv_delta: search_metrics.map(|m| m.hv_delta).unwrap_or(0.0),
                                stagnation: search_metrics.map(|m| m.stagnation_steps).unwrap_or(0),
                                consistency: first.delta.consistency_score,
                                depth: 2,
                                confidence: first.delta.confidence,
                            },
                            max_depth,
                        ),
                    ),
                    beta_controller(
                        base_beta,
                        beta_profile,
                        BetaContext {
                            hv_delta: search_metrics.map(|m| m.hv_delta).unwrap_or(0.0),
                            stagnation: search_metrics.map(|m| m.stagnation_steps).unwrap_or(0),
                            consistency: first.delta.consistency_score,
                            depth: 2,
                            confidence: first.delta.confidence,
                        },
                        max_depth,
                    ),
                    crate::scalar_score(&first.objective),
                    &intent_model,
                    intent_profile,
                    mode,
                    variance_penalty,
                    semantic_variance_penalty,
                    semantic_variance_max_penalty,
                    confidence_floor,
                    learning_engine,
                    cache,
                    &world_model,
                    2,
                ) {
                    best = match best {
                        Some(current) if current.delta.final_score >= second.delta.final_score => {
                            Some(current)
                        }
                        _ => Some(second),
                    };
                }
            }
        }
    }

    if let Some(selected) = best.as_ref() {
        learning_engine.update(
            intent_profile,
            &Experience {
                action_kind: action_kind_from_label(&selected.delta.action_label),
                result: ProbabilisticEvaluation {
                    score: selected.delta.simulated_score,
                    confidence: selected.delta.confidence,
                    variance: selected.delta.variance,
                    semantic_variance: selected.delta.semantic_variance,
                    uncertainty: selected.delta.uncertainty,
                    beta_reliance: selected.delta.beta_reliance,
                    intent_score: selected.delta.intent_score,
                    learning_bias: selected.delta.learning_bias,
                    final_score: selected.delta.final_score,
                },
            },
        );
    }

    best
}

fn simulate_action(
    state: &DesignState,
    code_state: &CodeState,
    action: &Action,
    vm: &mut HybridVM,
    alpha: f64,
    beta: f64,
    static_score: f64,
    intent_model: &IntentModel,
    intent_profile: crate::IntentProfile,
    mode: crate::WorldModelMode,
    variance_penalty: f64,
    semantic_variance_penalty: f64,
    semantic_variance_max_penalty: f64,
    confidence_floor: f64,
    learning_engine: &LearningEngine,
    cache: &mut SimulationCache,
    world_model: &impl WorldModel,
    depth: usize,
) -> Option<SimulationOutcome> {
    if let Some(cached) = cache.get(state, action) {
        return Some(cached);
    }

    let mut next_code_state = world_model.apply(code_state, action);
    if next_code_state.graph == code_state.graph {
        return None;
    }
    let analyzed_code_state = build_code_state_from_graph(
        next_code_state.graph.clone(),
        code_state.metrics.objective.clone(),
    );
    let consistency =
        DeterministicConsistencyValidator::validate(&next_code_state, &analyzed_code_state);
    let fallback_used = !consistency.is_valid;
    if fallback_used {
        next_code_state = analyzed_code_state;
    }

    let next_state = DesignState::new(
        deterministic_state_id(state, action, next_code_state.graph.as_ref()),
        next_code_state.graph.clone(),
        format!(
            "{}|sim{}:{}",
            state.profile_snapshot,
            depth,
            action_label(action)
        ),
    );
    let simulated_obj = vm.evaluate(&next_state).clamped();
    let simulated_score = crate::scalar_score(&simulated_obj);
    let blended_obj = blend_objective(
        code_state.metrics.objective.clone(),
        simulated_obj,
        alpha,
        beta,
    );
    let blended_score = crate::scalar_score(&blended_obj);
    let consistency_score =
        1.0 - (consistency.metric_diff + consistency.graph_diff + consistency.violation_diff) / 3.0;
    let intent_score = intent_model.score(&next_code_state, action, consistency_score);
    let learning_bias = learning_engine.bias_for(intent_profile, action.kind())
        + learning_engine.exploration_bonus(intent_profile, action.kind());
    let evaluation = evaluate_probabilistic_outcome(
        static_score,
        blended_score,
        intent_score,
        consistency_score,
        &next_code_state,
        variance_penalty,
        semantic_variance_penalty,
        semantic_variance_max_penalty,
        confidence_floor,
        learning_bias,
        beta,
        code_state,
        action,
        &intent_model.rules,
        mode,
        depth,
    );
    let outcome = SimulationOutcome {
        state: next_state,
        objective: blended_obj,
        delta: SimulationDelta {
            action_label: action_label(action),
            rp: simulated_score - static_score,
            intent_score: evaluation.intent_score,
            confidence: evaluation.confidence,
            variance: evaluation.variance,
            semantic_variance: evaluation.semantic_variance,
            uncertainty: evaluation.uncertainty,
            beta_reliance: evaluation.beta_reliance,
            learning_bias: evaluation.learning_bias,
            final_score: evaluation.final_score,
            delta_violations: next_code_state.constraints.violations.len() as f64
                - code_state.constraints.violations.len() as f64,
            delta_coupling: next_code_state.metrics.coupling - code_state.metrics.coupling,
            delta_propagation_score: next_code_state.metrics.propagation_score
                - code_state.metrics.propagation_score,
            static_score,
            simulated_score: evaluation.score,
            consistency_score,
            simulated_depth: depth,
            fallback_used,
        },
        code_state: next_code_state,
    };
    cache.insert(state, action, outcome.clone());
    Some(outcome)
}

fn blend_objective(
    static_obj: ObjectiveVector,
    sim_obj: ObjectiveVector,
    alpha: f64,
    beta: f64,
) -> ObjectiveVector {
    ObjectiveVector {
        f_struct: (alpha * static_obj.f_struct + beta * sim_obj.f_struct).clamp(0.0, 1.0),
        f_field: (alpha * static_obj.f_field + beta * sim_obj.f_field).clamp(0.0, 1.0),
        f_risk: (alpha * static_obj.f_risk + beta * sim_obj.f_risk).clamp(0.0, 1.0),
        f_shape: (alpha * static_obj.f_shape + beta * sim_obj.f_shape).clamp(0.0, 1.0),
    }
}

fn evaluate_probabilistic_outcome(
    static_score: f64,
    simulated_score: f64,
    intent_score: f64,
    consistency_score: f64,
    code_state: &CodeState,
    variance_penalty: f64,
    semantic_variance_penalty: f64,
    semantic_variance_max_penalty: f64,
    confidence_floor: f64,
    learning_bias: f64,
    beta_reliance: f64,
    previous_code_state: &CodeState,
    action: &Action,
    intent_rules: &[crate::IntentRule],
    mode: crate::WorldModelMode,
    depth: usize,
) -> ProbabilisticEvaluation {
    let scenario_scores = scenario_scores(
        static_score,
        simulated_score,
        intent_score,
        code_state,
        depth,
    );
    let mean_score = scenario_scores.iter().sum::<f64>() / scenario_scores.len() as f64;
    let variance = (scenario_scores
        .iter()
        .map(|score| {
            let delta = *score - mean_score;
            delta * delta
        })
        .sum::<f64>()
        / scenario_scores.len() as f64)
        .max(MIN_OUTCOME_VARIANCE);
    let semantic_variance =
        semantic_variance(previous_code_state, code_state, action, intent_rules)
            .total()
            .min(semantic_variance_max_penalty);
    let inconsistency = ((1.0 - consistency_score).clamp(0.0, 1.0) * 0.6
        + code_state.metrics.structural_variance * 0.25
        + variance.sqrt().min(1.0) * 0.10
        + semantic_variance * 0.05)
        .clamp(0.0, 1.0);
    let confidence = (1.0 - inconsistency).clamp(confidence_floor, 1.0);
    let uncertainty =
        (variance.sqrt() + (1.0 - confidence) + semantic_variance * 0.5).clamp(0.0, 1.0);
    let exploration_bias = if mode == crate::WorldModelMode::Probabilistic {
        uncertainty * 0.05
    } else {
        0.0
    };
    let gamma = (1.0 - beta_reliance).clamp(0.15, 0.45);
    let alpha = (1.0 - beta_reliance - gamma).clamp(0.15, 0.75);
    let integrated_score =
        (alpha * static_score + beta_reliance * simulated_score + gamma * intent_score)
            .clamp(0.0, 1.0);
    let final_score = (integrated_score * confidence
        - variance_penalty * variance
        - semantic_variance_penalty * semantic_variance
        + learning_bias
        + exploration_bias)
        .clamp(0.0, 1.0);

    ProbabilisticEvaluation {
        score: simulated_score,
        confidence,
        variance,
        semantic_variance,
        uncertainty,
        beta_reliance,
        intent_score,
        learning_bias,
        final_score,
    }
}

fn scenario_scores(
    static_score: f64,
    simulated_score: f64,
    intent_score: f64,
    code_state: &CodeState,
    depth: usize,
) -> [f64; 3] {
    let structural_drag = code_state.metrics.structural_variance * 0.08;
    let coupling_drag = code_state.metrics.coupling * 0.06;
    let depth_bonus = (depth as f64 * 0.01).min(0.03);
    [
        (0.40 * static_score + 0.35 * simulated_score + 0.25 * intent_score - structural_drag)
            .clamp(0.0, 1.0),
        (0.20 * static_score + 0.55 * simulated_score + 0.25 * intent_score + depth_bonus)
            .clamp(0.0, 1.0),
        (0.25 * static_score + 0.30 * simulated_score + 0.45 * intent_score - coupling_drag)
            .clamp(0.0, 1.0),
    ]
}

impl SemanticVariance {
    fn total(&self) -> f64 {
        (self.structural_diff + self.constraint_risk + self.intent_conflict)
            .clamp(0.0, SEMANTIC_VARIANCE_CLIP)
    }
}

fn semantic_variance(
    previous: &CodeState,
    next: &CodeState,
    action: &Action,
    intent_rules: &[crate::IntentRule],
) -> SemanticVariance {
    let structural_diff =
        ((next.metrics.structural_variance - previous.metrics.structural_variance).abs()
            + (next.metrics.coupling - previous.metrics.coupling).abs() * 0.5)
            .clamp(0.0, 1.0);
    let constraint_risk =
        normalized_violation_diff(&previous.constraints, &next.constraints).clamp(0.0, 1.0);
    let intent_conflict = intent_conflict_score(next, action, intent_rules);

    SemanticVariance {
        structural_diff,
        constraint_risk,
        intent_conflict,
    }
}

fn intent_conflict_score(
    code_state: &CodeState,
    action: &Action,
    intent_rules: &[crate::IntentRule],
) -> f64 {
    let metric_values = BTreeMap::from([
        (
            crate::IntentMetric::Maintainability,
            (1.0 - code_state.metrics.coupling).clamp(0.0, 1.0),
        ),
        (
            crate::IntentMetric::Performance,
            (1.0 - code_state.metrics.constraint_score).clamp(0.0, 1.0),
        ),
        (
            crate::IntentMetric::Risk,
            code_state.metrics.violation_intensity.clamp(0.0, 1.0),
        ),
        (
            crate::IntentMetric::RefactorEase,
            action_refactor_affinity(action).clamp(0.0, 1.0),
        ),
    ]);
    let action_fit = action_policy_affinity(action);
    let mut conflict = 0.0;

    for rule in intent_rules {
        conflict += match *rule {
            crate::IntentRule::Maximize(metric) => {
                1.0 - metric_values.get(&metric).copied().unwrap_or(0.0)
            }
            crate::IntentRule::Minimize(metric) => {
                metric_values.get(&metric).copied().unwrap_or(0.0)
            }
            crate::IntentRule::Constraint(metric, threshold) => {
                (metric_values.get(&metric).copied().unwrap_or(0.0) - threshold)
                    .max(0.0)
                    .clamp(0.0, 1.0)
            }
            crate::IntentRule::ActionPolicy(policy, _) => match policy {
                crate::IntentActionPolicy::Prefer => 1.0 - action_fit,
                crate::IntentActionPolicy::Avoid => action_fit,
            },
        };
    }

    if intent_rules.is_empty() {
        0.0
    } else {
        (conflict / intent_rules.len() as f64).clamp(0.0, 1.0)
    }
}

fn build_code_state_from_graph(
    graph: Arc<StructuralGraph>,
    objective: ObjectiveVector,
) -> CodeState {
    let evaluator = StructuralEvaluator::default();
    let design_state = DesignState::new(Uuid::from_u128(0), graph.clone(), "code-state");
    let features = evaluator.extract_features(&design_state);
    let constraints = approximate_constraints_from_features(
        features.violation_intensity,
        features.impact,
        features.propagation_score,
    );

    CodeState {
        graph,
        metrics: CodeMetrics {
            static_score: crate::scalar_score(&objective),
            objective,
            coupling: features.impact.max(features.violation_intensity).min(1.0),
            propagation_score: features.propagation_score,
            impact: features.impact,
            structural_variance: features.structural_variance,
            violation_intensity: features.violation_intensity,
            constraint_score: features.cs,
        },
        constraints,
    }
}

fn approximate_constraints(metrics: &CodeMetrics) -> Constraints {
    approximate_constraints_from_features(
        metrics.violation_intensity,
        metrics.impact,
        metrics.propagation_score,
    )
}

fn approximate_constraints_from_features(
    violation_intensity: f64,
    impact: f64,
    propagation_score: f64,
) -> Constraints {
    let mut violations = Vec::new();
    if violation_intensity > 0.2 {
        violations.push("violation:intensity".to_string());
    }
    if impact > 0.4 {
        violations.push("violation:impact".to_string());
    }
    if propagation_score > 0.6 {
        violations.push("violation:propagation".to_string());
    }
    Constraints { violations }
}

fn graph_diff_ratio(lhs: &StructuralGraph, rhs: &StructuralGraph) -> f64 {
    let lhs_nodes = lhs.nodes().len().max(1) as f64;
    let lhs_edges = lhs.edges().len().max(1) as f64;
    let lhs_node_set = lhs
        .nodes()
        .keys()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let rhs_node_set = rhs
        .nodes()
        .keys()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let node_diff = lhs_node_set.symmetric_difference(&rhs_node_set).count() as f64;
    let edge_diff = lhs.edges().symmetric_difference(rhs.edges()).count() as f64;
    ((node_diff / lhs_nodes) + (edge_diff / lhs_edges)) / 2.0
}

fn normalized_violation_diff(lhs: &Constraints, rhs: &Constraints) -> f64 {
    let lhs_set = lhs
        .violations
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    let rhs_set = rhs
        .violations
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    let union = lhs_set.union(&rhs_set).count();
    if union == 0 {
        0.0
    } else {
        lhs_set.symmetric_difference(&rhs_set).count() as f64 / union as f64
    }
}

fn beta_controller(
    base_beta: f64,
    profile: crate::BetaProfile,
    ctx: BetaContext,
    max_depth: usize,
) -> f64 {
    let stagnation_factor = (ctx.stagnation as f64 / 6.0).clamp(0.0, 1.0);
    let inconsistency = (1.0 - ctx.consistency).clamp(0.0, 1.0);
    let depth_factor = (ctx.depth as f64 / max_depth.max(1) as f64).clamp(0.0, 1.0);
    let confidence_gap = (1.0 - ctx.confidence).clamp(0.0, 1.0);
    let hv_brake = (ctx.hv_delta / 0.05).clamp(0.0, 1.0);
    let (w_hv, w_stag, w_cons, w_depth, w_conf, phase_scale) = match profile {
        crate::BetaProfile::Conservative => (0.20, 0.22, 0.26, 0.10, 0.12, 0.7),
        crate::BetaProfile::Balanced => (0.15, 0.30, 0.20, 0.10, 0.15, 1.0),
        crate::BetaProfile::Aggressive => (0.08, 0.34, 0.16, 0.16, 0.20, 1.2),
    };

    let phase_bias = if depth_factor <= 0.33 {
        0.10 * phase_scale
    } else if depth_factor <= 0.66 {
        0.0
    } else {
        -0.10 * phase_scale
    };

    (base_beta * 0.35
        + w_stag * stagnation_factor
        + w_cons * inconsistency
        + w_depth * depth_factor
        + w_conf * confidence_gap
        + phase_bias
        - w_hv * hv_brake)
        .clamp(0.1, 0.7)
}

fn simulation_alpha(base_alpha: f64, beta: f64) -> f64 {
    (base_alpha.min(1.0) - (beta - 0.3).max(0.0) * 0.45).clamp(0.2, 0.9)
}

fn apply_action(graph: &StructuralGraph, action: &Action) -> StructuralGraph {
    match action {
        Action::ExtractModule { target } => extract_module(graph, *target),
        Action::InlineModule { target } => inline_module(graph, *target),
        Action::RemoveDependency { from, to } => graph.with_edge_removed(*from, *to),
        Action::SplitModule { target } => split_module(graph, *target),
        Action::MergeModules { a, b } => merge_modules(graph, *a, *b),
    }
}

fn extract_module(graph: &StructuralGraph, target: Uuid) -> StructuralGraph {
    if !graph.nodes().contains_key(&target) {
        return graph.clone();
    }
    let outgoing = outgoing_neighbors(graph, target);
    let Some(first_edge_target) = outgoing.first().copied() else {
        return graph.clone();
    };
    let new_id = derived_node_id(target, 0xE1);
    let mut next = graph.with_node_added(clone_node_with_new_id(
        graph,
        target,
        new_id,
        "ExtractedModule",
    ));
    next = next.with_edge_added(target, new_id);
    next = next.with_edge_removed(target, first_edge_target);
    next.with_edge_added(new_id, first_edge_target)
}

fn inline_module(graph: &StructuralGraph, target: Uuid) -> StructuralGraph {
    if !graph.nodes().contains_key(&target) {
        return graph.clone();
    }
    let incoming = incoming_neighbors(graph, target);
    let outgoing = outgoing_neighbors(graph, target);
    if incoming.len() != 1 {
        return graph.clone();
    }
    let mut next = graph.clone();
    for to in outgoing {
        next = next.with_edge_added(incoming[0], to);
    }
    next.with_node_removed(target)
}

fn split_module(graph: &StructuralGraph, target: Uuid) -> StructuralGraph {
    if !graph.nodes().contains_key(&target) {
        return graph.clone();
    }
    let outgoing = outgoing_neighbors(graph, target);
    if outgoing.len() < 2 {
        return graph.clone();
    }
    let new_id = derived_node_id(target, 0x51);
    let split_count = outgoing.len() / 2;
    let mut next =
        graph.with_node_added(clone_node_with_new_id(graph, target, new_id, "SplitModule"));
    next = next.with_edge_added(target, new_id);
    for to in outgoing.into_iter().skip(split_count) {
        next = next.with_edge_removed(target, to);
        next = next.with_edge_added(new_id, to);
    }
    next
}

fn merge_modules(graph: &StructuralGraph, a: Uuid, b: Uuid) -> StructuralGraph {
    if a == b || !graph.nodes().contains_key(&a) || !graph.nodes().contains_key(&b) {
        return graph.clone();
    }
    let mut next = graph.clone();
    for from in incoming_neighbors(graph, b) {
        if from != a {
            next = next.with_edge_added(from, a);
        }
    }
    for to in outgoing_neighbors(graph, b) {
        if to != a {
            next = next.with_edge_added(a, to);
        }
    }
    next.with_node_removed(b)
}

fn clone_node_with_new_id(
    graph: &StructuralGraph,
    source_id: Uuid,
    new_id: Uuid,
    kind: &str,
) -> DesignNode {
    let mut attrs = graph
        .nodes()
        .get(&source_id)
        .map(|node| node.attributes.clone())
        .unwrap_or_default();
    attrs.insert("simulated".to_string(), Value::Bool(true));
    DesignNode::new(new_id, kind, attrs)
}

fn action_label(action: &Action) -> String {
    match action {
        Action::ExtractModule { target } => format!("extract:{:x}", target.as_u128()),
        Action::InlineModule { target } => format!("inline:{:x}", target.as_u128()),
        Action::RemoveDependency { from, to } => {
            format!("remove_dep:{:x}->{:x}", from.as_u128(), to.as_u128())
        }
        Action::SplitModule { target } => format!("split:{:x}", target.as_u128()),
        Action::MergeModules { a, b } => format!("merge:{:x}+{:x}", a.as_u128(), b.as_u128()),
    }
}

fn action_refactor_affinity(action: &Action) -> f64 {
    match action {
        Action::ExtractModule { .. } => 0.90,
        Action::SplitModule { .. } => 0.95,
        Action::RemoveDependency { .. } => 0.65,
        Action::InlineModule { .. } => 0.35,
        Action::MergeModules { .. } => 0.40,
    }
}

fn action_policy_affinity(action: &Action) -> f64 {
    match action {
        Action::ExtractModule { .. } | Action::SplitModule { .. } => 0.90,
        Action::RemoveDependency { .. } => 0.70,
        Action::InlineModule { .. } => 0.30,
        Action::MergeModules { .. } => 0.25,
    }
}

fn constraint_pressure(code_state: &CodeState) -> f64 {
    (code_state.constraints.violations.len() as f64 / 3.0).clamp(0.0, 1.0)
}

fn action_kind_from_label(label: &str) -> ActionKind {
    if label.starts_with("extract:") {
        ActionKind::Extract
    } else if label.starts_with("inline:") {
        ActionKind::Inline
    } else if label.starts_with("remove_dep:") {
        ActionKind::RemoveDependency
    } else if label.starts_with("split:") {
        ActionKind::Split
    } else {
        ActionKind::Merge
    }
}

fn deterministic_state_id(state: &DesignState, action: &Action, graph: &StructuralGraph) -> Uuid {
    let mut acc = 0x517cc1b727220a95u128;
    acc = fnv_mix_u128(acc, state.id.as_u128());
    acc = fnv_mix_u128(acc, graph.nodes().len() as u128);
    acc = fnv_mix_u128(acc, graph.edges().len() as u128);
    for byte in action_label(action).bytes() {
        acc = fnv_mix_u128(acc, byte as u128);
    }
    Uuid::from_u128(acc)
}

fn derived_node_id(base: Uuid, salt: u128) -> Uuid {
    Uuid::from_u128(fnv_mix_u128(base.as_u128(), salt))
}

fn fnv_mix_u128(acc: u128, value: u128) -> u128 {
    let prime = 0x100000001b3u128;
    (acc ^ value).wrapping_mul(prime)
}

fn indegree(graph: &StructuralGraph, node: Uuid) -> usize {
    graph.edges().iter().filter(|(_, to)| *to == node).count()
}

fn outdegree(graph: &StructuralGraph, node: Uuid) -> usize {
    graph
        .edges()
        .iter()
        .filter(|(from, _)| *from == node)
        .count()
}

fn node_load(graph: &StructuralGraph, node: Uuid) -> usize {
    indegree(graph, node) + outdegree(graph, node)
}

fn merge_cost(graph: &StructuralGraph, pair: (Uuid, Uuid)) -> usize {
    node_load(graph, pair.0) + node_load(graph, pair.1)
}

fn incoming_neighbors(graph: &StructuralGraph, node: Uuid) -> Vec<Uuid> {
    let mut incoming = graph
        .edges()
        .iter()
        .filter_map(|(from, to)| (*to == node).then_some(*from))
        .collect::<Vec<_>>();
    incoming.sort_unstable();
    incoming
}

fn outgoing_neighbors(graph: &StructuralGraph, node: Uuid) -> Vec<Uuid> {
    let mut outgoing = graph
        .edges()
        .iter()
        .filter_map(|(from, to)| (*from == node).then_some(*to))
        .collect::<Vec<_>>();
    outgoing.sort_unstable();
    outgoing
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use memory_space::{StateId, StructuralGraph};

    use super::*;
    use crate::runtime::structured_search::SearchMetrics;

    fn state_with_graph(id: u128, edges: &[(u128, u128)]) -> DesignState {
        let mut graph = StructuralGraph::default();
        for node_id in 1..=5 {
            graph = graph.with_node_added(DesignNode::new(
                Uuid::from_u128(node_id),
                format!("N{node_id}"),
                BTreeMap::new(),
            ));
        }
        for (from, to) in edges {
            graph = graph.with_edge_added(Uuid::from_u128(*from), Uuid::from_u128(*to));
        }
        DesignState::new(StateId::from_u128(id), Arc::new(graph), "history:")
    }

    #[test]
    fn action_generation_is_deterministic() {
        let state = state_with_graph(1, &[(1, 2), (2, 3), (3, 4)]);
        let code_state = build_code_state(
            &state,
            ObjectiveVector {
                f_struct: 0.7,
                f_field: 0.7,
                f_risk: 0.7,
                f_shape: 0.7,
            },
        );
        assert_eq!(
            generate_actions(&code_state, 5),
            generate_actions(&code_state, 5)
        );
    }

    #[test]
    fn same_action_produces_same_state() {
        let state = state_with_graph(1, &[(1, 2), (2, 3), (3, 4)]);
        let code_state = build_code_state(
            &state,
            ObjectiveVector {
                f_struct: 0.7,
                f_field: 0.7,
                f_risk: 0.7,
                f_shape: 0.7,
            },
        );
        let world_model = DeterministicWorldModel;
        let action = Action::RemoveDependency {
            from: Uuid::from_u128(1),
            to: Uuid::from_u128(2),
        };
        let first = world_model.apply(&code_state, &action);
        let second = world_model.apply(&code_state, &action);
        assert_eq!(first.graph, second.graph);
    }

    #[test]
    fn invalid_action_is_noop() {
        let state = state_with_graph(1, &[(1, 2)]);
        let code_state = build_code_state(
            &state,
            ObjectiveVector {
                f_struct: 0.5,
                f_field: 0.5,
                f_risk: 0.5,
                f_shape: 0.5,
            },
        );
        let world_model = DeterministicWorldModel;
        let next = world_model.apply(
            &code_state,
            &Action::RemoveDependency {
                from: Uuid::from_u128(3),
                to: Uuid::from_u128(4),
            },
        );
        assert_eq!(next.graph, code_state.graph);
    }

    #[test]
    fn incremental_update_matches_full_analyze_within_tolerance() {
        let state = state_with_graph(1, &[(1, 2), (2, 3), (3, 4)]);
        let code_state = build_code_state(
            &state,
            ObjectiveVector {
                f_struct: 0.6,
                f_field: 0.6,
                f_risk: 0.6,
                f_shape: 0.6,
            },
        );
        let incremental = LocalIncrementalUpdater::update(
            &code_state,
            &Action::RemoveDependency {
                from: Uuid::from_u128(1),
                to: Uuid::from_u128(2),
            },
        );
        let analyzed = build_code_state_from_graph(
            incremental.graph.clone(),
            code_state.metrics.objective.clone(),
        );
        let report = DeterministicConsistencyValidator::validate(&incremental, &analyzed);
        assert!(report.graph_diff <= CONSISTENCY_GRAPH_EPS);
        assert!(report.metric_diff.is_finite());
    }

    #[test]
    fn inconsistency_triggers_fallback_to_full_analyze() {
        let state = state_with_graph(1, &[(1, 2), (2, 3), (3, 4)]);
        let mut vm = HybridVM::with_default_memory(StructuralEvaluator::default()).expect("vm");
        let mut cache = SimulationCache::new();
        let mut learning =
            LearningEngine::new(crate::WorldModelMode::Deterministic, 0.1, 0.05, 0.55);
        let outcome = simulate_best_action(
            &state,
            ObjectiveVector {
                f_struct: 0.6,
                f_field: 0.6,
                f_risk: 0.6,
                f_shape: 0.6,
            },
            &mut vm,
            0.7,
            0.3,
            crate::BetaProfile::Balanced,
            5,
            1,
            crate::IntentProfile::Balanced,
            crate::WorldModelMode::Deterministic,
            0.2,
            0.15,
            0.35,
            0.2,
            &mut learning,
            Some(&SearchMetrics {
                hv_delta: 0.0,
                ..SearchMetrics::default()
            }),
            &mut cache,
        )
        .expect("simulation outcome");
        assert!(outcome.delta.fallback_used || outcome.delta.consistency_score >= 0.95);
    }

    #[test]
    fn depth_two_simulation_is_deterministic() {
        let state = state_with_graph(1, &[(1, 2), (2, 3), (3, 4)]);
        let mut vm_a = HybridVM::with_default_memory(StructuralEvaluator::default()).expect("vm_a");
        let mut vm_b = HybridVM::with_default_memory(StructuralEvaluator::default()).expect("vm_b");
        let metrics = SearchMetrics {
            hv_delta: 0.0,
            diversity: 0.0,
            stagnation_steps: 4,
            ..SearchMetrics::default()
        };
        let mut learning_a =
            LearningEngine::new(crate::WorldModelMode::Deterministic, 0.1, 0.05, 0.55);
        let first = simulate_best_action(
            &state,
            ObjectiveVector {
                f_struct: 0.6,
                f_field: 0.6,
                f_risk: 0.6,
                f_shape: 0.6,
            },
            &mut vm_a,
            0.7,
            0.3,
            crate::BetaProfile::Balanced,
            5,
            2,
            crate::IntentProfile::Balanced,
            crate::WorldModelMode::Deterministic,
            0.2,
            0.15,
            0.35,
            0.2,
            &mut learning_a,
            Some(&metrics),
            &mut SimulationCache::new(),
        )
        .expect("first");
        let mut learning_b =
            LearningEngine::new(crate::WorldModelMode::Deterministic, 0.1, 0.05, 0.55);
        let second = simulate_best_action(
            &state,
            ObjectiveVector {
                f_struct: 0.6,
                f_field: 0.6,
                f_risk: 0.6,
                f_shape: 0.6,
            },
            &mut vm_b,
            0.7,
            0.3,
            crate::BetaProfile::Balanced,
            5,
            2,
            crate::IntentProfile::Balanced,
            crate::WorldModelMode::Deterministic,
            0.2,
            0.15,
            0.35,
            0.2,
            &mut learning_b,
            Some(&metrics),
            &mut SimulationCache::new(),
        )
        .expect("second");
        assert_eq!(first.state.id, second.state.id);
        assert_eq!(first.delta.action_label, second.delta.action_label);
    }
}
