use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chm::Chm;
use core_types::{stability_index as core_stability_index, ObjectiveVector, P_INFER_ALPHA, P_INFER_BETA, P_INFER_GAMMA};
use evaluator::{Evaluator, StructuralEvaluator};
use field_engine::{resonance_score, FieldEngine, FieldVector, NodeCategory, TargetField};
use memory_space::{DesignNode, DesignState, StateId, StructuralGraph, Uuid, Value};
use profile::PreferenceProfile;
use shm::{DesignRule, EffectVector, RuleCategory, RuleId, Shm, Transformation};

#[derive(Clone, Debug, PartialEq)]
pub struct ParetoFront {
    pub states: Vec<(StateId, ObjectiveVector)>,
}

impl ParetoFront {
    pub fn new() -> Self {
        Self { states: Vec::new() }
    }

    pub fn insert(&mut self, state_id: StateId, obj: ObjectiveVector) {
        if self
            .states
            .iter()
            .any(|(_, existing)| dominates(existing, &obj))
        {
            return;
        }

        self.states.retain(|(_, existing)| !dominates(&obj, existing));

        if let Some(existing) = self
            .states
            .iter_mut()
            .find(|(existing_id, _)| *existing_id == state_id)
        {
            existing.1 = obj;
        } else {
            self.states.push((state_id, obj));
        }
    }

    pub fn get_front(&self) -> Vec<StateId> {
        self.states.iter().map(|(id, _)| *id).collect()
    }
}

