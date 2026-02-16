use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub static DISTANCE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static NN_DISTANCE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);


mod diversity;
mod stability;

use chm::Chm;
use core_types::{stability_index as core_stability_index, ObjectiveVector, P_INFER_ALPHA, P_INFER_BETA, P_INFER_GAMMA};
use diversity::apply_diversity_pressure;
use evaluator::{Evaluator, StructuralEvaluator};
use field_engine::{resonance_score, FieldEngine, FieldVector, NodeCategory, TargetField};
use memory_space::{DesignNode, DesignState, StateId, StructuralGraph, Uuid, Value};
use profile::PreferenceProfile;
use shm::{DesignRule, EffectVector, RuleCategory, RuleId, Shm, Transformation};
use stability::*;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub max_depth: usize,
    pub norm_alpha: f64,
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
    pub pressure: f32,
    pub epsilon_effect: f32,
    pub target_local_weight: f32,
    pub target_global_weight: f32,
    pub local_global_distance: f32,
    pub field_min_distance: f32,
    pub field_rejected_count: usize,
    pub mu: f32,
    pub dhm_k: usize,
    pub dhm_norm: f32,
    pub dhm_resonance_mean: f32,
    pub dhm_score_ratio: f32,
    pub dhm_build_us: f32,
    pub expanded_categories_count: usize,
    pub selected_rules_count: usize,
    pub per_category_selected: String,
    pub entropy_per_depth: f32,
    pub unique_category_count_per_depth: usize,
    pub pareto_front_size_per_depth: usize,
    pub pareto_mean_nn_dist: f32,
    pub pareto_spacing: f32,
    pub pareto_hv_2d: f32,
    pub field_extract_us: f32,
    pub field_score_us: f32,
    pub field_aggregate_us: f32,
    pub field_total_us: f32,
    pub norm_median_0: f32,
    pub norm_median_1: f32,
    pub norm_median_2: f32,
    pub norm_median_3: f32,
    pub norm_mad_0: f32,
    pub norm_mad_1: f32,
    pub norm_mad_2: f32,
    pub norm_mad_3: f32,
    pub median_nn_dist_all_depth: f32,
    pub collapse_flag: bool,
    pub normalization_mode: String,
    pub unique_norm_vec_count: usize,
    pub norm_dim_mad_zero_count: usize,
    pub mean_nn_dist_raw: f32,
    pub mean_nn_dist_norm: f32,
    pub pareto_spacing_raw: f32,
    pub pareto_spacing_norm: f32,
    pub distance_calls: usize,
    pub nn_distance_calls: usize,
    pub weak_dim_count: usize,
    pub effective_dim_count: usize,
    pub alpha_t: f32,
    pub weak_contrib_ratio: f32,
    pub collapse_proxy: f32,
    // Stability V3
    pub redundancy_flags: String,
    pub saturation_flags: String,
    pub discrete_saturation_count: usize,
    pub effective_dim: usize,
    pub effective_dim_ratio: f32,
    pub collapse_reasons: String,
}

