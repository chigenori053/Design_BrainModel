// ALLOW_LIB_LOOP: temporarily allowed until phase3.14
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

pub static DISTANCE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static NN_DISTANCE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
pub(crate) const SOFT_PARETO_TEMPERATURE: f64 = 0.05;

pub mod adapters;
pub mod agent;
pub mod capability;
pub mod domain;
pub mod ports;
pub mod prelude;
pub mod runtime;

mod diversity;
mod engine;
mod normalization;
mod stability;

use core_types::ObjectiveVector;
use field_engine::{FieldEngine, TargetField};
use hybrid_vm::Chm;
use hybrid_vm::{DesignRule, Shm, Transformation};
use hybrid_vm::{Evaluator, HybridVM};
use memory_space::{DesignState, StateId, Uuid};
use stability::*;

pub use engine::pareto::dominates;

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

        self.states
            .retain(|(_, existing)| !dominates(&obj, existing));

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

impl Default for ParetoFront {
    fn default() -> Self {
        Self::new()
    }
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

pub struct BeamSearch<'a> {
    pub shm: &'a Shm,
    pub chm: &'a Chm,
    pub evaluator: &'a dyn Evaluator,
    pub config: SearchConfig,
}

pub struct SystemEvaluator<'a> {
    pub(crate) vm: std::sync::Mutex<HybridVM>,
    pub(crate) _chm: &'a Chm,
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
    pub avg_tau_mem: f32,
    pub avg_delta_norm: f32,
    pub memory_hit_rate: f32,
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
            avg_tau_mem: 0.0,
            avg_delta_norm: 0.0,
            memory_hit_rate: 0.0,
            redundancy_flags: String::new(),
            saturation_flags: String::new(),
            discrete_saturation_count: 0,
            effective_dim: 0,
            effective_dim_ratio: 0.0,
            collapse_reasons: String::new(),
        }
    }
}

pub struct TraceRowBuilder {
    row: TraceRow,
}

impl TraceRowBuilder {
    pub fn new() -> Self {
        Self {
            row: TraceRow::default(),
        }
    }

    pub fn apply(mut self, f: impl FnOnce(&mut TraceRow)) -> Self {
        f(&mut self.row);
        self
    }

    pub fn build(self) -> TraceRow {
        self.row
    }
}

impl Default for TraceRowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub use prelude::*;

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
    pub completeness: f64,
    pub ambiguity_mean: f64,
    pub inconsistency: f64,
    pub cls: f64,
    pub scs_v1: f64,
    pub scs_v1_1: f64,
    pub dependency_consistency: f64,
    pub connectivity: f64,
    pub cyclicity: f64,
    pub orphan_rate: f64,
    pub phase2_triggered: bool,
    pub phase2_false_trigger_proxy: bool,
    pub sanity_empty_id_fixes: usize,
    pub sanity_duplicate_id_fixes: usize,
    pub sanity_unknown_dependency_drops: usize,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoftTraceParams {
    pub alpha: f64,
    pub temperature: f64,
    pub entropy_beta: f64,
    pub lambda_min: f64,
    pub lambda_target_entropy: f64,
    pub lambda_k: f64,
    pub lambda_ema: f64,
    pub field_profile: bool,
}

impl Default for SoftTraceParams {
    fn default() -> Self {
        Self {
            alpha: 0.6,
            temperature: 0.7,
            entropy_beta: 0.25,
            lambda_min: 0.05,
            lambda_target_entropy: 1.0,
            lambda_k: 0.05,
            lambda_ema: 0.2,
            field_profile: true,
        }
    }
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

    pub fn on_profile_update(
        &mut self,
        depth: usize,
        stability_index: f64,
        kind: ProfileUpdateType,
    ) {
        let priority = match kind {
            ProfileUpdateType::TypeAExplicit => 3,
            ProfileUpdateType::TypeBStructural => 2,
            ProfileUpdateType::TypeCStatistical => 1,
        };
        if depth < self.next_allowed_update_depth && priority < 3 {
            return;
        }

        self.k = runtime::trace_helpers::select_k_with_hysteresis(self.k, stability_index);
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
    runtime::execute_trace(config)
}

pub fn generate_trace_baseline_off(config: TraceRunConfig) -> Vec<TraceRow> {
    runtime::execute_trace_baseline_off(config)
}

pub fn generate_trace_baseline_off_balanced(config: TraceRunConfig, m: usize) -> Vec<TraceRow> {
    runtime::execute_trace_baseline_off_balanced(config, m)
}

pub fn generate_trace_baseline_off_soft(
    config: TraceRunConfig,
    params: SoftTraceParams,
) -> Vec<TraceRow> {
    runtime::execute_soft_trace(config, params)
}

pub fn run_bench(config: BenchConfig) -> BenchResult {
    runtime::bench::run(config)
}

pub fn run_bench_baseline_off(config: BenchConfig) -> BenchResult {
    runtime::bench::run_baseline_off(config)
}

pub fn run_bench_baseline_off_balanced(config: BenchConfig, m: usize) -> BenchResult {
    runtime::bench::run_baseline_off_balanced(config, m)
}

pub fn run_bench_baseline_off_soft(config: BenchConfig, params: SoftTraceParams) -> BenchResult {
    runtime::bench::run_baseline_off_soft(config, params)
}

pub(crate) fn normalize_by_depth(
    candidates: Vec<(DesignState, ObjectiveVector)>,
    alpha: f64,
) -> (Vec<(DesignState, ObjectiveVector)>, GlobalRobustStats) {
    engine::normalization::normalize_by_depth_candidates(candidates, alpha)
}

pub fn scalar_score(obj: &ObjectiveVector) -> f64 {
    capability::LinearObjectiveScorer.score_objective(obj)
}

pub fn apply_atomic(rule: &DesignRule, state: &DesignState) -> DesignState {
    capability::apply::apply_atomic(rule, state)
}

pub fn apply_macro(op: &MacroOperator, state: &DesignState) -> DesignState {
    capability::apply::apply_macro(op, state)
}

pub fn build_target_field(
    field: &FieldEngine,
    shm: &Shm,
    state: &DesignState,
    lambda: f64,
) -> TargetField {
    domain::build_target_field(field, shm, state, lambda)
}

pub fn build_target_field_with_diversity(
    field: &FieldEngine,
    shm: &Shm,
    state: &DesignState,
    lambda: f64,
    diversity: f64,
) -> (TargetField, diversity::DiversityAdjustment) {
    domain::build_target_field_with_diversity(field, shm, state, lambda, diversity)
}

pub fn chm_density(n_edge_obs: usize, category_count: usize) -> f64 {
    domain::chm_density(n_edge_obs, category_count)
}

pub fn profile_modulation(stability_index: f64) -> f64 {
    domain::profile_modulation(stability_index)
}

pub fn run_phase1_matrix(config: Phase1Config) -> (Vec<Phase1RawRow>, Vec<Phase1SummaryRow>) {
    runtime::phase1::run_phase1_matrix(config)
}