pub fn dominates(a: &ObjectiveVector, b: &ObjectiveVector) -> bool {
    let all_ge = a.f_struct >= b.f_struct
        && a.f_field >= b.f_field
        && a.f_risk >= b.f_risk
        && a.f_cost >= b.f_cost;
    let one_gt = a.f_struct > b.f_struct
        || a.f_field > b.f_field
        || a.f_risk > b.f_risk
        || a.f_cost > b.f_cost;

    all_ge && one_gt
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub max_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    Auto,
    Manual,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DepthFront {
    pub depth: usize,
    pub state_ids: Vec<StateId>,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub final_frontier: Vec<DesignState>,
    pub depth_fronts: Vec<DepthFront>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroOperator {
    pub id: Uuid,
    pub steps: Vec<Transformation>,
    pub max_activations: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileUpdateType {
    TypeAExplicit,
    TypeBStructural,
    TypeCStatistical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phase45Log {
    pub depth: usize,
    pub k: usize,
    pub lambda_old: f64,
    pub lambda_new: f64,
    pub delta_lambda: f64,
    pub density: f64,
    pub a_density: f64,
    pub e_ref: f64,
    pub conf_chm: f64,
    pub tau: f64,
    pub tau_prime: f64,
    pub stability_index: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceRow {
    pub depth: usize,
    pub lambda: f32,
    pub delta_lambda: f32,
    pub tau_prime: f32,
    pub conf_chm: f32,
    pub density: f32,
    pub k: usize,
    pub h_profile: f32,
    pub pareto_size: usize,
    pub diversity: f32,
    pub resonance_avg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceRunConfig {
    pub depth: usize,
    pub beam: usize,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phase45Controller {
    lambda: f64,
    k: usize,
    tau: f64,
    eta: f64,
    gain: f64,
    cooldown_depths: usize,
    next_allowed_update_depth: usize,
}

impl Phase45Controller {
    pub fn new(initial_lambda: f64) -> Self {
        Self {
            lambda: initial_lambda.clamp(0.0, 1.0),
            k: 3,
            tau: 0.2,
            eta: 0.2,
            gain: 0.9,
            cooldown_depths: 2,
            next_allowed_update_depth: 0,
        }
    }

    pub fn lambda(&self) -> f64 {
        self.lambda
    }

    pub fn k(&self) -> usize {
        self.k
    }

    pub fn on_profile_update(&mut self, depth: usize, stability_index: f64, kind: ProfileUpdateType) {
        let priority = match kind {
            ProfileUpdateType::TypeAExplicit => 3,
            ProfileUpdateType::TypeBStructural => 2,
            ProfileUpdateType::TypeCStatistical => 1,
        };
        if depth < self.next_allowed_update_depth && priority < 3 {
            return;
        }

        self.k = select_k_with_hysteresis(self.k, stability_index);
        self.next_allowed_update_depth = depth + self.cooldown_depths;
    }

    pub fn update_depth(
        &mut self,
        depth: usize,
        conflict_k: f64,
        align_k: f64,
        n_edge_obs: usize,
        category_count: usize,
        stability_index: f64,
    ) -> Phase45Log {
        let lambda_old = self.lambda;
        let g_eff = self.gain / (self.k as f64).sqrt();
        let raw_delta = g_eff * (conflict_k - align_k);
        let bounded_delta = raw_delta.clamp(-0.05, 0.05);
        let smoothed_delta = self.eta * bounded_delta;
        self.lambda = (self.lambda + smoothed_delta).clamp(0.0, 1.0);

        let density = chm_density(n_edge_obs, category_count);
        let a_density = 2.0 + (6.0 - 2.0) * density;
        let e_ref = a_density * category_count as f64;
        let conf_chm = if e_ref <= f64::EPSILON {
            0.0
        } else {
            (n_edge_obs as f64 / e_ref).clamp(0.0, 1.0)
        };

        let h = profile_modulation(stability_index);
        let tau_prime_raw = self.tau * (0.1 + 0.6 * conf_chm * conf_chm) * h;
        let tau_prime = tau_prime_raw.clamp(0.1 * self.tau, 0.7 * self.tau);

        Phase45Log {
            depth,
            k: self.k,
            lambda_old,
            lambda_new: self.lambda,
            delta_lambda: self.lambda - lambda_old,
            density,
            a_density,
            e_ref,
            conf_chm,
            tau: self.tau,
            tau_prime,
            stability_index,
        }
    }
}

pub fn generate_trace(config: TraceRunConfig) -> Vec<TraceRow> {
    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, config.seed);
    let field = FieldEngine::new(256);
    let evaluator = SystemEvaluator::with_base(&chm, &field, StructuralEvaluator::default());

    let mut controller = Phase45Controller::new(0.5);
    let mut profile = PreferenceProfile {
        struct_weight: 0.25,
        field_weight: 0.25,
        risk_weight: 0.25,
        cost_weight: 0.25,
    };
    let mut frontier = vec![trace_initial_state(config.seed)];
    let mut rows = Vec::with_capacity(config.depth);

    let n_edge_obs = chm.rule_graph.values().map(|v| v.len()).sum::<usize>();
    let mut conflict_hist = Vec::new();
    let mut align_hist = Vec::new();

    for depth in 1..=config.depth {
        controller.on_profile_update(depth, 0.25, ProfileUpdateType::TypeCStatistical);

        let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
        for state in &frontier {
            for rule in shm.applicable_rules(state) {
                let new_state = apply_atomic(rule, state);
                let obj = evaluator.evaluate(&new_state);
                candidates.push((new_state, obj));
            }
        }

        if candidates.is_empty() {
            rows.push(TraceRow {
                depth,
                lambda: controller.lambda() as f32,
                delta_lambda: 0.0,
                tau_prime: 0.0,
                conf_chm: 0.0,
                density: 0.0,
                k: controller.k(),
                h_profile: profile_modulation(0.25) as f32,
                pareto_size: 0,
                diversity: 0.0,
                resonance_avg: 0.0,
            });
            continue;
        }

        let normalized = normalize_by_depth(candidates);
        let mut pareto = ParetoFront::new();
        for (state, obj) in &normalized {
            pareto.insert(state.id, obj.clone());
        }

        let front_set: BTreeSet<Uuid> = pareto.get_front().into_iter().collect();
        let mut front: Vec<(DesignState, ObjectiveVector)> = normalized
            .into_iter()
            .filter(|(s, _)| front_set.contains(&s.id))
            .collect();

        front.sort_by(|(ls, lo), (rs, ro)| {
            scalar_score(ro)
                .partial_cmp(&scalar_score(lo))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ls.id.cmp(&rs.id))
        });
        front.dedup_by(|a, b| a.0.id == b.0.id);

        let target_field = build_target_field(&field, &shm, &front[0].0, controller.lambda());
        let resonance_avg = front.iter().map(|(_, o)| o.f_field).sum::<f64>() / front.len() as f64;

        let conflict_raw = front
            .iter()
            .map(|(_, o)| (1.0 - o.f_risk + 1.0 - o.f_cost) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        let align_raw = front
            .iter()
            .map(|(_, o)| {
                let r = resonance_score(&field.aggregate_state(&front[0].0), &target_field);
                (o.f_struct + r) * 0.5
            })
            .sum::<f64>()
            / front.len() as f64;

        conflict_hist.push(conflict_raw);
        align_hist.push(align_raw);
        let k = controller.k().max(1);
        let conflict_k = moving_average_tail(&conflict_hist, k);
        let align_k = moving_average_tail(&align_hist, k);

        let log = controller.update_depth(
            depth,
            conflict_k,
            align_k,
            n_edge_obs,
            10,
            stability_index(0.25, 0.25, 0.0, 0.0),
        );

        let diversity = variance(&front.iter().map(|(_, o)| scalar_score(o)).collect::<Vec<_>>());
        rows.push(TraceRow {
            depth,
            lambda: log.lambda_new as f32,
            delta_lambda: log.delta_lambda as f32,
            tau_prime: log.tau_prime as f32,
            conf_chm: log.conf_chm as f32,
            density: log.density as f32,
            k: log.k,
            h_profile: profile_modulation(log.stability_index) as f32,
            pareto_size: front.len(),
            diversity: diversity as f32,
            resonance_avg: resonance_avg as f32,
        });

        frontier = front
            .into_iter()
            .take(config.beam.max(1))
            .map(|(s, _)| s)
            .collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(config.seed)];
        }

        profile = p_inferred(&profile, &profile, &profile, &profile);
    }

    rows
}

pub struct BeamSearch<'a> {
    pub shm: &'a Shm,
    pub chm: &'a Chm,
    pub evaluator: &'a dyn Evaluator,
    pub config: SearchConfig,
}

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
                let rules = self.shm.applicable_rules(state);
                for rule in rules {
                    let new_state = apply_atomic(rule, state);
                    let obj = self.evaluator.evaluate(&new_state);
                    candidates.push((new_state, obj));
                }
            }

            if candidates.is_empty() {
                break;
            }

            let normalized = normalize_by_depth(candidates);

            let mut pareto = ParetoFront::new();
            for (state, obj) in &normalized {
                pareto.insert(state.id, obj.clone());
            }

            let front_ids = pareto.get_front();
            let mut front_map: BTreeMap<StateId, (DesignState, ObjectiveVector)> = BTreeMap::new();
            for (state, obj) in normalized {
                if front_ids.binary_search(&state.id).is_ok() {
                    front_map.entry(state.id).or_insert((state, obj));
                }
            }

            let mut front_states: Vec<(DesignState, ObjectiveVector)> = front_map.into_values().collect();
            front_states.sort_by(|(left_state, left_obj), (right_state, right_obj)| {
                let ls = scalar_score(left_obj);
                let rs = scalar_score(right_obj);
                rs.partial_cmp(&ls)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left_state.id.cmp(&right_state.id))
            });

            frontier = front_states
                .into_iter()
                .take(self.config.beam_width)
                .map(|(state, _)| state)
                .collect();

            let ids = frontier.iter().map(|state| state.id).collect::<Vec<_>>();
            all_depths.push(DepthFront {
                depth: depth + 1,
                state_ids: ids,
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

pub fn scalar_score(obj: &ObjectiveVector) -> f64 {
    0.4 * obj.f_struct + 0.2 * obj.f_field + 0.2 * obj.f_risk + 0.2 * obj.f_cost
}

pub struct SystemEvaluator<'a> {
    base: StructuralEvaluator,
    chm: &'a Chm,
    field: &'a FieldEngine,
    target: TargetField,
}

impl<'a> SystemEvaluator<'a> {
    pub fn with_base(chm: &'a Chm, field: &'a FieldEngine, base: StructuralEvaluator) -> Self {
        Self {
            base,
            chm,
            field,
            target: TargetField::fixed(field.dimensions()),
        }
    }
}

impl Evaluator for SystemEvaluator<'_> {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        let mut obj = self.base.evaluate(state);
        let projection = self.field.aggregate_state(state);
        obj.f_field = resonance_score(&projection, &self.target);
        obj.f_risk = risk_score_from_chm(state, self.chm);
        obj.clamped()
    }
}