impl Default for TraceRow {
    fn default() -> Self {
        Self {
            depth: 0,
            lambda: 0.0,
            delta_lambda: 0.0,
            tau_prime: 0.0,
            conf_chm: 0.0,
            density: 0.0,
            k: 0,
            h_profile: 0.0,
            pareto_size: 0,
            diversity: 0.0,
            resonance_avg: 0.0,
            pressure: 0.0,
            epsilon_effect: 0.0,
            target_local_weight: 0.0,
            target_global_weight: 0.0,
            local_global_distance: 0.0,
            field_min_distance: 0.0,
            field_rejected_count: 0,
            mu: 0.0,
            dhm_k: 0,
            dhm_norm: 0.0,
            dhm_resonance_mean: 0.0,
            dhm_score_ratio: 0.0,
            dhm_build_us: 0.0,
            expanded_categories_count: 0,
            selected_rules_count: 0,
            per_category_selected: String::new(),
            entropy_per_depth: 0.0,
            unique_category_count_per_depth: 0,
            pareto_front_size_per_depth: 0,
            pareto_mean_nn_dist: 0.0,
            pareto_spacing: 0.0,
            pareto_hv_2d: 0.0,
            field_extract_us: 0.0,
            field_score_us: 0.0,
            field_aggregate_us: 0.0,
            field_total_us: 0.0,
            norm_median_0: 0.0,
            norm_median_1: 0.0,
            norm_median_2: 0.0,
            norm_median_3: 0.0,
            norm_mad_0: 0.0,
            norm_mad_1: 0.0,
            norm_mad_2: 0.0,
            norm_mad_3: 0.0,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: false,
            normalization_mode: String::new(),
            unique_norm_vec_count: 0,
            norm_dim_mad_zero_count: 0,
            mean_nn_dist_raw: 0.0,
            mean_nn_dist_norm: 0.0,
            pareto_spacing_raw: 0.0,
            pareto_spacing_norm: 0.0,
            distance_calls: 0,
            nn_distance_calls: 0,
            weak_dim_count: 0,
            effective_dim_count: 0,
            alpha_t: 0.0,
            weak_contrib_ratio: 0.0,
            collapse_proxy: 0.0,
            redundancy_flags: String::new(),
            saturation_flags: String::new(),
            discrete_saturation_count: 0,
            effective_dim: 0,
            effective_dim_ratio: 0.0,
            collapse_reasons: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase1Variant {
    Base,
    Delta,
    Ortho { epsilon: f64 },
}

impl Phase1Variant {
    fn name(self) -> &'static str {
        match self {
            Phase1Variant::Base => "Base",
            Phase1Variant::Delta => "Delta",
            Phase1Variant::Ortho { .. } => "Ortho",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phase1Config {
    pub depth: usize,
    pub beam: usize,
    pub seed: u64,
    pub norm_alpha: f64,
    pub alpha: f64,
    pub temperature: f64,
    pub entropy_beta: f64,
    pub lambda_min: f64,
    pub lambda_target_entropy: f64,
    pub lambda_k: f64,
    pub lambda_ema: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phase1RawRow {
    pub variant: String,
    pub depth: usize,
    pub beam_index: usize,
    pub rule_id: String,
    pub objective_vector_raw: String,
    pub objective_vector_norm: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phase1SummaryRow {
    pub variant: String,
    pub depth: usize,
    pub corr_matrix_flat: String,
    pub mean_nn_dist: f64,
    pub spacing: f64,
    pub pareto_front_size: usize,
    pub collapse_flag: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveRaw(pub [f64; 4]);

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveNorm(pub [f64; 4]);

#[derive(Clone, Debug, PartialEq)]
pub struct GlobalRobustStats {
    pub median: [f64; 4],
    pub mad: [f64; 4],
    pub mean: [f64; 4],
    pub std: [f64; 4],
    pub active_dims: [bool; 4], // Meaning "Not Degenerate" (Strong OR Weak)
    pub weak_dims: [bool; 4],   // Subset of active_dims
    pub weights: [f64; 4],
    pub mad_zero_count: usize,
    pub alpha_used: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveAlphaState {
    pub alpha: f64,
    pub alpha_prev: f64,
    pub d_prev: f64,
}

impl AdaptiveAlphaState {
    pub fn new(initial_alpha: f64) -> Self {
        Self {
            alpha: initial_alpha,
            alpha_prev: initial_alpha,
            d_prev: 0.0,
        }
    }
}

pub fn calculate_adaptive_alpha(
    state: &AdaptiveAlphaState,
    stats: &GlobalRobustStats,
    mean_nn_dist: f64,
    pareto_size: usize,
    d_target: f64,
    effective_dim: usize,
) -> AdaptiveAlphaState {
    // Rule 4: Effective Dimension Guarantee
    // If effective_dim < 3, alpha adjustment is invalid.
    if effective_dim < 3 {
        return state.clone();
    }

    let alpha_min = 0.01;
    let alpha_max = 0.20;
    let r0 = 0.25;
    let r1 = 0.75;
    let k = 0.05;
    let beta = 0.2;
    let rho_max = 0.35;
    let delta = 0.1 * d_target;

    // 1. Input Metrics
    let s_count = stats
        .active_dims
        .iter()
        .zip(stats.weak_dims.iter())
        .filter(|&(&a, &w)| a && !w)
        .count() as f64;
    let w_count = stats.weak_dims.iter().filter(|&&w| w).count() as f64;
    let e_count = s_count + w_count;

    // 2. Base Alpha (State-based)
    let r = w_count / e_count.max(1.0);
    let ratio_factor = ((r - r0) / (r1 - r0)).clamp(0.0, 1.0);
    let alpha_base = alpha_min + (alpha_max - alpha_min) * ratio_factor;

    // 3. Feedback Correction (Collapse Proxy)
    // If d < d_target, we need MORE alpha (to increase distance).
    // If d > d_target, we can reduce alpha.
    let error = ((d_target - mean_nn_dist) / d_target).clamp(-1.0, 1.0);
    let alpha_fb = alpha_base + k * error;

    // 4. Smoothing
    let alpha_target = (1.0 - beta) * state.alpha + beta * alpha_fb;

    // 5. Hysteresis (Deadband)
    let mut next_alpha = alpha_target;
    if (mean_nn_dist - d_target).abs() < delta && pareto_size > 1 {
        // Inside deadband, keep previous alpha to prevent chattering
        next_alpha = state.alpha;
    }

    // 6. Safety Valve (Outlier Dominance Suppression)
    // alpha * W / (S + alpha * W) <= rho_max
    // alpha * W <= rho_max * S + rho_max * alpha * W
    // alpha * W * (1 - rho_max) <= rho_max * S
    // alpha <= (rho_max * S) / (W * (1 - rho_max))
    if w_count > 0.0 {
        let max_allowed = (rho_max * s_count) / (w_count * (1.0 - rho_max));
        next_alpha = next_alpha.min(max_allowed);
    }

    // Final Clamp
    next_alpha = next_alpha.clamp(alpha_min, alpha_max);

    AdaptiveAlphaState {
        alpha: next_alpha,
        alpha_prev: state.alpha,
        d_prev: mean_nn_dist,
    }
}

#[derive(Clone, Debug, Default)]
struct GlobalRobustEstimator {
    samples: Vec<ObjectiveRaw>,
    frozen: Option<GlobalRobustStats>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceRunConfig {
    pub depth: usize,
    pub beam: usize,
    pub seed: u64,
    pub norm_alpha: f64,
    pub adaptive_alpha: bool,
    pub raw_output_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MuSchedule {
    Fixed { mu: f32 },
    DepthExp { mu_max: f32, k: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DhMConfig {
    pub enabled: bool,
    pub mu_schedule: MuSchedule,
    pub gamma: f32,
    pub k_nearest: usize,
}

impl DhMConfig {
    pub fn phase7_fixed() -> Self {
        Self {
            enabled: true,
            mu_schedule: MuSchedule::Fixed { mu: 0.05 },
            gamma: 0.05,
            k_nearest: 20,
        }
    }

    pub fn mu_at_depth(&self, depth: usize) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        match self.mu_schedule {
            MuSchedule::Fixed { mu } => mu as f64,
            MuSchedule::DepthExp { mu_max, k } => {
                let d = depth as f64;
                let mu_max = mu_max as f64;
                let k = k as f64;
                mu_max * (1.0 - (-k * d).exp())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BenchConfig {
    pub depth: usize,
    pub beam: usize,
    pub iterations: usize,
    pub warmup: usize,
    pub seed: u64,
    pub norm_alpha: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BenchResult {
    pub depth: usize,
    pub beam: usize,
    pub iterations: usize,
    pub avg_total_ms: f64,
    pub avg_per_depth_ms: f64,
    pub avg_field_us: f64,
    pub avg_resonance_us: f64,
    pub avg_chm_us: f64,
    pub avg_dhm_us: f64,
    pub avg_pareto_us: f64,
    pub avg_lambda_us: f64,
    pub lambda_final: f64,
}

const FIELD_DISTANCE_DELTA: f64 = 0.5;
const FIELD_CACHE_CAPACITY: usize = 50_000;

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
    
    // Initialize alpha. 
    // If adaptive, start with 0.01 (per spec recommendation) or config.norm_alpha if provided.
    // If fixed, use config.norm_alpha.
    // [Adaptive Alpha v2.2]
    // Initialize with config.norm_alpha (or min/max if specified, 
    // but typically start with 0.01 per spec or config).
    // Let's use config.norm_alpha as the initial value if > 0, else 0.01.
    let initial_alpha = if config.adaptive_alpha {
        if config.norm_alpha > 1e-6 { config.norm_alpha } else { 0.01 }
    } else {
        config.norm_alpha
    };
    let mut adaptive_state = AdaptiveAlphaState::new(initial_alpha);

    let mut profile = PreferenceProfile {
        struct_weight: 0.25,
        field_weight: 0.25,
        risk_weight: 0.25,
        cost_weight: 0.25,
    };
    let mut frontier = vec![trace_initial_state(config.seed)];
    let mut rows = Vec::with_capacity(config.depth);
    let mut depth_boundary_diversity = 1.0f64;
    let dhm_config = DhMConfig::phase7_fixed();
    let mut dhm_memory = vec![(0usize, field.aggregate_state(&frontier[0]))];

    let n_edge_obs = chm.rule_graph.values().map(|v| v.len()).sum::<usize>();
    let mut conflict_hist = Vec::new();
    let mut align_hist = Vec::new();

    for depth in 1..=config.depth {
        controller.on_profile_update(depth, 0.25, ProfileUpdateType::TypeCStatistical);
        let mu = dhm_config.mu_at_depth(depth);
        let t_dhm = Instant::now();
        let (dhm_field, dhm_norm) = build_dhm_field(
            &dhm_memory,
            depth,
            dhm_config.gamma as f64,
            dhm_config.k_nearest,
            field.dimensions(),
        );
        let dhm_build_us = elapsed_us(t_dhm);
        let mut dhm_res_sum = 0.0f64;
        let mut dhm_ratio_sum = 0.0f64;
        let mut dhm_count = 0usize;

        let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
        for state in &frontier {
            let mut ranked_rules = shm
                .applicable_rules(state)
                .into_iter()
                .map(|rule| {
                    let r_dhm = dhm_rule_resonance(rule, &field, &dhm_field);
                    let score_ratio = 1.0 + mu * r_dhm;
                    let score = rule.priority.max(0.0) * score_ratio;
                    (rule, r_dhm, score_ratio, score)
                })
                .collect::<Vec<_>>();
            ranked_rules.sort_by(|(l_rule, _, _, l_score), (r_rule, _, _, r_score)| {
                r_score
                    .partial_cmp(l_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| l_rule.id.cmp(&r_rule.id))
            });

            for (rule, r_dhm, score_ratio, _) in ranked_rules {
                dhm_res_sum += r_dhm;
                dhm_ratio_sum += score_ratio;
                dhm_count += 1;

                let new_state = apply_atomic(rule, state);
                let obj = evaluator.evaluate(&new_state);
                candidates.push((new_state, obj));
            }
        }
        let dhm_resonance_mean = if dhm_count == 0 {
            0.0
        } else {
            dhm_res_sum / dhm_count as f64
        };
        let dhm_score_ratio = if dhm_count == 0 {
            1.0
        } else {
            dhm_ratio_sum / dhm_count as f64
        };

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
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: 0,
                mu: mu as f32,
                dhm_k: dhm_config.k_nearest,
                dhm_norm: dhm_norm as f32,
                dhm_resonance_mean: dhm_resonance_mean as f32,
                dhm_score_ratio: dhm_score_ratio as f32,
                dhm_build_us: dhm_build_us as f32,
                expanded_categories_count: 0,
                selected_rules_count: 0,
                per_category_selected: String::new(),
                entropy_per_depth: 0.0,
                unique_category_count_per_depth: 0,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: 0.0,
                field_score_us: 0.0,
                field_aggregate_us: 0.0,
                field_total_us: 0.0,
                ..TraceRow::default()
            });
            continue;
        }

        let (filtered_candidates, field_min_distance, field_rejected_count) =
            filter_candidates_by_field_distance(candidates, &field, FIELD_DISTANCE_DELTA);
        if filtered_candidates.is_empty() {
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
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: field_min_distance as f32,
                field_rejected_count,
                mu: mu as f32,
                dhm_k: dhm_config.k_nearest,
                dhm_norm: dhm_norm as f32,
                dhm_resonance_mean: dhm_resonance_mean as f32,
                dhm_score_ratio: dhm_score_ratio as f32,
                dhm_build_us: dhm_build_us as f32,
                expanded_categories_count: 0,
                selected_rules_count: 0,
                per_category_selected: String::new(),
                entropy_per_depth: 0.0,
                unique_category_count_per_depth: 0,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: 0.0,
                field_score_us: 0.0,
                field_aggregate_us: 0.0,
                field_total_us: 0.0,
                ..TraceRow::default()
            });
            frontier = vec![trace_initial_state(config.seed)];
            continue;
        }

        let (normalized, stats) = normalize_by_depth(filtered_candidates, adaptive_state.alpha);
        
        // Stability Analysis V3
        let norm_data: Vec<[f64; 4]> = normalized.iter().map(|(_, obj)| obj_to_arr(obj)).collect();
        let mad_norm = if let Some(s) = &stats { s.mad } else { [0.0; 4] };
        
        // Calculate basic metrics for analyzer
        let weights_default = if let Some(s) = &stats { s.weights } else { [1.0; 4] }; // Approximation
        let mean_nn_norm = mean_nn_dist_norm(
            &normalized.iter().map(|(_, o)| normalize_objective(&ObjectiveRaw(obj_to_arr(o)), s.as_ref().unwrap())).collect::<Vec<_>>(),
            &weights_default
        ); // Wait, normalized is already normalized?
           // normalize_by_depth returns (Vec<(DesignState, ObjectiveVector)>, Option<GlobalRobustStats>)
           // The ObjectiveVector in the Vec IS NORMALIZED.
           // normalize_by_depth calls normalize_phase1_vectors OR uses robust_stats.
           // Let's check normalize_by_depth implementation.
           
        // Re-reading normalize_by_depth (I need to be sure what it returns)
        // It returns (Vec<(DesignState, ObjectiveVector)>, Option<GlobalRobustStats>)
        // The objects are NORMALIZED.
        
        let normalized_objc: Vec<ObjectiveNorm> = normalized.iter().map(|(_, o)| ObjectiveNorm(obj_to_arr(o))).collect();
        let current_mean_nn_dist_norm = mean_nn_dist_norm(&normalized_objc, &weights_default);
        let current_unique_norm_count = count_unique_norm(&normalized_objc, &weights_default);
        
        let stability_metrics = ObjectiveStabilityAnalyzer::analyze(
            &norm_data,
            &mad_norm,
            current_unique_norm_count,
            current_mean_nn_dist_norm,
        );

        let mut pareto = ParetoFront::new();
        for (state, obj) in &normalized {
            pareto.insert(state.id, obj.clone());
        }

        let front_set: BTreeSet<Uuid> = pareto.get_front().into_iter().collect();
        let s_ref = stats.as_ref().unwrap();
        adaptive_state = calculate_adaptive_alpha(
            &adaptive_state,
            s_ref,
            mean_nn_dist_norm_val,
            front.len(),
            0.5,
            stability_metrics.effective_dim,
        );

        rows.push(TraceRow {
            depth,
            lambda: controller.lambda() as f32,
            delta_lambda: controller.lambda() as f32, // Check logic
            tau_prime: adjustment.tau_prime as f32,
            conf_chm: adjustment.conf_chm as f32,
            density: adjustment.density as f32,
            k: controller.k(),
            h_profile: profile_modulation(0.25) as f32,
            pareto_size: front.len(),
            diversity: 0.0,
            resonance_avg: resonance_avg as f32,
            pressure: 0.0,
            epsilon_effect: 0.0,
            target_local_weight: 0.0,
            target_global_weight: 0.0,
            local_global_distance: 0.0,
            field_min_distance: field_min_distance as f32,
            field_rejected_count,
            mu: mu as f32,
            dhm_k: dhm_config.k_nearest,
            dhm_norm: dhm_norm as f32,
            dhm_resonance_mean: dhm_resonance_mean as f32,
            dhm_score_ratio: dhm_score_ratio as f32,
            dhm_build_us: dhm_build_us as f32,
            expanded_categories_count: 0,
            selected_rules_count: 0,
            per_category_selected: String::new(),
            entropy_per_depth: 0.0,
            unique_category_count_per_depth: 0,
            pareto_front_size_per_depth: front.len(),
            pareto_mean_nn_dist: mean_nn_dist_val as f32,
            pareto_spacing: spacing_val as f32,
            pareto_hv_2d: hv_2d as f32,
            field_extract_us: 0.0,
            field_score_us: 0.0,
            field_aggregate_us: 0.0,
            field_total_us: 0.0,
            norm_median_0: s_ref.median[0] as f32,
            norm_median_1: s_ref.median[1] as f32,
            norm_median_2: s_ref.median[2] as f32,
            norm_median_3: s_ref.median[3] as f32,
            norm_mad_0: s_ref.mad[0] as f32,
            norm_mad_1: s_ref.mad[1] as f32,
            norm_mad_2: s_ref.mad[2] as f32,
            norm_mad_3: s_ref.mad[3] as f32,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: stability_metrics.is_collapsed,
            normalization_mode: "RobustV3".to_string(),
            unique_norm_vec_count: unique_norm_count_val,
            norm_dim_mad_zero_count: s_ref.mad_zero_count,
            mean_nn_dist_raw: mean_nn_dist_val as f32,
            mean_nn_dist_norm: mean_nn_dist_norm_val as f32,
            pareto_spacing_raw: spacing_val as f32,
            pareto_spacing_norm: spacing_norm_val as f32,
            distance_calls: DISTANCE_CALL_COUNT.load(Ordering::Relaxed),
            nn_distance_calls: NN_DISTANCE_CALL_COUNT.load(Ordering::Relaxed),
            weak_dim_count: s_ref.weak_dims.iter().filter(|&&w| w).count(),
            effective_dim_count: stability_metrics.effective_dim,
            alpha_t: adaptive_state.alpha as f32,
            weak_contrib_ratio: 0.0,
            collapse_proxy: 0.0,
            redundancy_flags: stability_metrics.redundancy_flags.join("|"),
            saturation_flags: stability_metrics.saturation_flags.join("|"),
            discrete_saturation_count: stability_metrics.discrete_saturation_count,
            effective_dim: stability_metrics.effective_dim,
            effective_dim_ratio: stability_metrics.effective_dim_ratio as f32,
            collapse_reasons: stability_metrics.collapse_reasons.join("|"),
        });        let resonance_avg = front.iter().map(|(_, o)| o.f_field).sum::<f64>() / front.len() as f64;

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
        depth_boundary_diversity = diversity;

        let front_norm_objs: Vec<ObjectiveNorm> = front
            .iter()
            .map(|(_, o)| ObjectiveNorm([o.f_struct, o.f_field, o.f_risk, o.f_cost]))
            .collect();
        let pareto_mean_nn = mean_nn_dist_norm(&front_norm_objs, &stats.weights);
        let unique_norm_vec_count = count_unique_norm(&front_norm_objs, &stats.weights);
        let effective_dim_count = stats.active_dims.iter().filter(|&&a| a).count();
        let weak_dim_count = stats.weak_dims.iter().filter(|&&w| w).count();

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
            pressure: adjustment.pressure as f32,
            epsilon_effect: adjustment.epsilon_effect as f32,
            target_local_weight: adjustment.target_local_weight as f32,
            target_global_weight: adjustment.target_global_weight as f32,
            local_global_distance: adjustment.local_global_distance as f32,
            field_min_distance: field_min_distance as f32,
            field_rejected_count,
            mu: mu as f32,
            dhm_k: dhm_config.k_nearest,
            dhm_norm: dhm_norm as f32,
            dhm_resonance_mean: dhm_resonance_mean as f32,
            dhm_score_ratio: dhm_score_ratio as f32,
            dhm_build_us: dhm_build_us as f32,
            expanded_categories_count: 0,
            selected_rules_count: 0,
            per_category_selected: String::new(),
            entropy_per_depth: 0.0,
            unique_category_count_per_depth: 0,
            pareto_front_size_per_depth: front.len(),
            pareto_mean_nn_dist: 0.0,
            alpha_t: 0.0,
            weak_contrib_ratio: 0.0,
            collapse_proxy: 0.0,
            pareto_spacing: 0.0,
            pareto_hv_2d: 0.0,
            field_extract_us: 0.0,
            field_score_us: 0.0,
            field_aggregate_us: 0.0,
            field_total_us: 0.0,
            norm_median_0: stats.median[0] as f32,
            norm_median_1: stats.median[1] as f32,
            norm_median_2: stats.median[2] as f32,
            norm_median_3: stats.median[3] as f32,
            norm_mad_0: stats.mad[0] as f32,
            norm_mad_1: stats.mad[1] as f32,
            norm_mad_2: stats.mad[2] as f32,
            norm_mad_3: stats.mad[3] as f32,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: front.len() == 1 || pareto_mean_nn == 0.0,
            normalization_mode: "robust_v2".to_string(),
            unique_norm_vec_count,
            norm_dim_mad_zero_count: stats.mad_zero_count,
            mean_nn_dist_raw: 0.0,
            mean_nn_dist_norm: pareto_mean_nn as f32,
            pareto_spacing_raw: 0.0,
            pareto_spacing_norm: 0.0,
            distance_calls: 0,
            nn_distance_calls: 0,
            weak_dim_count,
            effective_dim_count,
        });

        frontier = front
            .into_iter()
            .take(config.beam.max(1))
            .map(|(s, _)| s)
            .collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(config.seed)];
        }
        for state in &frontier {
            dhm_memory.push((depth, field.aggregate_state(state)));
        }

        profile = p_inferred(&profile, &profile, &profile, &profile);
    }

    rows
}

pub fn generate_trace_baseline_off(config: TraceRunConfig) -> Vec<TraceRow> {
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
        let mu = 0.0f64;

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
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: 0,
                mu: mu as f32,
                dhm_k: 0,
                dhm_norm: 0.0,
                dhm_resonance_mean: 0.0,
                dhm_score_ratio: 1.0,
                dhm_build_us: 0.0,
                expanded_categories_count: 0,
                selected_rules_count: 0,
                per_category_selected: String::new(),
                entropy_per_depth: 0.0,
                unique_category_count_per_depth: 0,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: 0.0,
                field_score_us: 0.0,
                field_aggregate_us: 0.0,
                field_total_us: 0.0,
                ..TraceRow::default()
            });
            continue;
        }

        let (normalized, stats) = normalize_by_depth(candidates, config.norm_alpha);
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

        if front.is_empty() {
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
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: 0,
                mu: mu as f32,
                dhm_k: 0,
                dhm_norm: 0.0,
                dhm_resonance_mean: 0.0,
                dhm_score_ratio: 1.0,
                dhm_build_us: 0.0,
                expanded_categories_count: 0,
                selected_rules_count: 0,
                per_category_selected: String::new(),
                entropy_per_depth: 0.0,
                unique_category_count_per_depth: 0,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: 0.0,
                field_score_us: 0.0,
                field_aggregate_us: 0.0,
                field_total_us: 0.0,
                ..TraceRow::default()
            });
            frontier = vec![trace_initial_state(config.seed)];
            continue;
        }

        let depth_boundary_diversity = variance(&front.iter().map(|(_, o)| scalar_score(o)).collect::<Vec<_>>());
        let target_field = build_target_field(
            &field,
            &shm,
            &front[0].0,
            controller.lambda(),
        );

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

        let front_norm_objs: Vec<ObjectiveNorm> = front
            .iter()
            .map(|(_, o)| ObjectiveNorm([o.f_struct, o.f_field, o.f_risk, o.f_cost]))
            .collect();
        let pareto_mean_nn = mean_nn_dist_norm(&front_norm_objs, &stats.weights);
        let unique_norm_vec_count = count_unique_norm(&front_norm_objs, &stats.weights);
        let effective_dim_count = stats.active_dims.iter().filter(|&&a| a).count();
        let weak_dim_count = stats.weak_dims.iter().filter(|&&w| w).count();

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
            diversity: depth_boundary_diversity as f32,
            resonance_avg: resonance_avg as f32,
            pressure: 0.0,
            epsilon_effect: 0.0,
            target_local_weight: 0.0,
            target_global_weight: 0.0,
            local_global_distance: 0.0,
            field_min_distance: 0.0,
            field_rejected_count: 0,
            mu: mu as f32,
            dhm_k: 0,
            dhm_norm: 0.0,
            dhm_resonance_mean: 0.0,
            dhm_score_ratio: 1.0,
            dhm_build_us: 0.0,
            expanded_categories_count: 0,
            selected_rules_count: 0,
            per_category_selected: String::new(),
            entropy_per_depth: 0.0,
            unique_category_count_per_depth: 0,
            pareto_front_size_per_depth: front.len(),
            pareto_mean_nn_dist: 0.0,
            alpha_t: 0.0,
            weak_contrib_ratio: 0.0,
            collapse_proxy: 0.0,
            pareto_spacing: 0.0,
            pareto_hv_2d: 0.0,
            field_extract_us: 0.0,
            field_score_us: 0.0,
            field_aggregate_us: 0.0,
            field_total_us: 0.0,
            norm_median_0: stats.median[0] as f32,
            norm_median_1: stats.median[1] as f32,
            norm_median_2: stats.median[2] as f32,
            norm_median_3: stats.median[3] as f32,
            norm_mad_0: stats.mad[0] as f32,
            norm_mad_1: stats.mad[1] as f32,
            norm_mad_2: stats.mad[2] as f32,
            norm_mad_3: stats.mad[3] as f32,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: front.len() == 1 || pareto_mean_nn == 0.0,
            normalization_mode: "robust_v2".to_string(),
            unique_norm_vec_count,
            norm_dim_mad_zero_count: stats.mad_zero_count,
            mean_nn_dist_raw: 0.0,
            mean_nn_dist_norm: pareto_mean_nn as f32,
            pareto_spacing_raw: 0.0,
            pareto_spacing_norm: 0.0,
            distance_calls: 0,
            nn_distance_calls: 0,
            weak_dim_count,
            effective_dim_count,
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

pub fn generate_trace_baseline_off_balanced(config: TraceRunConfig, m: usize) -> Vec<TraceRow> {
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
        let mu = 0.0f64;
        let mut depth_category_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut depth_selected_rules_count = 0usize;

        let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
        for state in &frontier {
            let (selected_rules, per_state_counts) =
                select_rules_category_balanced(shm.applicable_rules(state), m);
            depth_selected_rules_count += selected_rules.len();
            for (cat, c) in per_state_counts {
                *depth_category_counts.entry(cat).or_insert(0) += c;
            }
            for rule in selected_rules {
                let new_state = apply_atomic(rule, state);
                let obj = evaluator.evaluate(&new_state);
                candidates.push((new_state, obj));
            }
        }
        let expanded_categories_count = depth_category_counts.len();
        let per_category_selected = format_category_counts(&depth_category_counts);

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
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: 0,
                mu: mu as f32,
                dhm_k: 0,
                dhm_norm: 0.0,
                dhm_resonance_mean: 0.0,
                dhm_score_ratio: 1.0,
                dhm_build_us: 0.0,
                expanded_categories_count,
                selected_rules_count: depth_selected_rules_count,
                per_category_selected,
                entropy_per_depth: 0.0,
                unique_category_count_per_depth: expanded_categories_count,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: 0.0,
                field_score_us: 0.0,
                field_aggregate_us: 0.0,
                field_total_us: 0.0,
                ..TraceRow::default()
            });
            continue;
        }

        let (normalized, stats) = normalize_by_depth(candidates, config.norm_alpha);
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

        if front.is_empty() {
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
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: 0,
                mu: mu as f32,
                dhm_k: 0,
                dhm_norm: 0.0,
                dhm_resonance_mean: 0.0,
                dhm_score_ratio: 1.0,
                dhm_build_us: 0.0,
                expanded_categories_count,
                selected_rules_count: depth_selected_rules_count,
                per_category_selected,
                entropy_per_depth: 0.0,
                unique_category_count_per_depth: expanded_categories_count,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: 0.0,
                field_score_us: 0.0,
                field_aggregate_us: 0.0,
                field_total_us: 0.0,
                ..TraceRow::default()
            });
            frontier = vec![trace_initial_state(config.seed)];
            continue;
        }

        let depth_boundary_diversity = variance(&front.iter().map(|(_, o)| scalar_score(o)).collect::<Vec<_>>());
        let target_field = build_target_field(
            &field,
            &shm,
            &front[0].0,
            controller.lambda(),
        );

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

        let front_norm_objs: Vec<ObjectiveNorm> = front
            .iter()
            .map(|(_, o)| ObjectiveNorm([o.f_struct, o.f_field, o.f_risk, o.f_cost]))
            .collect();
        let pareto_mean_nn = mean_nn_dist_norm(&front_norm_objs, &stats.weights);
        let unique_norm_vec_count = count_unique_norm(&front_norm_objs, &stats.weights);
        let effective_dim_count = stats.active_dims.iter().filter(|&&a| a).count();
        let weak_dim_count = stats.weak_dims.iter().filter(|&&w| w).count();

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
            diversity: depth_boundary_diversity as f32,
            resonance_avg: resonance_avg as f32,
            pressure: 0.0,
            epsilon_effect: 0.0,
            target_local_weight: 0.0,
            target_global_weight: 0.0,
            local_global_distance: 0.0,
            field_min_distance: 0.0,
            field_rejected_count: 0,
            mu: mu as f32,
            dhm_k: 0,
            dhm_norm: 0.0,
            dhm_resonance_mean: 0.0,
            dhm_score_ratio: 1.0,
            dhm_build_us: 0.0,
            expanded_categories_count,
            selected_rules_count: depth_selected_rules_count,
            per_category_selected,
            entropy_per_depth: shannon_entropy_from_counts(&depth_category_counts) as f32,
            unique_category_count_per_depth: expanded_categories_count,
            pareto_front_size_per_depth: front.len(),
            pareto_mean_nn_dist: 0.0,
            alpha_t: 0.0,
            weak_contrib_ratio: 0.0,
            collapse_proxy: 0.0,
            pareto_spacing: 0.0,
            pareto_hv_2d: 0.0,
            field_extract_us: 0.0,
            field_score_us: 0.0,
            field_aggregate_us: 0.0,
            field_total_us: 0.0,
            norm_median_0: stats.median[0] as f32,
            norm_median_1: stats.median[1] as f32,
            norm_median_2: stats.median[2] as f32,
            norm_median_3: stats.median[3] as f32,
            norm_mad_0: stats.mad[0] as f32,
            norm_mad_1: stats.mad[1] as f32,
            norm_mad_2: stats.mad[2] as f32,
            norm_mad_3: stats.mad[3] as f32,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: front.len() == 1 || pareto_mean_nn == 0.0,
            normalization_mode: "robust_v2".to_string(),
            unique_norm_vec_count,
            norm_dim_mad_zero_count: stats.mad_zero_count,
            mean_nn_dist_raw: 0.0,
            mean_nn_dist_norm: pareto_mean_nn as f32,
            pareto_spacing_raw: 0.0,
            pareto_spacing_norm: 0.0,
            distance_calls: 0,
            nn_distance_calls: 0,
            weak_dim_count,
            effective_dim_count,
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

pub fn generate_trace_baseline_off_soft(
    config: TraceRunConfig,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
    lambda_min: f64,
    lambda_target_entropy: f64,
    lambda_k: f64,
    lambda_ema: f64,
    field_profile: bool,
) -> Vec<TraceRow> {
    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, config.seed);
    let field = FieldEngine::new(256);
    let structural = StructuralEvaluator::default();

    let mut frontier = vec![trace_initial_state(config.seed)];
    let mut rows = Vec::with_capacity(config.depth);
    let mut lambda = 0.5f64;
    let mut field_cache: BTreeMap<(u128, u128, usize, usize), FieldVector> = BTreeMap::new();
    let mut field_cache_order: VecDeque<(u128, u128, usize, usize)> = VecDeque::new();
    let mut estimator = GlobalRobustEstimator::default();
    let warmup_depths = 10usize;

    let initial_alpha = if config.adaptive_alpha {
        if config.norm_alpha > 1e-6 { config.norm_alpha } else { 0.01 }
    } else {
        config.norm_alpha
    };
    let mut adaptive_state = AdaptiveAlphaState::new(initial_alpha);

    for depth in 1..=config.depth {
        let calls_start = DISTANCE_CALL_COUNT.load(Ordering::Relaxed);
        let nn_calls_start = NN_DISTANCE_CALL_COUNT.load(Ordering::Relaxed);
        let norm_alpha_val = if config.adaptive_alpha {
            adaptive_state.alpha
        } else {
            config.norm_alpha
        };
        let mu = 0.0f64;
        let target_field = build_target_field(&field, &shm, &frontier[0], lambda);
        let batch = build_soft_candidates_for_frontier(
            &frontier,
            config.beam.max(1),
            depth,
            alpha,
            temperature,
            entropy_beta,
            &field,
            &shm,
            &chm,
            &structural,
            &target_field,
            field_profile,
            &mut field_cache,
            &mut field_cache_order,
        );
        let candidates = batch.candidates;
        let expanded_categories_count = batch.depth_category_counts.len();
        let per_category_selected = format_category_counts(&batch.depth_category_counts);
        let entropy_per_depth = shannon_entropy_from_counts(&batch.depth_category_counts) as f32;
        let depth_selected_rules_count = batch.depth_selected_rules_count;
        let field_extract_us = batch.field_extract_us;
        let field_score_us = batch.field_score_us;
        let field_aggregate_us = batch.field_aggregate_us;
        let field_total_us = batch.field_total_us;

        if let Some(path) = &config.raw_output_path {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("failed to open raw trace file");
            
            if depth == 1 {
                writeln!(file, "depth,candidate_id,objective_0,objective_1,objective_2,objective_3").unwrap();
            }

            for (i, (_, obj)) in candidates.iter().enumerate() {
                writeln!(
                    file,
                    "{},{},{},{},{},{}",
                    depth,
                    i,
                    obj.f_struct,
                    obj.f_field,
                    obj.f_risk,
                    obj.f_cost
                )
                .unwrap();
            }
        }

        if depth <= warmup_depths {
            estimator
                .samples
                .extend(candidates.iter().map(|(_, o)| ObjectiveRaw(obj_to_arr(o))));
            if depth == warmup_depths {
                estimator.frozen = robust_stats_from_samples(&estimator.samples, norm_alpha_val);
            }
        }
        let stats = estimator
            .frozen.clone()
            .or_else(|| robust_stats_from_samples(&estimator.samples, norm_alpha_val))
            .unwrap_or(GlobalRobustStats {
                alpha_used: norm_alpha_val,
                median: [0.0; 4],
                mad: [1.0; 4],
                mean: [0.0; 4],
                std: [1.0; 4],
                active_dims: [true; 4],
                weak_dims: [false; 4],
                weights: [1.0; 4],
                mad_zero_count: 0,
            });

        let lambda_old = lambda;
        lambda = update_lambda_entropy(
            lambda,
            entropy_per_depth as f64,
            lambda_target_entropy,
            lambda_k,
            lambda_ema,
            lambda_min,
            1.0,
        );

        if candidates.is_empty() {
            rows.push(TraceRow {
                depth,
                lambda: lambda as f32,
                delta_lambda: (lambda - lambda_old) as f32,
                tau_prime: 0.0,
                conf_chm: 0.0,
                density: 0.0,
                k: 0,
                h_profile: 1.0,
                pareto_size: 0,
                diversity: 0.0,
                resonance_avg: 0.0,
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.0,
                target_global_weight: 0.0,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: 0,
                mu: mu as f32,
                dhm_k: 0,
                dhm_norm: 0.0,
                dhm_resonance_mean: 0.0,
                dhm_score_ratio: 1.0,
                dhm_build_us: 0.0,
                expanded_categories_count,
                selected_rules_count: depth_selected_rules_count,
                per_category_selected,
                entropy_per_depth,
                unique_category_count_per_depth: expanded_categories_count,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: field_extract_us as f32,
                field_score_us: field_score_us as f32,
                field_aggregate_us: field_aggregate_us as f32,
                field_total_us: field_total_us as f32,
                norm_median_0: stats.median[0] as f32,
                norm_median_1: stats.median[1] as f32,
                norm_median_2: stats.median[2] as f32,
                norm_median_3: stats.median[3] as f32,
                norm_mad_0: stats.mad[0] as f32,
                norm_mad_1: stats.mad[1] as f32,
                norm_mad_2: stats.mad[2] as f32,
                norm_mad_3: stats.mad[3] as f32,
                median_nn_dist_all_depth: 0.0,
                collapse_flag: false,
                normalization_mode: "global_robust".to_string(),
                unique_norm_vec_count: 0,
                norm_dim_mad_zero_count: 0,
                mean_nn_dist_raw: 0.0,
                mean_nn_dist_norm: 0.0,
                pareto_spacing_raw: 0.0,
                pareto_spacing_norm: 0.0,
                distance_calls: 0,
                nn_distance_calls: 0,
                weak_dim_count: 0,
                effective_dim_count: 0,
                alpha_t: 0.0,
                weak_contrib_ratio: 0.0,
                collapse_proxy: 0.0,
            });
            continue;
        }

        let (normalized, _) = normalize_by_depth(candidates, norm_alpha_val);
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

        if front.is_empty() {
            frontier = vec![trace_initial_state(config.seed)];
            continue;
        }

        let front_norm = front
            .iter()
            .map(|(_, o)| normalize_objective(&ObjectiveRaw(obj_to_arr(o)), &stats))
            .collect::<Vec<_>>();
        let depth_boundary_diversity = variance(&front.iter().map(|(_, o)| scalar_score(o)).collect::<Vec<_>>());
        let resonance_avg = front.iter().map(|(_, o)| o.f_field).sum::<f64>() / front.len() as f64;
        let pareto_mean_nn = mean_nn_dist_norm(&front_norm, &stats.weights);
        let pareto_spacing = spacing_norm(&front_norm, &stats.weights);
        let pareto_hv_2d = pareto_hv_2d_norm(&front_norm);
        // unique_norm_vec_count is used in TraceRow, computing it here
        let unique_norm_vec_count = count_unique_norm(&front_norm, &stats.weights);
        let s_count = stats.active_dims.iter().zip(stats.weak_dims.iter()).filter(|&(&a, &w)| a && !w).count();
        let w_count = stats.weak_dims.iter().filter(|&&w| w).count();
        let effective_dim_count = s_count + w_count;
        let weak_dim_count = w_count;
        let weak_contrib_ratio = if w_count > 0 {
            norm_alpha_val * (w_count as f64) / ( (s_count as f64) + norm_alpha_val * (w_count as f64) )
        } else {
            0.0
        };
        let collapse_proxy = if depth > warmup_depths && front.len() > 1 && pareto_mean_nn < 0.01 { 1.0 } else { 0.0 };

        // Stability V3 Integration for run_search_phase1
        let norm_data: Vec<[f64; 4]> = normalized.iter().map(|(_, obj)| obj_to_arr(obj)).collect();
        let stability_metrics = ObjectiveStabilityAnalyzer::analyze(
            &norm_data,
            &stats.mad,
            unique_norm_vec_count,
            pareto_mean_nn,
        );

        if config.adaptive_alpha && depth > warmup_depths {
            adaptive_state = calculate_adaptive_alpha(
                &adaptive_state,
                &stats,
                pareto_mean_nn,
                front.len(),
                0.01, // d_target default
                stability_metrics.effective_dim,
            );
        }
        let norm_dim_mad_zero_count = stats.mad.iter().filter(|&&m| m.abs() < 1e-9).count();

        // Calculate raw metrics
        let front_refs: Vec<&ObjectiveVector> = front.iter().map(|(_, o)| o).collect();
        let pareto_mean_nn_raw = pareto_mean_nn_distance(&front_refs);
        let pareto_spacing_raw = pareto_spacing_metric(&front_refs);

        let calls_end = DISTANCE_CALL_COUNT.load(Ordering::Relaxed);
        let nn_calls_end = NN_DISTANCE_CALL_COUNT.load(Ordering::Relaxed);
        let distance_calls = calls_end.saturating_sub(calls_start);
        let nn_distance_calls = nn_calls_end.saturating_sub(nn_calls_start);

        rows.push(TraceRow {
            depth,
            lambda: lambda as f32,
            delta_lambda: (lambda - lambda_old) as f32,
            tau_prime: 0.0,
            conf_chm: 0.0,
            density: 0.0,
            k: 0,
            h_profile: 1.0,
            pareto_size: front.len(),
            diversity: depth_boundary_diversity as f32,
            resonance_avg: resonance_avg as f32,
            pressure: 0.0,
            epsilon_effect: 0.0,
            target_local_weight: 0.0,
            target_global_weight: 0.0,
            local_global_distance: 0.0,
            field_min_distance: 0.0,
            field_rejected_count: 0,
            mu: mu as f32,
            dhm_k: 0,
            dhm_norm: 0.0,
            dhm_resonance_mean: 0.0,
            dhm_score_ratio: 1.0,
            dhm_build_us: 0.0,
            expanded_categories_count,
            selected_rules_count: depth_selected_rules_count,
            per_category_selected,
            entropy_per_depth,
            unique_category_count_per_depth: expanded_categories_count,
            pareto_front_size_per_depth: front.len(),
            pareto_mean_nn_dist: pareto_mean_nn as f32,
            pareto_spacing: pareto_spacing as f32,
            pareto_hv_2d: pareto_hv_2d as f32,
            field_extract_us: field_extract_us as f32,
            field_score_us: field_score_us as f32,
            alpha_t: norm_alpha_val as f32,
            weak_contrib_ratio: weak_contrib_ratio as f32,
            collapse_proxy: collapse_proxy as f32,
            field_aggregate_us: field_aggregate_us as f32,
            field_total_us: field_total_us as f32,
            norm_median_0: stats.median[0] as f32,
            norm_median_1: stats.median[1] as f32,
            norm_median_2: stats.median[2] as f32,
            norm_median_3: stats.median[3] as f32,
            norm_mad_0: stats.mad[0] as f32,
            norm_mad_1: stats.mad[1] as f32,
            norm_mad_2: stats.mad[2] as f32,
            norm_mad_3: stats.mad[3] as f32,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: false,
            normalization_mode: "global_robust".to_string(),
            unique_norm_vec_count,
            norm_dim_mad_zero_count,
            mean_nn_dist_raw: pareto_mean_nn_raw as f32,
            mean_nn_dist_norm: pareto_mean_nn as f32,
            pareto_spacing_raw: pareto_spacing_raw as f32,
            pareto_spacing_norm: pareto_spacing as f32,
            distance_calls,
            nn_distance_calls,
            weak_dim_count,
            effective_dim_count,
            redundancy_flags: stability_metrics.redundancy_flags.join("|"),
            saturation_flags: stability_metrics.saturation_flags.join("|"),
            discrete_saturation_count: stability_metrics.discrete_saturation_count,
            effective_dim: stability_metrics.effective_dim,
            effective_dim_ratio: stability_metrics.effective_dim_ratio as f32,
            collapse_reasons: stability_metrics.collapse_reasons.join("|"),
        });

        frontier = select_beam_maxmin_norm(front, front_norm, config.beam.max(1), &stats.weights)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(config.seed)];
        }
    }

    let all_nn = rows
        .iter()
        .map(|r| r.pareto_mean_nn_dist as f64)
        .filter(|v| *v > 0.0)
        .collect::<Vec<_>>();
    let d_med = median(all_nn);
    for row in &mut rows {
        row.median_nn_dist_all_depth = d_med as f32;
        row.collapse_flag =
            (row.pareto_mean_nn_dist as f64) < 0.01 * d_med && row.pareto_front_size_per_depth >= 2;
    }
    rows
}