pub fn apply_atomic(rule: &DesignRule, state: &DesignState) -> DesignState {
    let graph = &state.graph;
    let next_graph = match rule.transformation {
        Transformation::AddNode => apply_add_node(graph, rule),
        Transformation::RemoveNode => apply_remove_node(graph),
        Transformation::ModifyAttribute => apply_modify_attribute(graph, rule),
        Transformation::AddConstraint => apply_add_constraint(graph, rule),
        Transformation::RewireDependency => apply_rewire_dependency(graph),
    };

    let next_snapshot = append_rule_history(&state.profile_snapshot, rule.id);
    let next_id = deterministic_state_id(
        state,
        rule,
        &next_snapshot,
        next_graph.nodes().len(),
        next_graph.edges().len(),
    );

    DesignState::new(next_id, Arc::new(next_graph), next_snapshot)
}

pub fn apply_macro(op: &MacroOperator, state: &DesignState) -> DesignState {
    let mut current = state.clone();
    for (idx, step) in op.steps.iter().take(op.max_activations).enumerate() {
        let rule = DesignRule {
            id: deterministic_uuid(op.id.as_u128(), idx as u128 + 1, 0xAA),
            category: RuleCategory::Refactor,
            priority: 0.5,
            precondition: |_| true,
            transformation: step.clone(),
            expected_effect: EffectVector {
                delta_struct: 0.0,
                delta_field: 0.0,
                delta_risk: 0.0,
                delta_cost: 0.0,
            },
        };
        current = apply_atomic(&rule, &current);
    }
    current
}