pub fn run_bench(config: BenchConfig) -> BenchResult {
    let iterations = config.iterations.max(1);
    let warmup = config.warmup;

    for i in 0..warmup {
        let _ = run_bench_once(config.depth, config.beam, config.seed.wrapping_add(i as u64), config.norm_alpha);
    }

    let mut total_ms_sum = 0.0;
    let mut per_depth_ms_sum = 0.0;
    let mut field_us_sum = 0.0;
    let mut resonance_us_sum = 0.0;
    let mut chm_us_sum = 0.0;
    let mut dhm_us_sum = 0.0;
    let mut pareto_us_sum = 0.0;
    let mut lambda_us_sum = 0.0;
    let mut lambda_final_sum = 0.0;

    for i in 0..iterations {
        let stats = run_bench_once(config.depth, config.beam, config.seed.wrapping_add(i as u64), config.norm_alpha);
        total_ms_sum += stats.total_ms;
        per_depth_ms_sum += stats.per_depth_ms;
        field_us_sum += stats.field_us;
        resonance_us_sum += stats.resonance_us;
        chm_us_sum += stats.chm_us;
        dhm_us_sum += stats.dhm_us;
        pareto_us_sum += stats.pareto_us;
        lambda_us_sum += stats.lambda_us;
        lambda_final_sum += stats.lambda_final;
    }

    let denom = iterations as f64;
    BenchResult {
        depth: config.depth,
        beam: config.beam,
        iterations,
        avg_total_ms: total_ms_sum / denom,
        avg_per_depth_ms: per_depth_ms_sum / denom,
        avg_field_us: field_us_sum / denom,
        avg_resonance_us: resonance_us_sum / denom,
        avg_chm_us: chm_us_sum / denom,
        avg_dhm_us: dhm_us_sum / denom,
        avg_pareto_us: pareto_us_sum / denom,
        avg_lambda_us: lambda_us_sum / denom,
        lambda_final: lambda_final_sum / denom,
    }
}

pub fn run_bench_baseline_off(config: BenchConfig) -> BenchResult {
    let iterations = config.iterations.max(1);
    let warmup = config.warmup;

    for i in 0..warmup {
        let _ = run_bench_once_baseline_off(config.depth, config.beam, config.seed.wrapping_add(i as u64), config.norm_alpha);
    }

    let mut total_ms_sum = 0.0;
    let mut per_depth_ms_sum = 0.0;
    let mut field_us_sum = 0.0;
    let mut resonance_us_sum = 0.0;
    let mut chm_us_sum = 0.0;
    let mut pareto_us_sum = 0.0;
    let mut lambda_us_sum = 0.0;
    let mut lambda_final_sum = 0.0;

    for i in 0..iterations {
        let stats = run_bench_once_baseline_off(config.depth, config.beam, config.seed.wrapping_add(i as u64), config.norm_alpha);
        total_ms_sum += stats.total_ms;
        per_depth_ms_sum += stats.per_depth_ms;
        field_us_sum += stats.field_us;
        resonance_us_sum += stats.resonance_us;
        chm_us_sum += stats.chm_us;
        pareto_us_sum += stats.pareto_us;
        lambda_us_sum += stats.lambda_us;
        lambda_final_sum += stats.lambda_final;
    }

    let denom = iterations as f64;
    BenchResult {
        depth: config.depth,
        beam: config.beam,
        iterations,
        avg_total_ms: total_ms_sum / denom,
        avg_per_depth_ms: per_depth_ms_sum / denom,
        avg_field_us: field_us_sum / denom,
        avg_resonance_us: resonance_us_sum / denom,
        avg_chm_us: chm_us_sum / denom,
        avg_dhm_us: 0.0,
        avg_pareto_us: pareto_us_sum / denom,
        avg_lambda_us: lambda_us_sum / denom,
        lambda_final: lambda_final_sum / denom,
    }
}

pub fn run_bench_baseline_off_balanced(config: BenchConfig, m: usize) -> BenchResult {
    let iterations = config.iterations.max(1);
    let warmup = config.warmup;

    for i in 0..warmup {
        let _ = run_bench_once_baseline_off_balanced(
            config.depth,
            config.beam,
            config.seed.wrapping_add(i as u64),
            m,
            config.norm_alpha,
        );
    }

    let mut total_ms_sum = 0.0;
    let mut per_depth_ms_sum = 0.0;
    let mut field_us_sum = 0.0;
    let mut resonance_us_sum = 0.0;
    let mut chm_us_sum = 0.0;
    let mut pareto_us_sum = 0.0;
    let mut lambda_us_sum = 0.0;
    let mut lambda_final_sum = 0.0;

    for i in 0..iterations {
        let stats = run_bench_once_baseline_off_balanced(
            config.depth,
            config.beam,
            config.seed.wrapping_add(i as u64),
            m,
            config.norm_alpha,
        );
        total_ms_sum += stats.total_ms;
        per_depth_ms_sum += stats.per_depth_ms;
        field_us_sum += stats.field_us;
        resonance_us_sum += stats.resonance_us;
        chm_us_sum += stats.chm_us;
        pareto_us_sum += stats.pareto_us;
        lambda_us_sum += stats.lambda_us;
        lambda_final_sum += stats.lambda_final;
    }

    let denom = iterations as f64;
    BenchResult {
        depth: config.depth,
        beam: config.beam,
        iterations,
        avg_total_ms: total_ms_sum / denom,
        avg_per_depth_ms: per_depth_ms_sum / denom,
        avg_field_us: field_us_sum / denom,
        avg_resonance_us: resonance_us_sum / denom,
        avg_chm_us: chm_us_sum / denom,
        avg_dhm_us: 0.0,
        avg_pareto_us: pareto_us_sum / denom,
        avg_lambda_us: lambda_us_sum / denom,
        lambda_final: lambda_final_sum / denom,
    }
}

pub fn run_bench_baseline_off_soft(
    config: BenchConfig,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
    lambda_min: f64,
    lambda_target_entropy: f64,
    lambda_k: f64,
    lambda_ema: f64,
    field_profile: bool,
) -> BenchResult {
    let iterations = config.iterations.max(1);
    let warmup = config.warmup;

    for i in 0..warmup {
        let _ = run_bench_once_baseline_off_soft(
            config.depth,
            config.beam,
            config.seed.wrapping_add(i as u64),
            config.norm_alpha,
            alpha,
            temperature,
            entropy_beta,
            lambda_min,
            lambda_target_entropy,
            lambda_k,
            lambda_ema,
            field_profile,
        );
    }

    let mut total_ms_sum = 0.0;
    let mut per_depth_ms_sum = 0.0;
    let mut field_us_sum = 0.0;
    let mut resonance_us_sum = 0.0;
    let mut chm_us_sum = 0.0;
    let mut pareto_us_sum = 0.0;
    let mut lambda_us_sum = 0.0;
    let mut lambda_final_sum = 0.0;

    for i in 0..iterations {
        let stats = run_bench_once_baseline_off_soft(
            config.depth,
            config.beam,
            config.seed.wrapping_add(i as u64),
            config.norm_alpha,
            alpha,
            temperature,
            entropy_beta,
            lambda_min,
            lambda_target_entropy,
            lambda_k,
            lambda_ema,
            field_profile,
        );
        total_ms_sum += stats.total_ms;
        per_depth_ms_sum += stats.per_depth_ms;
        field_us_sum += stats.field_us;
        resonance_us_sum += stats.resonance_us;
        chm_us_sum += stats.chm_us;
        pareto_us_sum += stats.pareto_us;
        lambda_us_sum += stats.lambda_us;
        lambda_final_sum += stats.lambda_final;
    }

    let denom = iterations as f64;
    BenchResult {
        depth: config.depth,
        beam: config.beam,
        iterations,
        avg_total_ms: total_ms_sum / denom,
        avg_per_depth_ms: per_depth_ms_sum / denom,
        avg_field_us: field_us_sum / denom,
        avg_resonance_us: resonance_us_sum / denom,
        avg_chm_us: chm_us_sum / denom,
        avg_dhm_us: 0.0,
        avg_pareto_us: pareto_us_sum / denom,
        avg_lambda_us: lambda_us_sum / denom,
        lambda_final: lambda_final_sum / denom,
    }
}

pub fn run_phase1_matrix(config: Phase1Config) -> (Vec<Phase1RawRow>, Vec<Phase1SummaryRow>) {
    let variants = [
        Phase1Variant::Base,
        Phase1Variant::Delta,
        Phase1Variant::Ortho { epsilon: 0.02 },
    ];
    let mut raw = Vec::new();
    let mut summary = Vec::new();
    for variant in variants {
        let (r, s) = run_phase1_variant(config, variant);
        raw.extend(r);
        summary.extend(s);
    }
    (raw, summary)
}

fn run_phase1_variant(config: Phase1Config, variant: Phase1Variant) -> (Vec<Phase1RawRow>, Vec<Phase1SummaryRow>) {
    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, config.seed);
    let field = FieldEngine::new(256);
    let structural = StructuralEvaluator::default();
    let mut frontier = vec![trace_initial_state(config.seed)];
    let mut lambda = 0.5f64;
    let mut field_cache: BTreeMap<(u128, u128, usize, usize), FieldVector> = BTreeMap::new();
    let mut field_cache_order: VecDeque<(u128, u128, usize, usize)> = VecDeque::new();
    let mut raw_rows = Vec::new();
    let mut summary_rows = Vec::new();

    for depth in 1..=config.depth.max(1) {
        let target_field = build_target_field(&field, &shm, &frontier[0], lambda);
        let mut depth_category_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut candidates: Vec<(DesignState, ObjectiveVector, RuleId)> = Vec::new();

        for (state_idx, state) in frontier.iter().enumerate() {
            let (selected_rules, _, _) = select_rules_category_soft(
                shm.applicable_rules(state),
                (config.beam.max(1) * 5).max(1),
                config.alpha,
                config.temperature,
                config.entropy_beta,
            );
            let current_obj = evaluate_state_for_phase1(state, &structural, &chm, &field, &target_field);
            for rule in selected_rules {
                *depth_category_counts
                    .entry(rule_category_name(&rule.category).to_string())
                    .or_insert(0) += 1;
                let new_state = apply_atomic(rule, state);
                let key = (new_state.id.as_u128(), rule.id.as_u128(), depth, state_idx);
                let projection =
                    bounded_cache_get_or_insert(&mut field_cache, &mut field_cache_order, key, || field.aggregate_state(&new_state)).0;
                let mut obj = structural.evaluate(&new_state);
                obj.f_risk = risk_score_from_chm(&new_state, &chm);
                obj.f_field = resonance_score(&projection, &target_field);
                let obj = match variant {
                    Phase1Variant::Base => obj.clamped(),
                    Phase1Variant::Delta => objective_delta(&obj, &current_obj),
                    Phase1Variant::Ortho { epsilon } => objective_with_ortho(&new_state, obj, epsilon),
                };
                candidates.push((new_state, obj, rule.id));
            }
        }

        if candidates.is_empty() {
            break;
        }

        let normalized_depth = normalize_phase1_vectors(&candidates.iter().map(|(_, o, _)| o.clone()).collect::<Vec<_>>());
        let mut pareto = ParetoFront::new();
        for (state, obj, _) in &candidates {
            pareto.insert(state.id, obj.clone());
        }
        let front_set: BTreeSet<Uuid> = pareto.get_front().into_iter().collect();
        let mut front: Vec<(DesignState, ObjectiveVector, RuleId)> = candidates
            .into_iter()
            .filter(|(s, _, _)| front_set.contains(&s.id))
            .collect();
        front.sort_by(|(ls, lo, _), (rs, ro, _)| {
            scalar_score(ro)
                .partial_cmp(&scalar_score(lo))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ls.id.cmp(&rs.id))
        });
        front.dedup_by(|a, b| a.0.id == b.0.id);

        let normalized_front = normalize_phase1_vectors(&front.iter().map(|(_, o, _)| o.clone()).collect::<Vec<_>>());
        let corr = corr_matrix4(&normalized_front);
        let mean_nn = mean_nn_dist4(&normalized_front);
        let spacing = spacing4(&normalized_front);
        let collapse_flag = mean_nn < 1e-4 && front.len() >= 2;
        summary_rows.push(Phase1SummaryRow {
            variant: variant.name().to_string(),
            depth,
            corr_matrix_flat: flatten_corr4(&corr),
            mean_nn_dist: mean_nn,
            spacing,
            pareto_front_size: front.len(),
            collapse_flag,
        });

        let beam_take = config.beam.max(1).min(front.len());
        let mut id_to_norm: BTreeMap<u128, [f64; 4]> = BTreeMap::new();
        for ((state, _, _), norm) in front.iter().zip(normalized_front.iter()) {
            id_to_norm.insert(state.id.as_u128(), *norm);
        }
        for (beam_index, (state, obj, rid)) in front.iter().take(beam_take).enumerate() {
            let norm = id_to_norm.get(&state.id.as_u128()).copied().unwrap_or([0.0; 4]);
            raw_rows.push(Phase1RawRow {
                variant: variant.name().to_string(),
                depth,
                beam_index,
                rule_id: format!("{:032x}", rid.as_u128()),
                objective_vector_raw: fmt_vec4(&obj_to_arr(obj)),
                objective_vector_norm: fmt_vec4(&norm),
            });
        }

        let entropy = shannon_entropy_from_counts(&depth_category_counts);
        lambda = update_lambda_entropy(
            lambda,
            entropy,
            config.lambda_target_entropy,
            config.lambda_k,
            config.lambda_ema,
            config.lambda_min,
            1.0,
        );
        frontier = front.into_iter().take(beam_take).map(|(s, _, _)| s).collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(config.seed)];
        }
        let _ = normalized_depth;
    }

    (raw_rows, summary_rows)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BenchOnceStats {
    total_ms: f64,
    per_depth_ms: f64,
    field_us: f64,
    resonance_us: f64,
    chm_us: f64,
    dhm_us: f64,
    pareto_us: f64,
    lambda_us: f64,
    lambda_final: f64,
}

fn run_bench_once(depth: usize, beam: usize, seed: u64, norm_alpha: f64) -> BenchOnceStats {
    let depth = depth.max(1);
    let beam = beam.max(1);

    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, seed);
    let field = FieldEngine::new(256);
    let structural = StructuralEvaluator::default();
    let mut controller = Phase45Controller::new(0.5);
    let mut frontier = vec![trace_initial_state(seed)];
    let dhm_config = DhMConfig::phase7_fixed();
    let mut dhm_memory = vec![(0usize, field.aggregate_state(&frontier[0]))];

    let n_edge_obs = chm.rule_graph.values().map(|v| v.len()).sum::<usize>();
    let mut conflict_hist = Vec::new();
    let mut align_hist = Vec::new();

    let mut field_us_total = 0.0;
    let mut resonance_us_total = 0.0;
    let mut chm_us_total = 0.0;
    let mut dhm_us_total = 0.0;
    let mut pareto_us_total = 0.0;
    let mut lambda_us_total = 0.0;
    let mut depth_count = 0usize;
    let mut depth_boundary_diversity = 1.0f64;

    let t_total = Instant::now();
    for d in 1..=depth {
        depth_count += 1;
        controller.on_profile_update(d, 0.25, ProfileUpdateType::TypeCStatistical);
        let mu = dhm_config.mu_at_depth(d);
        let t_dhm = Instant::now();
        let (dhm_field, _dhm_norm) = build_dhm_field(
            &dhm_memory,
            d,
            dhm_config.gamma as f64,
            dhm_config.k_nearest,
            field.dimensions(),
        );
        dhm_us_total += elapsed_us(t_dhm);

        let (target_field, _adjustment) = build_target_field_with_diversity(
            &field,
            &shm,
            &frontier[0],
            controller.lambda(),
            depth_boundary_diversity,
        );
        let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
        for state in &frontier {
            let mut ranked_rules = shm
                .applicable_rules(state)
                .into_iter()
                .map(|rule| {
                    let r_dhm = dhm_rule_resonance(rule, &field, &dhm_field);
                    let score_ratio = 1.0 + mu * r_dhm;
                    let score = rule.priority.max(0.0) * score_ratio;
                    (rule, score)
                })
                .collect::<Vec<_>>();
            ranked_rules.sort_by(|(l_rule, l_score), (r_rule, r_score)| {
                r_score
                    .partial_cmp(l_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| l_rule.id.cmp(&r_rule.id))
            });
            for (rule, _) in ranked_rules {
                let new_state = apply_atomic(rule, state);
                let mut obj = structural.evaluate(&new_state);

                let t_field = Instant::now();
                let projection = field.aggregate_state(&new_state);
                field_us_total += elapsed_us(t_field);

                let t_res = Instant::now();
                obj.f_field = resonance_score(&projection, &target_field);
                resonance_us_total += elapsed_us(t_res);

                let t_chm = Instant::now();
                obj.f_risk = risk_score_from_chm(&new_state, &chm);
                chm_us_total += elapsed_us(t_chm);

                candidates.push((new_state, obj.clamped()));
            }
        }

        if candidates.is_empty() {
            frontier = vec![trace_initial_state(seed)];
            continue;
        }

        let t_pareto = Instant::now();
        let (filtered_candidates, _field_min_distance, _field_rejected_count) =
            filter_candidates_by_field_distance(candidates, &field, FIELD_DISTANCE_DELTA);
        if filtered_candidates.is_empty() {
            frontier = vec![trace_initial_state(seed)];
            continue;
        }
        let (normalized, _) = normalize_by_depth(filtered_candidates, norm_alpha);
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
        pareto_us_total += elapsed_us(t_pareto);
        depth_boundary_diversity = variance(&front.iter().map(|(_, o)| scalar_score(o)).collect::<Vec<_>>());

        let conflict_raw = front
            .iter()
            .map(|(_, o)| (1.0 - o.f_risk + 1.0 - o.f_cost) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        let align_raw = front
            .iter()
            .map(|(_, o)| (o.f_struct + o.f_field) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        conflict_hist.push(conflict_raw);
        align_hist.push(align_raw);
        let k = controller.k().max(1);
        let conflict_k = moving_average_tail(&conflict_hist, k);
        let align_k = moving_average_tail(&align_hist, k);

        let t_lambda = Instant::now();
        let _log = controller.update_depth(
            d,
            conflict_k,
            align_k,
            n_edge_obs,
            10,
            stability_index(0.25, 0.25, 0.0, 0.0),
        );
        lambda_us_total += elapsed_us(t_lambda);

        frontier = front.into_iter().take(beam).map(|(s, _)| s).collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(seed)];
        }
        for state in &frontier {
            dhm_memory.push((d, field.aggregate_state(state)));
        }
    }

    let total_ms = elapsed_ms(t_total);
    let per_depth_ms = total_ms / depth_count.max(1) as f64;
    let denom_depth = depth_count.max(1) as f64;
    BenchOnceStats {
        total_ms,
        per_depth_ms,
        field_us: field_us_total / denom_depth,
        resonance_us: resonance_us_total / denom_depth,
        chm_us: chm_us_total / denom_depth,
        dhm_us: dhm_us_total / denom_depth,
        pareto_us: pareto_us_total / denom_depth,
        lambda_us: lambda_us_total / denom_depth,
        lambda_final: controller.lambda(),
    }
}

fn run_bench_once_baseline_off(depth: usize, beam: usize, seed: u64, norm_alpha: f64) -> BenchOnceStats {
    let depth = depth.max(1);
    let beam = beam.max(1);

    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, seed);
    let field = FieldEngine::new(256);
    let structural = StructuralEvaluator::default();
    let mut controller = Phase45Controller::new(0.5);
    let mut frontier = vec![trace_initial_state(seed)];

    let n_edge_obs = chm.rule_graph.values().map(|v| v.len()).sum::<usize>();
    let mut conflict_hist = Vec::new();
    let mut align_hist = Vec::new();

    let mut field_us_total = 0.0;
    let mut resonance_us_total = 0.0;
    let mut chm_us_total = 0.0;
    let mut pareto_us_total = 0.0;
    let mut lambda_us_total = 0.0;
    let mut depth_count = 0usize;
    let mut depth_boundary_diversity = 1.0f64;

    let t_total = Instant::now();
    for d in 1..=depth {
        depth_count += 1;
        controller.on_profile_update(d, 0.25, ProfileUpdateType::TypeCStatistical);

        let target_field = build_target_field(
            &field,
            &shm,
            &frontier[0],
            controller.lambda(),
        );
        let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
        for state in &frontier {
            for rule in shm.applicable_rules(state) {
                let new_state = apply_atomic(rule, state);
                let mut obj = structural.evaluate(&new_state);

                let t_field = Instant::now();
                let projection = field.aggregate_state(&new_state);
                field_us_total += elapsed_us(t_field);

                let t_res = Instant::now();
                obj.f_field = resonance_score(&projection, &target_field);
                resonance_us_total += elapsed_us(t_res);

                let t_chm = Instant::now();
                obj.f_risk = risk_score_from_chm(&new_state, &chm);
                chm_us_total += elapsed_us(t_chm);

                candidates.push((new_state, obj.clamped()));
            }
        }

        if candidates.is_empty() {
            frontier = vec![trace_initial_state(seed)];
            continue;
        }

        let t_pareto = Instant::now();
        let (normalized, _) = normalize_by_depth(candidates, norm_alpha);
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
        pareto_us_total += elapsed_us(t_pareto);
        depth_boundary_diversity = variance(&front.iter().map(|(_, o)| scalar_score(o)).collect::<Vec<_>>());

        let conflict_raw = front
            .iter()
            .map(|(_, o)| (1.0 - o.f_risk + 1.0 - o.f_cost) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        let align_raw = front
            .iter()
            .map(|(_, o)| (o.f_struct + o.f_field) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        conflict_hist.push(conflict_raw);
        align_hist.push(align_raw);
        let k = controller.k().max(1);
        let conflict_k = moving_average_tail(&conflict_hist, k);
        let align_k = moving_average_tail(&align_hist, k);

        let t_lambda = Instant::now();
        let _log = controller.update_depth(
            d,
            conflict_k,
            align_k,
            n_edge_obs,
            10,
            stability_index(0.25, 0.25, 0.0, 0.0),
        );
        lambda_us_total += elapsed_us(t_lambda);

        frontier = front.into_iter().take(beam).map(|(s, _)| s).collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(seed)];
        }
    }

    let total_ms = elapsed_ms(t_total);
    let per_depth_ms = total_ms / depth_count.max(1) as f64;
    let denom_depth = depth_count.max(1) as f64;
    let _ = depth_boundary_diversity;
    BenchOnceStats {
        total_ms,
        per_depth_ms,
        field_us: field_us_total / denom_depth,
        resonance_us: resonance_us_total / denom_depth,
        chm_us: chm_us_total / denom_depth,
        dhm_us: 0.0,
        pareto_us: pareto_us_total / denom_depth,
        lambda_us: lambda_us_total / denom_depth,
        lambda_final: controller.lambda(),
    }
}

fn run_bench_once_baseline_off_balanced(depth: usize, beam: usize, seed: u64, m: usize, norm_alpha: f64) -> BenchOnceStats {
    let depth = depth.max(1);
    let beam = beam.max(1);

    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, seed);
    let field = FieldEngine::new(256);
    let structural = StructuralEvaluator::default();
    let mut controller = Phase45Controller::new(0.5);
    let mut frontier = vec![trace_initial_state(seed)];

    let n_edge_obs = chm.rule_graph.values().map(|v| v.len()).sum::<usize>();
    let mut conflict_hist = Vec::new();
    let mut align_hist = Vec::new();

    let mut field_us_total = 0.0;
    let mut resonance_us_total = 0.0;
    let mut chm_us_total = 0.0;
    let mut pareto_us_total = 0.0;
    let mut lambda_us_total = 0.0;
    let mut depth_count = 0usize;

    let t_total = Instant::now();
    for d in 1..=depth {
        depth_count += 1;
        controller.on_profile_update(d, 0.25, ProfileUpdateType::TypeCStatistical);

        let target_field = build_target_field(
            &field,
            &shm,
            &frontier[0],
            controller.lambda(),
        );
        let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
        for state in &frontier {
            let (selected_rules, _) = select_rules_category_balanced(shm.applicable_rules(state), m);
            for rule in selected_rules {
                let new_state = apply_atomic(rule, state);
                let mut obj = structural.evaluate(&new_state);

                let t_field = Instant::now();
                let projection = field.aggregate_state(&new_state);
                field_us_total += elapsed_us(t_field);

                let t_res = Instant::now();
                obj.f_field = resonance_score(&projection, &target_field);
                resonance_us_total += elapsed_us(t_res);

                let t_chm = Instant::now();
                obj.f_risk = risk_score_from_chm(&new_state, &chm);
                chm_us_total += elapsed_us(t_chm);

                candidates.push((new_state, obj.clamped()));
            }
        }

        if candidates.is_empty() {
            frontier = vec![trace_initial_state(seed)];
            continue;
        }

        let t_pareto = Instant::now();
        let (normalized, _) = normalize_by_depth(candidates, norm_alpha);
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
        pareto_us_total += elapsed_us(t_pareto);

        let conflict_raw = front
            .iter()
            .map(|(_, o)| (1.0 - o.f_risk + 1.0 - o.f_cost) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        let align_raw = front
            .iter()
            .map(|(_, o)| (o.f_struct + o.f_field) * 0.5)
            .sum::<f64>()
            / front.len() as f64;
        conflict_hist.push(conflict_raw);
        align_hist.push(align_raw);
        let k = controller.k().max(1);
        let conflict_k = moving_average_tail(&conflict_hist, k);
        let align_k = moving_average_tail(&align_hist, k);

        let t_lambda = Instant::now();
        let _log = controller.update_depth(
            d,
            conflict_k,
            align_k,
            n_edge_obs,
            10,
            stability_index(0.25, 0.25, 0.0, 0.0),
        );
        lambda_us_total += elapsed_us(t_lambda);

        frontier = front.into_iter().take(beam).map(|(s, _)| s).collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(seed)];
        }
    }

    let total_ms = elapsed_ms(t_total);
    let per_depth_ms = total_ms / depth_count.max(1) as f64;
    let denom_depth = depth_count.max(1) as f64;
    BenchOnceStats {
        total_ms,
        per_depth_ms,
        field_us: field_us_total / denom_depth,
        resonance_us: resonance_us_total / denom_depth,
        chm_us: chm_us_total / denom_depth,
        dhm_us: 0.0,
        pareto_us: pareto_us_total / denom_depth,
        lambda_us: lambda_us_total / denom_depth,
        lambda_final: controller.lambda(),
    }
}