fn apply_add_node(graph: &StructuralGraph, rule: &DesignRule) -> StructuralGraph {
    let mut next = graph.clone();
    let node_id = deterministic_uuid(rule.id.as_u128(), graph.nodes().len() as u128 + 1, 0xA1);

    let mut attrs = BTreeMap::new();
    attrs.insert(format!("generated_by_{}", rule.id.as_u128()), Value::Bool(true));

    let node = DesignNode::new(node_id, "GeneratedNode", attrs);
    next = next.with_node_added(node);

    if let Some(existing_id) = sorted_node_ids(graph).first().copied() {
        next = next.with_edge_added(existing_id, node_id);
    }

    next
}

fn apply_remove_node(graph: &StructuralGraph) -> StructuralGraph {
    let mut ids = sorted_node_ids(graph);
    if let Some(last) = ids.pop() {
        graph.with_node_removed(last)
    } else {
        graph.clone()
    }
}

fn apply_modify_attribute(graph: &StructuralGraph, rule: &DesignRule) -> StructuralGraph {
    let Some(target_id) = sorted_node_ids(graph).first().copied() else {
        return graph.clone();
    };

    let mut next = graph.clone();
    let Some(original) = graph.nodes().get(&target_id).cloned() else {
        return graph.clone();
    };

    let mut attrs = original.attributes;
    attrs.insert(
        format!("mod_{}", rule.id.as_u128()),
        Value::Int((rule.id.as_u128() & 0xFFFF) as i64),
    );

    next = next.with_node_removed(target_id);
    next.with_node_added(DesignNode::new(target_id, original.kind, attrs))
}

fn apply_add_constraint(graph: &StructuralGraph, rule: &DesignRule) -> StructuralGraph {
    let Some(target_id) = sorted_node_ids(graph).first().copied() else {
        return graph.clone();
    };

    let mut next = graph.clone();
    let Some(original) = graph.nodes().get(&target_id).cloned() else {
        return graph.clone();
    };

    let mut attrs = original.attributes;
    attrs.insert(format!("constraint_{}", rule.id.as_u128()), Value::Bool(true));

    next = next.with_node_removed(target_id);
    next.with_node_added(DesignNode::new(target_id, original.kind, attrs))
}

fn apply_rewire_dependency(graph: &StructuralGraph) -> StructuralGraph {
    let mut next = graph.clone();

    if let Some((from, to)) = graph.edges().iter().next().copied() {
        next = next.with_edge_removed(from, to);

        for candidate in sorted_node_ids(graph) {
            if candidate != from && !next.edges().contains(&(from, candidate)) {
                return next.with_edge_added(from, candidate);
            }
        }
        return next;
    }

    let ids = sorted_node_ids(graph);
    if ids.len() >= 2 {
        next = next.with_edge_added(ids[0], ids[1]);
    }
    next
}

fn sorted_node_ids(graph: &StructuralGraph) -> Vec<Uuid> {
    graph.nodes().keys().copied().collect()
}

fn append_rule_history(snapshot: &str, rule_id: RuleId) -> String {
    let mut history = parse_rule_history(snapshot);
    history.push(rule_id);
    let joined = history
        .iter()
        .map(|id| id.as_u128().to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("history:{joined}")
}

fn parse_rule_history(snapshot: &str) -> Vec<RuleId> {
    let Some(raw) = snapshot.strip_prefix("history:") else {
        return Vec::new();
    };

    raw.split(',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u128>().ok())
        .map(RuleId::from_u128)
        .collect()
}

fn risk_score_from_chm(state: &DesignState, chm: &Chm) -> f64 {
    let history = parse_rule_history(&state.profile_snapshot);
    if history.len() < 2 {
        return 0.5;
    }

    let mut penalties = Vec::new();
    for pair in history.windows(2) {
        let from = pair[0];
        let to = pair[1];

        let strength = chm
            .rule_graph
            .get(&from)
            .and_then(|edges| edges.iter().find(|edge| edge.to_rule == to))
            .map(|edge| edge.strength)
            .unwrap_or(0.0);

        penalties.push((-strength).max(0.0));
    }

    if penalties.is_empty() {
        return 0.5;
    }

    let avg_penalty = penalties.iter().sum::<f64>() / penalties.len() as f64;
    (1.0 - avg_penalty).clamp(0.0, 1.0)
}

fn normalize_by_depth(candidates: Vec<(DesignState, ObjectiveVector)>) -> Vec<(DesignState, ObjectiveVector)> {
    let mut min = ObjectiveVector {
        f_struct: 1.0,
        f_field: 1.0,
        f_risk: 1.0,
        f_cost: 1.0,
    };
    let mut max = ObjectiveVector {
        f_struct: 0.0,
        f_field: 0.0,
        f_risk: 0.0,
        f_cost: 0.0,
    };

    for (_, obj) in &candidates {
        min.f_struct = min.f_struct.min(obj.f_struct);
        min.f_field = min.f_field.min(obj.f_field);
        min.f_risk = min.f_risk.min(obj.f_risk);
        min.f_cost = min.f_cost.min(obj.f_cost);
        max.f_struct = max.f_struct.max(obj.f_struct);
        max.f_field = max.f_field.max(obj.f_field);
        max.f_risk = max.f_risk.max(obj.f_risk);
        max.f_cost = max.f_cost.max(obj.f_cost);
    }

    candidates
        .into_iter()
        .map(|(state, obj)| {
            (
                state,
                ObjectiveVector {
                    f_struct: norm(obj.f_struct, min.f_struct, max.f_struct),
                    f_field: norm(obj.f_field, min.f_field, max.f_field),
                    f_risk: norm(obj.f_risk, min.f_risk, max.f_risk),
                    f_cost: norm(obj.f_cost, min.f_cost, max.f_cost),
                },
            )
        })
        .collect()
}

fn norm(value: f64, min: f64, max: f64) -> f64 {
    let denom = max - min;
    if denom.abs() < 1e-12 {
        1.0
    } else {
        ((value - min) / denom).clamp(0.0, 1.0)
    }
}

fn deterministic_state_id(
    state: &DesignState,
    rule: &DesignRule,
    snapshot: &str,
    node_count: usize,
    edge_count: usize,
) -> StateId {
    let mut acc = 0xcbf29ce484222325u128;
    acc = fnv_mix_u128(acc, state.id.as_u128());
    acc = fnv_mix_u128(acc, rule.id.as_u128());
    acc = fnv_mix_u128(acc, node_count as u128);
    acc = fnv_mix_u128(acc, edge_count as u128);
    for b in snapshot.as_bytes() {
        acc = fnv_mix_u128(acc, *b as u128);
    }
    Uuid::from_u128(acc)
}

fn deterministic_uuid(a: u128, b: u128, salt: u128) -> Uuid {
    let mut acc = 0x9e3779b97f4a7c15u128;
    acc = fnv_mix_u128(acc, a);
    acc = fnv_mix_u128(acc, b);
    acc = fnv_mix_u128(acc, salt);
    Uuid::from_u128(acc)
}

fn fnv_mix_u128(acc: u128, value: u128) -> u128 {
    let prime = 0x100000001b3u128;
    (acc ^ value).wrapping_mul(prime)
}

pub fn build_target_field(
    field: &FieldEngine,
    shm: &Shm,
    state: &DesignState,
    lambda: f64,
) -> TargetField {
    let global_categories = categories_from_rules(shm.rules.iter().map(|r| r.category.clone()));
    let local_categories = categories_from_rules(
        shm.applicable_rules(state)
            .into_iter()
            .map(|rule| rule.category.clone()),
    );

    let global = compose_category_field(field, &global_categories);
    let local = compose_category_field(field, &local_categories);
    TargetField::blend(&global, &local, lambda as f32)
}

pub fn p_inferred(
    p_shm: &PreferenceProfile,
    p_pareto: &PreferenceProfile,
    p_chm: &PreferenceProfile,
    prev: &PreferenceProfile,
) -> PreferenceProfile {
    let raw = PreferenceProfile {
        struct_weight: P_INFER_ALPHA * p_shm.struct_weight + P_INFER_BETA * p_pareto.struct_weight + P_INFER_GAMMA * p_chm.struct_weight,
        field_weight: P_INFER_ALPHA * p_shm.field_weight + P_INFER_BETA * p_pareto.field_weight + P_INFER_GAMMA * p_chm.field_weight,
        risk_weight: P_INFER_ALPHA * p_shm.risk_weight + P_INFER_BETA * p_pareto.risk_weight + P_INFER_GAMMA * p_chm.risk_weight,
        cost_weight: P_INFER_ALPHA * p_shm.cost_weight + P_INFER_BETA * p_pareto.cost_weight + P_INFER_GAMMA * p_chm.cost_weight,
    }
    .normalized();

    PreferenceProfile {
        struct_weight: (1.0 - 0.2) * prev.struct_weight + 0.2 * raw.struct_weight,
        field_weight: (1.0 - 0.2) * prev.field_weight + 0.2 * raw.field_weight,
        risk_weight: (1.0 - 0.2) * prev.risk_weight + 0.2 * raw.risk_weight,
        cost_weight: (1.0 - 0.2) * prev.cost_weight + 0.2 * raw.cost_weight,
    }
    .normalized()
}

pub fn need_from_objective(obj: &ObjectiveVector) -> PreferenceProfile {
    PreferenceProfile {
        struct_weight: 1.0 - obj.f_struct,
        field_weight: 1.0 - obj.f_field,
        risk_weight: 1.0 - obj.f_risk,
        cost_weight: 1.0 - obj.f_cost,
    }
    .normalized()
}

pub fn stability_index(
    high_reliability: f64,
    safety_critical: f64,
    experimental: f64,
    rapid_prototype: f64,
) -> f64 {
    core_stability_index(high_reliability, safety_critical, experimental, rapid_prototype)
}

pub fn chm_density(n_edge_obs: usize, category_count: usize) -> f64 {
    if category_count <= 1 {
        return 0.0;
    }
    let denom = (category_count * (category_count - 1)) as f64;
    (n_edge_obs as f64 / denom).clamp(0.0, 1.0)
}

pub fn profile_modulation(stability_index: f64) -> f64 {
    let s = stability_index.clamp(-1.0, 1.0);
    let sigma = 1.0 / (1.0 + (-1.5 * s).exp());
    0.85 + (1.20 - 0.85) * sigma
}

fn make_dense_trace_chm(shm: &Shm, seed: u64) -> Chm {
    let mut chm = Chm::default();
    let ids: Vec<Uuid> = shm.rules.iter().map(|r| r.id).collect();
    for (i, from) in ids.iter().enumerate() {
        for (j, to) in ids.iter().enumerate() {
            if i == j {
                continue;
            }
            chm.insert_edge(*from, *to, pseudo_strength(seed, *from, *to));
        }
    }
    chm
}

fn pseudo_strength(seed: u64, a: Uuid, b: Uuid) -> f64 {
    let mut x = seed ^ (a.as_u128() as u64).wrapping_mul(0x9e3779b97f4a7c15);
    x ^= (b.as_u128() as u64).wrapping_mul(0xD1B54A32D192ED03);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    let frac = (x as f64) / (u64::MAX as f64);
    frac * 2.0 - 1.0
}

fn trace_initial_state(seed: u64) -> DesignState {
    let mut graph = StructuralGraph::default();
    let categories = ["Interface", "Storage", "Network", "Compute", "Control"];
    for i in 0..6u128 {
        let mut attrs = BTreeMap::new();
        attrs.insert("seed".to_string(), Value::Int(seed as i64 + i as i64));
        attrs.insert(
            "category".to_string(),
            Value::Text(categories[(i as usize) % categories.len()].to_string()),
        );
        graph = graph.with_node_added(DesignNode::new(Uuid::from_u128(100 + i), format!("N{i}"), attrs));
    }
    for i in 0..5u128 {
        graph = graph.with_edge_added(Uuid::from_u128(100 + i), Uuid::from_u128(101 + i));
    }
    DesignState::new(Uuid::from_u128(42), Arc::new(graph), "history:")
}

fn moving_average_tail(v: &[f64], k: usize) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let start = v.len().saturating_sub(k);
    let slice = &v[start..];
    slice.iter().sum::<f64>() / slice.len() as f64
}

fn variance(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64
}

fn select_k_with_hysteresis(prev_k: usize, stability_index: f64) -> usize {
    match prev_k {
        4 => {
            if stability_index < 0.70 {
                3
            } else {
                4
            }
        }
        3 => {
            if stability_index >= 0.80 {
                4
            } else if stability_index < 0.40 {
                2
            } else {
                3
            }
        }
        _ => {
            if stability_index >= 0.50 {
                3
            } else {
                2
            }
        }
    }
}

fn categories_from_rules<I>(categories: I) -> Vec<NodeCategory>
where
    I: IntoIterator<Item = RuleCategory>,
{
    let mut out = Vec::new();
    for c in categories {
        let mapped = match c {
            RuleCategory::Structural => NodeCategory::Abstraction,
            RuleCategory::Performance => NodeCategory::Performance,
            RuleCategory::Reliability => NodeCategory::Reliability,
            RuleCategory::Cost => NodeCategory::CostSensitive,
            RuleCategory::Refactor => NodeCategory::Control,
            RuleCategory::ConstraintPropagation => NodeCategory::Constraint,
        };
        if !out.contains(&mapped) {
            out.push(mapped);
        }
    }
    out
}

fn compose_category_field(field: &FieldEngine, categories: &[NodeCategory]) -> FieldVector {
    if categories.is_empty() {
        return FieldVector::zeros(field.dimensions());
    }
    let basis = categories
        .iter()
        .map(|c| field.projector().basis_for(*c))
        .collect::<Vec<_>>();
    FieldVector::average(&basis, field.dimensions())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use chm::Chm;
    use core_types::ObjectiveVector;
    use evaluator::{Evaluator, StructuralEvaluator};
    use field_engine::FieldEngine;
    use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid};
    use profile::PreferenceProfile;
    use shm::Shm;

    use crate::{
        apply_atomic, apply_macro, build_target_field, chm_density, dominates, need_from_objective,
        normalize_by_depth, p_inferred, profile_modulation, scalar_score, stability_index, BeamSearch,
        MacroOperator, ParetoFront, Phase45Controller, ProfileUpdateType, SearchConfig, SearchMode,
        SystemEvaluator,
    };

    fn base_state() -> DesignState {
        let mut graph = StructuralGraph::default();
        graph = graph.with_node_added(DesignNode::new(Uuid::from_u128(1), "A", BTreeMap::new()));
        graph = graph.with_node_added(DesignNode::new(Uuid::from_u128(2), "B", BTreeMap::new()));
        graph = graph.with_edge_added(Uuid::from_u128(1), Uuid::from_u128(2));

        DesignState::new(Uuid::from_u128(50), Arc::new(graph), "history:")
    }

    #[test]
    fn pareto_dominance_correctness() {
        let a = ObjectiveVector {
            f_struct: 0.9,
            f_field: 0.5,
            f_risk: 0.7,
            f_cost: 0.8,
        };
        let b = ObjectiveVector {
            f_struct: 0.8,
            f_field: 0.5,
            f_risk: 0.7,
            f_cost: 0.7,
        };
        assert!(dominates(&a, &b));

        let mut front = ParetoFront::new();
        front.insert(Uuid::from_u128(1), b);
        front.insert(Uuid::from_u128(2), a);

        let ids = front.get_front();
        assert_eq!(ids, vec![Uuid::from_u128(2)]);
    }

    #[test]
    fn beam_truncation_correctness() {
        let mut items = vec![
            ObjectiveVector {
                f_struct: 0.5,
                f_field: 0.5,
                f_risk: 0.5,
                f_cost: 0.5,
            },
            ObjectiveVector {
                f_struct: 0.9,
                f_field: 0.5,
                f_risk: 0.5,
                f_cost: 0.5,
            },
            ObjectiveVector {
                f_struct: 0.7,
                f_field: 0.5,
                f_risk: 0.5,
                f_cost: 0.5,
            },
        ];

        items.sort_by(|l, r| scalar_score(r).partial_cmp(&scalar_score(l)).expect("finite"));

        assert!(scalar_score(&items[0]) >= scalar_score(&items[1]));
        assert!(scalar_score(&items[1]) >= scalar_score(&items[2]));
    }

    #[test]
    fn deterministic_result_verification() {
        let shm = Shm::with_default_rules();
        let mut chm = Chm::default();
        chm.insert_edge(Uuid::from_u128(1001), Uuid::from_u128(1002), -0.2);

        let field = FieldEngine::new(16);
        let evaluator = SystemEvaluator::with_base(&chm, &field, StructuralEvaluator::new(20, 40));
        let engine = BeamSearch {
            shm: &shm,
            chm: &chm,
            evaluator: &evaluator,
            config: SearchConfig {
                beam_width: 3,
                max_depth: 2,
            },
        };

        let initial = base_state();
        let first = engine.search(&initial);
        let second = engine.search(&initial);

        let first_ids: Vec<_> = first.iter().map(|s| s.id).collect();
        let second_ids: Vec<_> = second.iter().map(|s| s.id).collect();
        assert_eq!(first_ids, second_ids);
    }

    #[test]
    fn no_mutation_of_original_state() {
        let shm = Shm::with_default_rules();
        let state = base_state();
        let rule = shm.rules.first().expect("rule exists");

        let before_nodes = state.graph.nodes().len();
        let before_edges = state.graph.edges().len();

        let _new_state = apply_atomic(rule, &state);

        assert_eq!(state.graph.nodes().len(), before_nodes);
        assert_eq!(state.graph.edges().len(), before_edges);
        assert_eq!(state.id, Uuid::from_u128(50));
    }

    #[test]
    fn depth_normalization_bounds() {
        let state = base_state();
        let candidates = vec![
            (
                state.clone(),
                ObjectiveVector {
                    f_struct: 0.3,
                    f_field: 0.5,
                    f_risk: 0.2,
                    f_cost: 0.9,
                },
            ),
            (
                state,
                ObjectiveVector {
                    f_struct: 0.7,
                    f_field: 0.8,
                    f_risk: 0.4,
                    f_cost: 0.2,
                },
            ),
        ];

        let normalized = normalize_by_depth(candidates);
        assert!(normalized.iter().all(|(_, o)| (0.0..=1.0).contains(&o.f_struct)));
        assert!(normalized.iter().all(|(_, o)| (0.0..=1.0).contains(&o.f_field)));
        assert!(normalized.iter().all(|(_, o)| (0.0..=1.0).contains(&o.f_risk)));
        assert!(normalized.iter().all(|(_, o)| (0.0..=1.0).contains(&o.f_cost)));
    }

    #[test]
    fn auto_manual_mode_behavior() {
        let shm = Shm::with_default_rules();
        let chm = Chm::default();
        let field = FieldEngine::new(8);
        let evaluator = SystemEvaluator::with_base(&chm, &field, StructuralEvaluator::new(20, 40));
        let engine = BeamSearch {
            shm: &shm,
            chm: &chm,
            evaluator: &evaluator,
            config: SearchConfig {
                beam_width: 2,
                max_depth: 3,
            },
        };

        let initial = base_state();
        let auto = engine.search_with_mode(&initial, SearchMode::Auto);
        let manual = engine.search_with_mode(&initial, SearchMode::Manual);

        assert!(manual.depth_fronts.len() >= auto.depth_fronts.len());
        assert!(auto.depth_fronts.len() <= 1);
    }

    #[test]
    fn system_evaluator_uses_chm_and_field() {
        let mut chm = Chm::default();
        chm.insert_edge(Uuid::from_u128(1001), Uuid::from_u128(1002), -0.8);

        let field = FieldEngine::new(8);
        let evaluator = SystemEvaluator::with_base(&chm, &field, StructuralEvaluator::default());
        let state = DesignState::new(
            Uuid::from_u128(9),
            Arc::new(StructuralGraph::default()),
            "history:1001,1002",
        );

        let obj = evaluator.evaluate(&state);
        assert!(obj.f_risk < 0.5);
        assert!((0.0..=1.0).contains(&obj.f_field));
    }

    #[test]
    fn macro_operator_applies_atomic_sequence() {
        let initial = base_state();
        let op = MacroOperator {
            id: Uuid::from_u128(7000),
            steps: vec![shm::Transformation::AddNode, shm::Transformation::AddConstraint],
            max_activations: 2,
        };

        let next = apply_macro(&op, &initial);
        assert!(next.graph.nodes().len() >= initial.graph.nodes().len());
    }

    #[test]
    fn phase45_controller_updates_and_logs() {
        let mut ctrl = Phase45Controller::new(0.5);
        ctrl.on_profile_update(1, 0.8, ProfileUpdateType::TypeAExplicit);
        let log = ctrl.update_depth(2, 0.7, 0.2, 30, 10, 0.9);

        assert_eq!(log.depth, 2);
        assert!((0.0..=1.0).contains(&log.lambda_new));
        assert!(log.delta_lambda.abs() <= 0.05);
        assert!((0.0..=1.0).contains(&log.conf_chm));
    }

    #[test]
    fn target_field_uses_global_and_local_categories() {
        let shm = Shm::with_default_rules();
        let state = base_state();
        let field = FieldEngine::new(16);

        let target_a = build_target_field(&field, &shm, &state, 0.8);
        let target_b = build_target_field(&field, &shm, &state, 0.2);

        assert_eq!(target_a.data.dimensions(), 16);
        assert_ne!(target_a.data, target_b.data);
    }

    #[test]
    fn profile_formula_helpers_behave() {
        let need = need_from_objective(&ObjectiveVector {
            f_struct: 0.9,
            f_field: 0.8,
            f_risk: 0.7,
            f_cost: 0.6,
        });
        assert!(need.cost_weight > need.struct_weight);

        let prev = PreferenceProfile {
            struct_weight: 0.25,
            field_weight: 0.25,
            risk_weight: 0.25,
            cost_weight: 0.25,
        };
        let inferred = p_inferred(&need, &need, &need, &prev);
        assert!((0.0..=1.0).contains(&inferred.struct_weight));

        let s = stability_index(0.8, 0.4, 0.1, 0.2);
        let h = profile_modulation(s);
        let d = chm_density(20, 10);
        assert!((-1.0..=1.0).contains(&s));
        assert!((0.85..=1.20).contains(&h));
        assert!((0.0..=1.0).contains(&d));
    }
}