fn run_bench_once_baseline_off_soft(
    depth: usize,
    beam: usize,
    seed: u64,
    norm_alpha: f64,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
    lambda_min: f64,
    lambda_target_entropy: f64,
    lambda_k: f64,
    lambda_ema: f64,
    field_profile: bool,
) -> BenchOnceStats {
    let depth = depth.max(1);
    let beam = beam.max(1);
    let shm = Shm::with_default_rules();
    let chm = make_dense_trace_chm(&shm, seed);
    let field = FieldEngine::new(256);
    let structural = StructuralEvaluator::default();
    let mut frontier = vec![trace_initial_state(seed)];
    let mut lambda = 0.5f64;
    let mut field_cache: BTreeMap<(u128, u128, usize, usize), FieldVector> = BTreeMap::new();
    let mut field_cache_order: VecDeque<(u128, u128, usize, usize)> = VecDeque::new();
    let mut estimator = GlobalRobustEstimator::default();
    let warmup_depths = 10usize;

    let mut field_us_total = 0.0;
    let mut resonance_us_total = 0.0;
    let mut chm_us_total = 0.0;
    let mut pareto_us_total = 0.0;
    let mut lambda_us_total = 0.0;
    let mut depth_count = 0usize;

    let t_total = Instant::now();
    for d in 1..=depth {
        depth_count += 1;
        let target_field = build_target_field(&field, &shm, &frontier[0], lambda);
        let batch = build_soft_candidates_for_frontier(
            &frontier,
            beam,
            d,
            alpha,
            temperature,
            entropy_beta,
            &field,
            &shm,
            &chm,
            &structural,
            &target_field,
            field_profile,
            &mut field_cache,
            &mut field_cache_order,
        );
        let candidates = batch.candidates;
        field_us_total += batch.field_extract_us + batch.field_aggregate_us;
        resonance_us_total += batch.field_score_us;
        chm_us_total += batch.chm_us;
        if d <= warmup_depths {
            estimator
                .samples
                .extend(candidates.iter().map(|(_, o)| ObjectiveRaw(obj_to_arr(o))));
            if d == warmup_depths {
                estimator.frozen = robust_stats_from_samples(&estimator.samples, norm_alpha);
            }
        }
        let stats = estimator
            .frozen.clone()
            .or_else(|| robust_stats_from_samples(&estimator.samples, norm_alpha))
            .unwrap_or(GlobalRobustStats {
                alpha_used: norm_alpha,
                median: [0.0; 4],
                mad: [1.0; 4],
                mean: [0.0; 4],
                std: [1.0; 4],
                active_dims: [true; 4],
                weak_dims: [false; 4],
                weights: [1.0; 4],
                mad_zero_count: 0,
            });
        if candidates.is_empty() {
            frontier = vec![trace_initial_state(seed)];
            continue;
        }

        let t_pareto = Instant::now();
        let (normalized, stats) = normalize_by_depth(candidates, norm_alpha);
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
        pareto_us_total += elapsed_us(t_pareto);
        let front_norm = front
            .iter()
            .map(|(_, o)| normalize_objective(&ObjectiveRaw(obj_to_arr(o)), &stats))
            .collect::<Vec<_>>();

        let entropy = shannon_entropy_from_counts(&batch.depth_category_counts);
        let t_lambda = Instant::now();
        lambda = update_lambda_entropy(
            lambda,
            entropy,
            lambda_target_entropy,
            lambda_k,
            lambda_ema,
            lambda_min,
            1.0,
        );
        lambda_us_total += elapsed_us(t_lambda);

        frontier = select_beam_maxmin_norm(front, front_norm, beam, &stats.weights)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        if frontier.is_empty() {
            frontier = vec![trace_initial_state(seed)];
        }
    }

    let total_ms = elapsed_ms(t_total);
    let per_depth_ms = total_ms / depth_count.max(1) as f64;
    let denom_depth = depth_count.max(1) as f64;
    BenchOnceStats {
        total_ms,
        per_depth_ms,
        field_us: field_us_total / denom_depth,
        resonance_us: resonance_us_total / denom_depth,
        chm_us: chm_us_total / denom_depth,
        dhm_us: 0.0,
        pareto_us: pareto_us_total / denom_depth,
        lambda_us: lambda_us_total / denom_depth,
        lambda_final: lambda,
    }
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

            let (normalized, _) = normalize_by_depth(candidates, self.config.norm_alpha);

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

fn filter_candidates_by_field_distance(
    candidates: Vec<(DesignState, ObjectiveVector)>,
    field: &FieldEngine,
    delta: f64,
) -> (Vec<(DesignState, ObjectiveVector)>, f64, usize) {
    let delta = delta.max(0.0);
    let mut accepted: Vec<(DesignState, ObjectiveVector, FieldVector)> = Vec::new();
    let mut rejected_count = 0usize;
    let mut min_distance_seen = f64::INFINITY;

    for (state, obj) in candidates {
        let projection = field.aggregate_state(&state);
        let mut min_d = f64::INFINITY;
        for (_, _, existing_projection) in &accepted {
            let d = field_l2_distance(&projection, existing_projection);
            min_d = min_d.min(d);
        }

        if min_d.is_finite() {
            min_distance_seen = min_distance_seen.min(min_d);
        }

        if min_d < delta {
            rejected_count += 1;
            continue;
        }
        accepted.push((state, obj, projection));
    }

    let filtered = accepted
        .into_iter()
        .map(|(state, obj, _)| (state, obj))
        .collect::<Vec<_>>();

    let min_distance = if min_distance_seen.is_finite() {
        min_distance_seen
    } else {
        0.0
    };

    (filtered, min_distance, rejected_count)
}

fn field_l2_distance(a: &FieldVector, b: &FieldVector) -> f64 {
    let len = a.dimensions().min(b.dimensions());
    let mut sum = 0.0f64;
    for i in 0..len {
        let diff = a.data[i] - b.data[i];
        sum += diff.norm_sqr() as f64;
    }
    sum.sqrt()
}



fn median_absolute_deviation(sorted_values: &[f64], median: f64) -> f64 {
    let mut devs: Vec<f64> = sorted_values.iter().map(|v| (v - median).abs()).collect();
    devs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    devs[devs.len() / 2]
}

fn normalize_with_mad(candidates: Vec<(DesignState, ObjectiveVector)>) -> (Vec<(DesignState, ObjectiveVector)>, usize) {
    if candidates.is_empty() {
        return (candidates, 0);
    }

    let n = candidates.len();
    let mut f_structs: Vec<f64> = candidates.iter().map(|(_, o)| o.f_struct).collect();
    let mut f_fields: Vec<f64> = candidates.iter().map(|(_, o)| o.f_field).collect();
    let mut f_risks: Vec<f64> = candidates.iter().map(|(_, o)| o.f_risk).collect();
    let mut f_costs: Vec<f64> = candidates.iter().map(|(_, o)| o.f_cost).collect();

    f_structs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    f_fields.sort_by(|a, b| a.partial_cmp(b).unwrap());
    f_risks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    f_costs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let median_struct = f_structs[n / 2];
    let median_field = f_fields[n / 2];
    let median_risk = f_risks[n / 2];
    let median_cost = f_costs[n / 2];

    let mad_struct = median_absolute_deviation(&f_structs, median_struct);
    let mad_field = median_absolute_deviation(&f_fields, median_field);
    let mad_risk = median_absolute_deviation(&f_risks, median_risk);
    let mad_cost = median_absolute_deviation(&f_costs, median_cost);

    let mad_floor = 1e-9;
    let mut mad_zero_count = 0;
    if mad_struct < mad_floor { mad_zero_count += 1; }
    if mad_field < mad_floor { mad_zero_count += 1; }
    if mad_risk < mad_floor { mad_zero_count += 1; }
    if mad_cost < mad_floor { mad_zero_count += 1; }

    let norm_struct_mad = mad_struct.max(mad_floor);
    let norm_field_mad = mad_field.max(mad_floor);
    let norm_risk_mad = mad_risk.max(mad_floor);
    let norm_cost_mad = mad_cost.max(mad_floor);

    let normalized = candidates.into_iter().map(|(s, o)| {
        (s, ObjectiveVector {
            f_struct: (o.f_struct - median_struct) / norm_struct_mad,
            f_field: (o.f_field - median_field) / norm_field_mad,
            f_risk: (o.f_risk - median_risk) / norm_risk_mad,
            f_cost: (o.f_cost - median_cost) / norm_cost_mad,
        })
    }).collect();

    (normalized, mad_zero_count)
}

fn calculate_pareto_metrics(vectors: &[(StateId, ObjectiveVector)]) -> (f32, f32, usize) {
    if vectors.len() < 2 {
        return (0.0, 0.0, vectors.len());
    }

    let mut nn_dists = Vec::new();
    let mut unique_vecs: Vec<ObjectiveVector> = Vec::new();

    for (_, v) in vectors {
         let mut found = false;
         for existing in &unique_vecs {
             if objective_distance(v, existing) < 1e-9 {
                 found = true;
                 break;
             }
         }
         if !found {
             unique_vecs.push(v.clone());
         }
    }
    
    // We calculate NN on the Full set or Unique set? 
    // Spec: "mean_nn_dist_raw: raw space NN dist", "unique_norm_vec_count: unique count".
    // Usually NN is calculated on the unique set to avoid 0 distance.
    // If we have duplicates, NN distance would be 0, which might trigger Case B or others.
    // Spec Case B: unique_norm_vec_count > 1 AND mean_nn_dist_norm == 0.
    // If we have distinct clusters but duplicates within clusters, valid NN might be 0.
    // But "NN calculation implementation inconsistency" suggests we should handle this.
    // I will calculate NN on the provided vectors, allowing 0 if duplicates exist,
    // BUT `unique_norm_vec_count` will detect strict degeneration.
    // However, if unique_norm > 1 and mean_nn == 0, it means *everyone* has a duplicate 
    // OR the calculation is wrong.
    // Let's stick to calculating on `vectors` as is.
    
    for i in 0..vectors.len() {
        let mut min_dist = f64::MAX;
        for j in 0..vectors.len() {
            if i == j { continue; }
            let d = objective_distance(&vectors[i].1, &vectors[j].1);
            if d < min_dist {
                min_dist = d;
            }
        }
        NN_DISTANCE_CALL_COUNT.fetch_add(vectors.len() - 1, Ordering::Relaxed);
        nn_dists.push(min_dist);
    }

    let mean_nn = if nn_dists.is_empty() { 0.0 } else { nn_dists.iter().sum::<f64>() / nn_dists.len() as f64 };
    
    // Spacing
    let variance = if nn_dists.is_empty() { 0.0 } else { nn_dists.iter().map(|d| (d - mean_nn).powi(2)).sum::<f64>() / nn_dists.len() as f64 };
    let spacing = variance.sqrt();

    (mean_nn as f32, spacing as f32, unique_vecs.len())
}

fn normalize_by_depth(candidates: Vec<(DesignState, ObjectiveVector)>, alpha: f64) -> (Vec<(DesignState, ObjectiveVector)>, GlobalRobustStats) {
    if candidates.is_empty() {
        return (Vec::new(), GlobalRobustStats {
            median: [0.0; 4],
            mad: [1.0; 4],
            mean: [0.0; 4],
            std: [1.0; 4],
            active_dims: [true; 4],
            weak_dims: [false; 4],
            weights: [1.0; 4],
            mad_zero_count: 0,
            alpha_used: alpha,
        });
    }

    let n = candidates.len();
    let mut structs = Vec::with_capacity(n);
    let mut fields = Vec::with_capacity(n);
    let mut risks = Vec::with_capacity(n);
    let mut costs = Vec::with_capacity(n);

    for (_, obj) in &candidates {
        structs.push(obj.f_struct);
        fields.push(obj.f_field);
        risks.push(obj.f_risk);
        costs.push(obj.f_cost);
    }

    let med_s = median(structs.clone());
    let med_f = median(fields.clone());
    let med_r = median(risks.clone());
    let med_c = median(costs.clone());

    let mad_s = compute_mad(&structs, med_s);
    let mad_f = compute_mad(&fields, med_f);
    let mad_r = compute_mad(&risks, med_r);
    let mad_c = compute_mad(&costs, med_c);

    let mean_s = compute_mean(&structs);
    let mean_f = compute_mean(&fields);
    let mean_r = compute_mean(&risks);
    let mean_c = compute_mean(&costs);

    let std_s = compute_std(&structs, mean_s);
    let std_f = compute_std(&fields, mean_f);
    let std_r = compute_std(&risks, mean_r);
    let std_c = compute_std(&costs, mean_c);

    let eps_mad = 1e-12;
    let eps_std = 1e-9;
    let alpha_weak = alpha;

    let mut active = [false; 4];
    let mut weak = [false; 4];
    let mut weights = [0.0; 4];
    let mad = [mad_s, mad_f, mad_r, mad_c];
    let median = [med_s, med_f, med_r, med_c];
    let mean = [mean_s, mean_f, mean_r, mean_c];
    let std_dev = [std_s, std_f, std_r, std_c];

    for i in 0..4 {
        if mad[i] > eps_mad {
            // Strong Active
            active[i] = true;
            weak[i] = false;
            weights[i] = 1.0;
        } else if std_dev[i] > eps_std {
            // Weak Active
            active[i] = true;
            weak[i] = true;
            weights[i] = alpha_weak;
        } else {
            // Degenerate
            active[i] = false;
            weak[i] = false;
            weights[i] = 0.0;
        }
    }

    let mad_zero_count = active.iter().zip(mad.iter()).filter(|&(&a, &m)| a && m <= eps_mad).count();

    let stats = GlobalRobustStats {
        median,
        mad,
        mean,
        std: std_dev,
        active_dims: active,
        weak_dims: weak,
        weights,
        mad_zero_count,
        alpha_used: alpha_weak,
    };

    let normalized = candidates
        .into_iter()
        .map(|(state, obj)| {
            let mut f = [0.0; 4];
            let raw = [obj.f_struct, obj.f_field, obj.f_risk, obj.f_cost];
            for i in 0..4 {
                if active[i] {
                    if !weak[i] {
                        // Strong: (x - median) / MAD
                        // MAD is guaranteed > eps_mad
                        f[i] = (raw[i] - median[i]) / mad[i];
                    } else {
                        // Weak: (x - mean) / max(std, eps_std)
                        let den = std_dev[i].max(eps_std);
                        f[i] = (raw[i] - mean[i]) / den;
                    }
                } else {
                    f[i] = 0.0;
                }
            }
            (
                state,
                ObjectiveVector {
                    f_struct: f[0],
                    f_field: f[1],
                    f_risk: f[2],
                    f_cost: f[3],
                },
            )
        })
        .collect();

    (normalized, stats)
}

fn compute_mad(values: &[f64], med: f64) -> f64 {
    let diffs: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    median(diffs)
}

fn compute_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn compute_std(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    var.sqrt()
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
    let (target, _) = build_target_field_with_diversity(field, shm, state, lambda, 1.0);
    target
}

pub fn build_target_field_with_diversity(
    field: &FieldEngine,
    shm: &Shm,
    state: &DesignState,
    lambda: f64,
    diversity: f64,
) -> (TargetField, diversity::DiversityAdjustment) {
    let global_categories = categories_from_rules(shm.rules.iter().map(|r| r.category.clone()));
    let local_categories = categories_from_rules(
        shm.applicable_rules(state)
            .into_iter()
            .map(|rule| rule.category.clone()),
    );

    let global = compose_category_field(field, &global_categories);
    let local = compose_category_field(field, &local_categories);
    let base = TargetField::blend(&global, &local, lambda as f32);
    apply_diversity_pressure(&base, &global, &local, lambda, diversity)
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

fn elapsed_us(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000_000.0
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
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

fn rule_category_name(category: &RuleCategory) -> &'static str {
    match category {
        RuleCategory::Structural => "Structural",
        RuleCategory::Performance => "Performance",
        RuleCategory::Reliability => "Reliability",
        RuleCategory::Cost => "Cost",
        RuleCategory::Refactor => "Refactor",
        RuleCategory::ConstraintPropagation => "ConstraintPropagation",
    }
}

fn select_rules_category_balanced<'a>(
    rules: Vec<&'a DesignRule>,
    m: usize,
) -> (Vec<&'a DesignRule>, BTreeMap<String, usize>) {
    let per_category = m.max(1);
    if rules.is_empty() {
        return (Vec::new(), BTreeMap::new());
    }

    let mut ranked = rules;
    ranked.sort_by(|l, r| {
        r.priority
            .partial_cmp(&l.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| l.id.cmp(&r.id))
    });

    let mut by_cat: BTreeMap<String, Vec<&DesignRule>> = BTreeMap::new();
    for rule in ranked {
        by_cat
            .entry(rule_category_name(&rule.category).to_string())
            .or_default()
            .push(rule);
    }

    let mut selected = Vec::new();
    let mut per_cat_selected: BTreeMap<String, usize> = BTreeMap::new();
    for rules_cat in by_cat.values() {
        for rule in rules_cat.iter().take(per_category) {
            selected.push(*rule);
            let cat = rule_category_name(&rule.category).to_string();
            *per_cat_selected.entry(cat).or_insert(0) += 1;
        }
    }
    (selected, per_cat_selected)
}

#[derive(Default)]
struct SoftCandidateBatch {
    candidates: Vec<(DesignState, ObjectiveVector)>,
    depth_category_counts: BTreeMap<String, usize>,
    depth_selected_rules_count: usize,
    field_extract_us: f64,
    field_score_us: f64,
    field_aggregate_us: f64,
    field_total_us: f64,
    chm_us: f64,
}

type FieldCacheKey = (u128, u128, usize, usize);

fn bounded_cache_get_or_insert(
    cache: &mut BTreeMap<FieldCacheKey, FieldVector>,
    order: &mut VecDeque<FieldCacheKey>,
    key: FieldCacheKey,
    compute: impl FnOnce() -> FieldVector,
) -> (FieldVector, bool) {
    if let Some(found) = cache.get(&key) {
        return (found.clone(), true);
    }
    let value = compute();
    cache.insert(key, value.clone());
    order.push_back(key);
    while cache.len() > FIELD_CACHE_CAPACITY {
        if let Some(old) = order.pop_front() {
            cache.remove(&old);
        } else {
            break;
        }
    }
    (value, false)
}

fn build_soft_candidates_for_frontier(
    frontier: &[DesignState],
    beam: usize,
    depth: usize,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
    field: &FieldEngine,
    shm: &Shm,
    chm: &Chm,
    structural: &StructuralEvaluator,
    target_field: &TargetField,
    field_profile: bool,
    field_cache: &mut BTreeMap<FieldCacheKey, FieldVector>,
    field_cache_order: &mut VecDeque<FieldCacheKey>,
) -> SoftCandidateBatch {
    let mut batch = SoftCandidateBatch::default();
    let mut partials: Vec<(DesignState, ObjectiveVector, RuleId, usize, f64)> = Vec::new();

    for (state_idx, state) in frontier.iter().enumerate() {
        let (selected_rules, per_state_counts, _availability_counts) = select_rules_category_soft(
            shm.applicable_rules(state),
            (beam.max(1) * 5).max(1),
            alpha,
            temperature,
            entropy_beta,
        );
        batch.depth_selected_rules_count += selected_rules.len();
        for (cat, c) in per_state_counts {
            *batch.depth_category_counts.entry(cat).or_insert(0) += c;
        }
        for rule in selected_rules {
            let new_state = apply_atomic(rule, state);
            let mut obj = structural.evaluate(&new_state);
            let t_chm = Instant::now();
            obj.f_risk = risk_score_from_chm(&new_state, chm);
            batch.chm_us += elapsed_us(t_chm);
            obj.f_field = 0.0;
            let pre_score = 0.4 * obj.f_struct + 0.2 * obj.f_risk + 0.2 * obj.f_cost;
            partials.push((new_state, obj.clamped(), rule.id, state_idx, pre_score));
        }
    }

    partials.sort_by(|(ls, _, _, _, lscore), (rs, _, _, _, rscore)| {
        rscore
            .partial_cmp(lscore)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| ls.id.cmp(&rs.id))
    });

    let detailed_n = (beam.max(1) * 5).min(partials.len());
    for (idx, (state, obj, rule_id, state_idx, _)) in partials.iter_mut().enumerate() {
        if idx >= detailed_n {
            break;
        }
        let t_total = Instant::now();
        let key = (state.id.as_u128(), rule_id.as_u128(), depth, *state_idx);
        let t_extract = Instant::now();
        let t_agg = Instant::now();
        let (projection, cache_hit) =
            bounded_cache_get_or_insert(field_cache, field_cache_order, key, || field.aggregate_state(state));
        if field_profile && !cache_hit {
            batch.field_aggregate_us += elapsed_us(t_agg);
        }
        if field_profile {
            batch.field_extract_us += elapsed_us(t_extract);
        }
        let t_score = Instant::now();
        obj.f_field = resonance_score(&projection, target_field);
        if field_profile {
            batch.field_score_us += elapsed_us(t_score);
            batch.field_total_us += elapsed_us(t_total);
        }
        *obj = obj.clone().clamped();
    }

    batch.candidates = partials.into_iter().map(|(s, o, _, _, _)| (s, o)).collect();
    batch
}

fn select_rules_category_soft<'a>(
    rules: Vec<&'a DesignRule>,
    max_select: usize,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
) -> (Vec<&'a DesignRule>, BTreeMap<String, usize>, BTreeMap<String, usize>) {
    if rules.is_empty() {
        return (Vec::new(), BTreeMap::new(), BTreeMap::new());
    }

    let mut availability_counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in &rules {
        *availability_counts
            .entry(rule_category_name(&rule.category).to_string())
            .or_insert(0) += 1;
    }

    let n_total = rules.len() as f64;
    let k = availability_counts.len().max(1) as f64;
    let uniform = 1.0 / k;
    let entropy = shannon_entropy_from_counts(&availability_counts);
    let t = temperature.max(1e-6);

    let mut scored: Vec<(&DesignRule, f64)> = rules
        .into_iter()
        .map(|rule| {
            let cat = rule_category_name(&rule.category).to_string();
            let p_i = *availability_counts.get(&cat).unwrap_or(&0) as f64 / n_total;
            let w_balance = (-alpha * (p_i - uniform)).exp();
            let s_final = rule.priority * w_balance + entropy_beta * entropy;
            (rule, s_final / t)
        })
        .collect();

    // Softmax normalization with max-shift for numerical stability.
    let max_logit = scored
        .iter()
        .map(|(_, logit)| *logit)
        .fold(f64::NEG_INFINITY, f64::max);

    let mut with_prob: Vec<(&DesignRule, f64, f64)> = scored
        .drain(..)
        .map(|(rule, logit)| (rule, logit, (logit - max_logit).exp()))
        .collect();
    let z = with_prob.iter().map(|(_, _, x)| *x).sum::<f64>().max(1e-12);
    for (_, _, x) in &mut with_prob {
        *x /= z;
    }

    with_prob.sort_by(|(l_rule, l_logit, l_prob), (r_rule, r_logit, r_prob)| {
        r_prob
            .partial_cmp(l_prob)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| r_logit.partial_cmp(l_logit).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| l_rule.id.cmp(&r_rule.id))
    });

    let limit = max_select.max(1).min(with_prob.len());
    let mut selected = Vec::with_capacity(limit);
    let mut selected_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (rule, _, _) in with_prob.into_iter().take(limit) {
        selected.push(rule);
        let cat = rule_category_name(&rule.category).to_string();
        *selected_counts.entry(cat).or_insert(0) += 1;
    }

    (selected, selected_counts, availability_counts)
}

fn format_category_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    counts
        .iter()
        .map(|(cat, count)| format!("{cat}:{count}"))
        .collect::<Vec<_>>()
        .join("|")
}

fn shannon_entropy_from_counts(counts: &BTreeMap<String, usize>) -> f64 {
    let total = counts.values().copied().sum::<usize>();
    if total == 0 {
        return 0.0;
    }
    let total_f = total as f64;
    counts
        .values()
        .map(|count| *count as f64 / total_f)
        .filter(|p| *p > 0.0)
        .map(|p| -(p * p.ln()))
        .sum::<f64>()
}

fn update_lambda_entropy(
    lambda: f64,
    entropy: f64,
    target_entropy: f64,
    k: f64,
    ema: f64,
    lambda_min: f64,
    lambda_max: f64,
) -> f64 {
    let e = target_entropy - entropy;
    let lambda_prime = lambda + k * e;
    let lambda_new = (1.0 - ema) * lambda + ema * lambda_prime;
    lambda_new.clamp(lambda_min, lambda_max)
}

fn objective_distance(a: &ObjectiveVector, b: &ObjectiveVector) -> f64 {
    DISTANCE_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    let ds = a.f_struct - b.f_struct;
    let df = a.f_field - b.f_field;
    let dr = a.f_risk - b.f_risk;
    let dc = a.f_cost - b.f_cost;
    (ds * ds + df * df + dr * dr + dc * dc).sqrt()
}

fn pareto_mean_nn_distance(front: &[&ObjectiveVector]) -> f64 {
    if front.len() < 2 {
        return 0.0;
    }
    NN_DISTANCE_CALL_COUNT.fetch_add(front.len() * (front.len() - 1), Ordering::Relaxed);
    let mut sum = 0.0;
    for (i, obj) in front.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, other) in front.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(objective_distance(obj, other));
        }
        if best.is_finite() {
            sum += best;
        }
    }
    sum / front.len() as f64
}

fn pareto_spacing_metric(front: &[&ObjectiveVector]) -> f64 {
    if front.len() < 2 {
        return 0.0;
    }
    let mut nn = Vec::with_capacity(front.len());
    for (i, obj) in front.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, other) in front.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(objective_distance(obj, other));
        }
        if best.is_finite() {
            nn.push(best);
        }
    }
    if nn.len() < 2 {
        return 0.0;
    }
    let mean = nn.iter().sum::<f64>() / nn.len() as f64;
    let var = nn.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (nn.len() - 1) as f64;
    var.sqrt()
}

fn hv_2d_rect_approx(points: &[(f64, f64)]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    let mut xs = vec![0.0f64];
    for (x, _) in points {
        xs.push(x.clamp(0.0, 1.0));
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    let mut area = 0.0;
    for w in xs.windows(2) {
        let left = w[0];
        let right = w[1];
        if right <= left {
            continue;
        }
        let mid = (left + right) * 0.5;
        let max_y = points
            .iter()
            .filter(|(x, _)| x.clamp(0.0, 1.0) >= mid)
            .map(|(_, y)| y.clamp(0.0, 1.0))
            .fold(0.0f64, f64::max);
        area += (right - left) * max_y;
    }
    area.clamp(0.0, 1.0)
}

fn pareto_hv_2d_metric(front: &[&ObjectiveVector]) -> f64 {
    if front.is_empty() {
        return 0.0;
    }
    let hv_cost_perf = hv_2d_rect_approx(
        &front
            .iter()
            .map(|o| (o.f_cost.clamp(0.0, 1.0), o.f_struct.clamp(0.0, 1.0)))
            .collect::<Vec<_>>(),
    );
    let hv_rel_cost = hv_2d_rect_approx(
        &front
            .iter()
            .map(|o| (o.f_risk.clamp(0.0, 1.0), o.f_cost.clamp(0.0, 1.0)))
            .collect::<Vec<_>>(),
    );
    (hv_cost_perf + hv_rel_cost) * 0.5
}

fn obj_to_arr(obj: &ObjectiveVector) -> [f64; 4] {
    [obj.f_struct, obj.f_field, obj.f_risk, obj.f_cost]
}

fn arr_to_obj(v: [f64; 4]) -> ObjectiveVector {
    ObjectiveVector {
        f_struct: v[0],
        f_field: v[1],
        f_risk: v[2],
        f_cost: v[3],
    }
}

fn fmt_vec4(v: &[f64; 4]) -> String {
    format!("{:.9}|{:.9}|{:.9}|{:.9}", v[0], v[1], v[2], v[3])
}

fn evaluate_state_for_phase1(
    state: &DesignState,
    structural: &StructuralEvaluator,
    chm: &Chm,
    field: &FieldEngine,
    target: &TargetField,
) -> ObjectiveVector {
    let mut obj = structural.evaluate(state);
    obj.f_risk = risk_score_from_chm(state, chm);
    obj.f_field = resonance_score(&field.aggregate_state(state), target);
    obj.clamped()
}

fn objective_delta(next: &ObjectiveVector, current: &ObjectiveVector) -> ObjectiveVector {
    arr_to_obj([
        next.f_struct - current.f_struct,
        next.f_field - current.f_field,
        next.f_risk - current.f_risk,
        next.f_cost - current.f_cost,
    ])
}

fn objective_with_ortho(state: &DesignState, obj: ObjectiveVector, eps: f64) -> ObjectiveVector {
    let nodes = state.graph.nodes().len() as f64;
    let edges = state.graph.edges().len() as f64;
    let hist = state.profile_snapshot.len() as f64;
    let g = [
        (nodes / 64.0).tanh(),
        (edges / 128.0).tanh(),
        ((nodes - edges).abs() / 64.0).tanh(),
        (hist / 256.0).tanh(),
    ];
    arr_to_obj([
        (obj.f_struct + eps * g[0]).clamp(0.0, 1.0),
        (obj.f_field + eps * g[1]).clamp(0.0, 1.0),
        (obj.f_risk + eps * g[2]).clamp(0.0, 1.0),
        (obj.f_cost + eps * g[3]).clamp(0.0, 1.0),
    ])
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

fn normalize_phase1_vectors(objs: &[ObjectiveVector]) -> Vec<[f64; 4]> {
    if objs.is_empty() {
        return Vec::new();
    }
    let eps = 1e-6;
    let arrs = objs.iter().map(obj_to_arr).collect::<Vec<_>>();
    let mut meds = [0.0; 4];
    let mut mads = [0.0; 4];
    for i in 0..4 {
        let col = arrs.iter().map(|v| v[i]).collect::<Vec<_>>();
        meds[i] = median(col.clone());
        let abs_dev = col.iter().map(|x| (x - meds[i]).abs()).collect::<Vec<_>>();
        mads[i] = median(abs_dev);
    }
    arrs.into_iter()
        .map(|v| {
            let mut out = [0.0; 4];
            for i in 0..4 {
                out[i] = (v[i] - meds[i]) / (mads[i] + eps);
            }
            out
        })
        .collect()
}

fn corr_matrix4(vs: &[[f64; 4]]) -> [[f64; 4]; 4] {
    let n = vs.len();
    if n < 2 {
        return [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    }
    let mut mean = [0.0; 4];
    for v in vs {
        for i in 0..4 {
            mean[i] += v[i];
        }
    }
    for i in 0..4 {
        mean[i] /= n as f64;
    }
    let mut var = [0.0; 4];
    for v in vs {
        for i in 0..4 {
            var[i] += (v[i] - mean[i]).powi(2);
        }
    }
    for i in 0..4 {
        var[i] /= (n - 1) as f64;
    }
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            if i == j {
                out[i][j] = 1.0;
                continue;
            }
            if var[i] <= 1e-12 || var[j] <= 1e-12 {
                out[i][j] = 0.0;
                continue;
            }
            let mut cov = 0.0;
            for v in vs {
                cov += (v[i] - mean[i]) * (v[j] - mean[j]);
            }
            cov /= (n - 1) as f64;
            out[i][j] = (cov / (var[i].sqrt() * var[j].sqrt())).clamp(-1.0, 1.0);
        }
    }
    out
}

fn flatten_corr4(c: &[[f64; 4]; 4]) -> String {
    let mut vals = Vec::with_capacity(16);
    for row in c {
        for v in row {
            vals.push(format!("{:.6}", v));
        }
    }
    vals.join("|")
}

fn dist4(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let mut s = 0.0;
    for i in 0..4 {
        s += (a[i] - b[i]).powi(2);
    }
    s.sqrt()
}

fn mean_nn_dist4(vs: &[[f64; 4]]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0;
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(dist4(v, u));
        }
        if best.is_finite() {
            sum += best;
        }
    }
    sum / vs.len() as f64
}

fn spacing4(vs: &[[f64; 4]]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    let mut nn = Vec::with_capacity(vs.len());
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(dist4(v, u));
        }
        if best.is_finite() {
            nn.push(best);
        }
    }
    if nn.len() < 2 {
        return 0.0;
    }
    let mean = nn.iter().sum::<f64>() / nn.len() as f64;
    (nn.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (nn.len() - 1) as f64).sqrt()
}

fn robust_stats_from_samples(samples: &[ObjectiveRaw], alpha: f64) -> Option<GlobalRobustStats> {
    if samples.is_empty() {
        return None;
    }
    let mut med = [0.0; 4];
    let mut mad = [0.0; 4];
    let mut std_dev = [0.0; 4];
    let mut mean = [0.0; 4];
    let mut active = [true; 4];
    let mut weak = [false; 4];
    let mut weights = [0.0; 4];
    let eps_mad = 1e-12;
    let eps_std = 1e-9;
    let alpha_weak = alpha;

    for i in 0..4 {
        let col = samples.iter().map(|v| v.0[i]).collect::<Vec<_>>();
        med[i] = median(col.clone());
        mad[i] = compute_mad(&col, med[i]);
        mean[i] = compute_mean(&col);
        std_dev[i] = compute_std(&col, mean[i]);

        if mad[i] > eps_mad {
            active[i] = true;
            weak[i] = false;
            weights[i] = 1.0;
        } else if std_dev[i] > eps_std {
            active[i] = true;
            weak[i] = true;
            weights[i] = alpha_weak;
        } else {
            active[i] = false;
            weak[i] = false;
            weights[i] = 0.0;
        }
    }
    let mad_zero_count = active.iter().zip(mad.iter()).filter(|&(&a, &m)| a && m <= eps_mad).count();

    Some(GlobalRobustStats {
        median: med,
        mad,
        mean,
        std: std_dev,
        active_dims: active,
        weak_dims: weak,
        weights,
        mad_zero_count,
        alpha_used: alpha_weak,
    })
}

fn normalize_objective(raw: &ObjectiveRaw, stats: &GlobalRobustStats) -> ObjectiveNorm {
    let eps_small = 1e-9;
    let mut out = [0.0; 4];
    for i in 0..4 {
        if stats.active_dims[i] {
            if !stats.weak_dims[i] {
                // Strong active: (x - median) / MAD
                let safe_mad = if stats.mad[i] < eps_small { eps_small } else { stats.mad[i] };
                out[i] = (raw.0[i] - stats.median[i]) / safe_mad;
            } else {
                // Weak active: (x - mean) / max(std, eps_small)
                let den = if stats.std[i] < eps_small { eps_small } else { stats.std[i] };
                out[i] = (raw.0[i] - stats.mean[i]) / den;
            }
        } else {
            out[i] = 0.0;
        }
    }
    ObjectiveNorm(out)
}

fn norm_distance(a: &ObjectiveNorm, b: &ObjectiveNorm, weights: &[f64; 4]) -> f64 {
    DISTANCE_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut s = 0.0;
    let mut w_sum = 0.0;
    for i in 0..4 {
        if weights[i] > 0.0 {
            s += weights[i] * (a.0[i] - b.0[i]).powi(2);
            w_sum += weights[i];
        }
    }
    if w_sum <= 1e-12 {
        return 0.0;
    }
    let dist = (s / w_sum).sqrt();
    let eps_dist = 1e-6;
    if dist < eps_dist {
        eps_dist
    } else {
        dist
    }
}

fn mean_nn_dist_norm(vs: &[ObjectiveNorm], weights: &[f64; 4]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    NN_DISTANCE_CALL_COUNT.fetch_add(vs.len() * (vs.len() - 1), Ordering::Relaxed);
    let mut sum = 0.0;
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(norm_distance(v, u, weights));
        }
        if best.is_finite() {
            sum += best;
        }
    }
    sum / vs.len() as f64
}

fn count_unique_norm(vs: &[ObjectiveNorm], weights: &[f64; 4]) -> usize {
    let mut unique: Vec<&ObjectiveNorm> = Vec::new();
    for v in vs {
        if !unique.iter().any(|u| norm_distance(v, u, weights) < 1e-6) {
            unique.push(v);
        }
    }
    unique.len()
}

fn spacing_norm(vs: &[ObjectiveNorm], weights: &[f64; 4]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    let mut nn = Vec::with_capacity(vs.len());
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(norm_distance(v, u, weights));
        }
        if best.is_finite() {
            nn.push(best);
        }
    }
    if nn.len() < 2 {
        return 0.0;
    }
    let mean = nn.iter().sum::<f64>() / nn.len() as f64;
    (nn.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (nn.len() - 1) as f64).sqrt()
}

fn corr_matrix_norm(vs: &[ObjectiveNorm]) -> [[f64; 4]; 4] {
    let arr = vs.iter().map(|v| v.0).collect::<Vec<_>>();
    corr_matrix4(&arr)
}

fn rescale_norm_for_hv(vs: &[ObjectiveNorm]) -> Vec<[f64; 4]> {
    if vs.is_empty() {
        return Vec::new();
    }
    let eps = 1e-6;
    let mut minv = [f64::INFINITY; 4];
    let mut maxv = [f64::NEG_INFINITY; 4];
    for v in vs {
        for i in 0..4 {
            minv[i] = minv[i].min(v.0[i]);
            maxv[i] = maxv[i].max(v.0[i]);
        }
    }
    vs.iter()
        .map(|v| {
            let mut out = [0.0; 4];
            for i in 0..4 {
                out[i] = ((v.0[i] - minv[i]) / (maxv[i] - minv[i] + eps)).clamp(0.0, 1.0);
            }
            out
        })
        .collect()
}

fn pareto_hv_2d_norm(vs: &[ObjectiveNorm]) -> f64 {
    let scaled = rescale_norm_for_hv(vs);
    if scaled.is_empty() {
        return 0.0;
    }
    let hv_cost_perf = hv_2d_rect_approx(&scaled.iter().map(|v| (v[3], v[0])).collect::<Vec<_>>());
    let hv_rel_cost = hv_2d_rect_approx(&scaled.iter().map(|v| (v[2], v[3])).collect::<Vec<_>>());
    (hv_cost_perf + hv_rel_cost) * 0.5
}

fn select_beam_maxmin_norm(
    front: Vec<(DesignState, ObjectiveVector)>,
    norms: Vec<ObjectiveNorm>,
    beam: usize,
    weights: &[f64; 4],
) -> Vec<(DesignState, ObjectiveVector)> {
    if front.is_empty() {
        return Vec::new();
    }
    let beam = beam.max(1).min(front.len());
    let mut used = vec![false; front.len()];
    let mut selected_idx = Vec::with_capacity(beam);
    let mut seed = 0usize;
    let mut best = f64::NEG_INFINITY;
    for (i, (_, obj)) in front.iter().enumerate() {
        let s = scalar_score(obj);
        if s > best {
            best = s;
            seed = i;
        }
    }
    selected_idx.push(seed);
    used[seed] = true;
    while selected_idx.len() < beam {
        let mut best_i = None;
        let mut best_d = f64::NEG_INFINITY;
        for i in 0..front.len() {
            if used[i] {
                continue;
            }
            let dmin = selected_idx
                .iter()
                .map(|j| norm_distance(&norms[i], &norms[*j], weights))
                .fold(f64::INFINITY, f64::min);
            if dmin > best_d {
                best_d = dmin;
                best_i = Some(i);
            }
        }
        if let Some(i) = best_i {
            used[i] = true;
            selected_idx.push(i);
        } else {
            break;
        }
    }
    selected_idx.into_iter().map(|i| front[i].clone()).collect()
}

fn map_rule_category(category: RuleCategory) -> NodeCategory {
    match category {
        RuleCategory::Structural => NodeCategory::Abstraction,
        RuleCategory::Performance => NodeCategory::Performance,
        RuleCategory::Reliability => NodeCategory::Reliability,
        RuleCategory::Cost => NodeCategory::CostSensitive,
        RuleCategory::Refactor => NodeCategory::Control,
        RuleCategory::ConstraintPropagation => NodeCategory::Constraint,
    }
}

fn dhm_rule_resonance(rule: &DesignRule, field: &FieldEngine, dhm_field: &FieldVector) -> f64 {
    if dhm_field.dimensions() == 0 {
        return 0.0;
    }
    let basis = field.projector().basis_for(map_rule_category(rule.category.clone()));
    let target = TargetField {
        data: dhm_field.clone(),
    };
    resonance_score(&basis, &target).clamp(0.0, 1.0)
}

fn build_dhm_field(
    memory: &[(usize, FieldVector)],
    depth: usize,
    gamma: f64,
    k_nearest: usize,
    dimensions: usize,
) -> (FieldVector, f64) {
    if memory.is_empty() {
        return (FieldVector::zeros(dimensions), 0.0);
    }

    let mut acc = FieldVector::zeros(dimensions);
    let window = k_nearest.max(1);
    let start = memory.len().saturating_sub(window);
    for (d_i, field_i) in &memory[start..] {
        let dt = depth.saturating_sub(*d_i) as f64;
        let w = (-gamma * dt).exp() as f32;
        acc = acc.add(&field_i.scale(w));
    }

    let norm = acc
        .data
        .iter()
        .map(|v| v.norm_sqr() as f64)
        .sum::<f64>()
        .sqrt();
    let denom = (norm + 1e-6) as f32;
    let normalized = if denom <= f32::EPSILON {
        FieldVector::zeros(dimensions)
    } else {
        acc.scale(1.0 / denom)
    };
    (normalized, norm)
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
        generate_trace, normalize_by_depth, p_inferred, profile_modulation, scalar_score,
        stability_index, BeamSearch, MacroOperator, ParetoFront, Phase45Controller,
        ProfileUpdateType, SearchConfig, SearchMode, SystemEvaluator, TraceRunConfig,
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
                norm_alpha: 0.25,
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

        let (normalized, _) = normalize_by_depth(candidates, 0.25);
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
                norm_alpha: 0.25,
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

    #[test]
    fn trace_rows_include_diversity_pressure_signals() {
        let rows = generate_trace(TraceRunConfig {
            depth: 10,
            beam: 5,
            seed: 42,
        });
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| (0.0..=1.0).contains(&r.pressure)));
        assert!(rows.iter().all(|r| (0.0..=0.15).contains(&r.epsilon_effect)));
        assert!(rows.iter().all(|r| (0.0..=1.0).contains(&r.target_local_weight)));
        assert!(rows.iter().all(|r| (0.0..=1.0).contains(&r.target_global_weight)));
        assert!(rows
            .iter()
            .all(|r| (r.target_local_weight + r.target_global_weight - 1.0).abs() < 1e-5));
        assert!(rows.iter().all(|r| r.local_global_distance >= 0.0));
        assert!(rows.iter().all(|r| r.field_min_distance >= 0.0));
        assert!(rows.iter().all(|r| r.field_rejected_count <= 1000));
        assert!(rows.iter().any(|r| r.field_rejected_count > 0));
        assert!(rows.iter().all(|r| (0.0..=1.0).contains(&r.mu)));
        assert!(rows.iter().all(|r| r.dhm_norm >= 0.0));
        assert!(rows.iter().all(|r| (0.0..=1.0).contains(&r.dhm_resonance_mean)));
        assert!(rows.iter().all(|r| r.dhm_score_ratio >= 1.0));
        assert!(rows.iter().all(|r| r.dhm_build_us >= 0.0));
    }
}
