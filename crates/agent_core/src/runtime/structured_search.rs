use std::collections::{BTreeMap, BTreeSet};

use core_types::ObjectiveVector;
use memory_space::{DesignState, Uuid};

const BETA_MIN: f64 = 0.05;
const BETA_MAX: f64 = 0.4;
const BETA_DECAY_RATE: f64 = 0.9;
const BETA_GROWTH_RATE: f64 = 1.2;
const BETA_STAGNATION_K: usize = 6;
const TEMPORAL_SMOOTHING_ALPHA: f64 = 0.3;
const HV_GROWTH_THRESHOLD: f64 = 1e-3;
const HV_STOP_EPS: f64 = 1e-3;
const STOP_WINDOW: usize = 12;
const COVERAGE_THRESHOLD: f64 = 0.8;
const MIN_CLUSTER_SIZE: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SearchFeatureVector {
    pub coupling: f64,
    pub propagation_score: f64,
    pub impact: f64,
    pub structural_variance: f64,
    pub cycle_flag: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchCandidate {
    pub state: DesignState,
    pub objective: ObjectiveVector,
    pub rule_id: Uuid,
    pub normalized_objective: [f64; 4],
    pub feature: SearchFeatureVector,
    pub simulation: Option<crate::runtime::world_model::SimulationDelta>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cluster {
    pub id: usize,
    pub members: Vec<usize>,
    pub centroid: SearchFeatureVector,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(crate) struct SearchMetrics {
    pub hv: f64,
    pub hv_delta: f64,
    pub score_variance: f64,
    pub diversity: f64,
    pub stagnation_steps: usize,
    pub cluster_coverage: f64,
    pub frontier_change_ratio: f64,
    pub unchanged_steps: usize,
    pub step: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchController {
    pub clusters: Vec<Cluster>,
    pub frontier: Vec<SearchCandidate>,
    pub beta: f64,
    pub metrics: SearchMetrics,
    pub smoothed_scores: BTreeMap<u128, f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct StructuredSearchOutcome {
    pub controller: SearchController,
    pub frontier_hv: f64,
    pub hv_delta: f64,
    pub beta_used: f64,
    pub cluster_coverage: f64,
    pub score_variance: f64,
    pub diversity_mean: f64,
    pub frontier_change_ratio: f64,
    pub cluster_collapsed: bool,
    pub stop_triggered: bool,
}

pub(crate) trait StructureClusterer {
    fn cluster(features: &[SearchFeatureVector], max_clusters: usize) -> Vec<Cluster>;
}

pub(crate) trait BetaScheduler {
    fn update(beta: f64, metrics: &SearchMetrics) -> f64;
}

pub(crate) trait FrontierManager {
    fn update(
        previous: Option<&SearchController>,
        candidates: Vec<SearchCandidate>,
        clusters: &[Cluster],
        beta: f64,
        beam_width: usize,
    ) -> FrontierUpdate;
}

pub(crate) trait ClusterManager {
    fn rebalance(
        clusters: &mut Vec<Cluster>,
        candidates: &[SearchCandidate],
        beam_width: usize,
        previous: Option<&SearchController>,
    ) -> bool;
}

pub(crate) trait StopController {
    fn should_stop(metrics: &SearchMetrics, max_steps: usize) -> bool;
}

pub(crate) struct DeterministicKMeansClusterer;
pub(crate) struct AdaptiveBetaScheduler;
pub(crate) struct StableFrontierManager;
pub(crate) struct AdaptiveClusterManager;
pub(crate) struct DeterministicStopController;

#[derive(Clone, Debug)]
pub(crate) struct FrontierUpdate {
    frontier: Vec<SearchCandidate>,
    smoothed_scores: BTreeMap<u128, f64>,
    diversity_mean: f64,
    frontier_change_ratio: f64,
}

impl StructureClusterer for DeterministicKMeansClusterer {
    fn cluster(features: &[SearchFeatureVector], max_clusters: usize) -> Vec<Cluster> {
        if features.is_empty() {
            return Vec::new();
        }
        if features.len() == 1 || max_clusters <= 1 {
            return vec![Cluster {
                id: 0,
                members: vec![0],
                centroid: features[0],
            }];
        }

        let target_k = desired_cluster_count(features.len(), max_clusters);
        if target_k <= 1 {
            return vec![Cluster {
                id: 0,
                members: (0..features.len()).collect(),
                centroid: centroid_of(features, &(0..features.len()).collect::<Vec<_>>()),
            }];
        }

        let mut centroids = initial_centroids(features, target_k);
        let mut assignments = vec![0usize; features.len()];

        for _ in 0..8 {
            for (idx, feature) in features.iter().enumerate() {
                assignments[idx] = nearest_centroid(*feature, &centroids);
            }

            rebalance_empty_clusters(&mut assignments, features, target_k, &centroids);

            let mut next_centroids = centroids.clone();
            for cluster_id in 0..target_k {
                let members = assignments
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, assigned)| (*assigned == cluster_id).then_some(idx))
                    .collect::<Vec<_>>();
                if !members.is_empty() {
                    next_centroids[cluster_id] = centroid_of(features, &members);
                }
            }
            if next_centroids == centroids {
                break;
            }
            centroids = next_centroids;
        }

        let mut clusters = (0..target_k)
            .map(|cluster_id| Cluster {
                id: cluster_id,
                members: assignments
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, assigned)| (*assigned == cluster_id).then_some(idx))
                    .collect(),
                centroid: centroids[cluster_id],
            })
            .filter(|cluster| !cluster.members.is_empty())
            .collect::<Vec<_>>();

        merge_small_clusters(&mut clusters, features);
        for (idx, cluster) in clusters.iter_mut().enumerate() {
            cluster.id = idx;
            cluster.members.sort_unstable();
            cluster.centroid = centroid_of(features, &cluster.members);
        }
        clusters
    }
}

impl BetaScheduler for AdaptiveBetaScheduler {
    fn update(beta: f64, metrics: &SearchMetrics) -> f64 {
        let mut next = beta.clamp(BETA_MIN, BETA_MAX);
        if metrics.hv_delta > HV_GROWTH_THRESHOLD {
            next *= BETA_DECAY_RATE;
        } else if metrics.stagnation_steps >= BETA_STAGNATION_K {
            next *= BETA_GROWTH_RATE;
        }
        next.clamp(BETA_MIN, BETA_MAX)
    }
}

impl FrontierManager for StableFrontierManager {
    fn update(
        previous: Option<&SearchController>,
        candidates: Vec<SearchCandidate>,
        clusters: &[Cluster],
        beta: f64,
        beam_width: usize,
    ) -> FrontierUpdate {
        let mut selected_indices = Vec::<usize>::new();
        let mut used = BTreeSet::<usize>::new();
        let global_centroid = centroid_of(
            &candidates
                .iter()
                .map(|candidate| candidate.feature)
                .collect::<Vec<_>>(),
            &(0..candidates.len()).collect::<Vec<_>>(),
        );

        let previous_scores = previous
            .map(|controller| &controller.smoothed_scores)
            .cloned()
            .unwrap_or_default();
        let previous_cluster_ids = previous_cluster_ids(previous, clusters, &candidates);
        let previous_frontier_ids = previous
            .map(|controller| {
                controller
                    .frontier
                    .iter()
                    .map(|candidate| candidate.state.id.as_u128())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let mut smoothed_scores = BTreeMap::new();
        let mut cluster_order = (0..clusters.len()).collect::<Vec<_>>();
        cluster_order.sort_by(|&lhs, &rhs| {
            cluster_priority(&clusters[rhs], &candidates, &previous_cluster_ids)
                .partial_cmp(&cluster_priority(
                    &clusters[lhs],
                    &candidates,
                    &previous_cluster_ids,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| clusters[lhs].id.cmp(&clusters[rhs].id))
        });

        let retain_target = previous
            .map(|controller| {
                controller
                    .frontier
                    .len()
                    .min(beam_width)
                    .min(((beam_width * 3) / 5).max(1))
            })
            .unwrap_or(0);
        if retain_target > 0 {
            let cluster_lookup = clusters
                .iter()
                .flat_map(|cluster| cluster.members.iter().map(|member| (*member, cluster.id)))
                .collect::<BTreeMap<_, _>>();
            let mut retained = candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    previous_frontier_ids.contains(&candidate.state.id.as_u128())
                })
                .collect::<Vec<_>>();
            retained.sort_by(|lhs, rhs| {
                previous_scores
                    .get(&rhs.1.state.id.as_u128())
                    .copied()
                    .unwrap_or(0.0)
                    .partial_cmp(
                        &previous_scores
                            .get(&lhs.1.state.id.as_u128())
                            .copied()
                            .unwrap_or(0.0),
                    )
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| lhs.1.state.id.cmp(&rhs.1.state.id))
            });
            let mut represented_clusters = BTreeSet::new();
            for (idx, candidate) in retained.into_iter().take(retain_target) {
                let cluster_id = cluster_lookup.get(&idx).copied().unwrap_or(usize::MAX);
                if !represented_clusters.insert(cluster_id) {
                    continue;
                }
                used.insert(idx);
                selected_indices.push(idx);
                let score = previous_scores
                    .get(&candidate.state.id.as_u128())
                    .copied()
                    .unwrap_or_else(|| {
                        smoothed_candidate_score(
                            idx,
                            &candidates,
                            &selected_indices,
                            beta,
                            global_centroid,
                            &previous_scores,
                        )
                    });
                smoothed_scores.insert(candidate.state.id.as_u128(), score);
            }
        }

        for cluster_idx in cluster_order.iter().copied().take(beam_width) {
            if selected_indices.len() >= beam_width {
                break;
            }
            if let Some(best) = pick_best_member(
                &clusters[cluster_idx],
                &candidates,
                &used,
                &selected_indices,
                beta,
                global_centroid,
                &previous_scores,
                &mut smoothed_scores,
            ) {
                used.insert(best);
                selected_indices.push(best);
            }
        }

        let fill_order = cluster_order.clone();
        let mut cursor = 0usize;
        while selected_indices.len() < beam_width && !fill_order.is_empty() {
            let cluster_idx = fill_order[cursor % fill_order.len()];
            if let Some(best) = pick_best_member(
                &clusters[cluster_idx],
                &candidates,
                &used,
                &selected_indices,
                beta,
                global_centroid,
                &previous_scores,
                &mut smoothed_scores,
            ) {
                used.insert(best);
                selected_indices.push(best);
            }
            cursor += 1;
            if cursor >= fill_order.len() * beam_width {
                break;
            }
        }

        if selected_indices.len() < beam_width {
            let mut remaining = (0..candidates.len())
                .filter(|idx| !used.contains(idx))
                .collect::<Vec<_>>();
            remaining.sort_by(|&lhs, &rhs| {
                smoothed_candidate_score(
                    lhs,
                    &candidates,
                    &selected_indices,
                    beta,
                    global_centroid,
                    &previous_scores,
                )
                .partial_cmp(&smoothed_candidate_score(
                    rhs,
                    &candidates,
                    &selected_indices,
                    beta,
                    global_centroid,
                    &previous_scores,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
                .reverse()
                .then_with(|| candidates[lhs].state.id.cmp(&candidates[rhs].state.id))
            });
            for idx in remaining
                .into_iter()
                .take(beam_width.saturating_sub(selected_indices.len()))
            {
                let score = smoothed_candidate_score(
                    idx,
                    &candidates,
                    &selected_indices,
                    beta,
                    global_centroid,
                    &previous_scores,
                );
                smoothed_scores.insert(candidates[idx].state.id.as_u128(), score);
                used.insert(idx);
                selected_indices.push(idx);
            }
        }

        selected_indices
            .sort_by(|lhs, rhs| candidates[*lhs].state.id.cmp(&candidates[*rhs].state.id));
        let frontier = selected_indices
            .iter()
            .map(|idx| candidates[*idx].clone())
            .collect::<Vec<_>>();
        let diversity_mean = mean_diversity(&frontier);
        let frontier_change_ratio = frontier_change_ratio(previous, &frontier);

        FrontierUpdate {
            frontier,
            smoothed_scores,
            diversity_mean,
            frontier_change_ratio,
        }
    }
}

impl ClusterManager for AdaptiveClusterManager {
    fn rebalance(
        clusters: &mut Vec<Cluster>,
        candidates: &[SearchCandidate],
        beam_width: usize,
        previous: Option<&SearchController>,
    ) -> bool {
        if clusters.is_empty() || candidates.len() < MIN_CLUSTER_SIZE * 2 {
            return false;
        }

        let collapse_detected = clusters.len() <= 1
            || clusters
                .iter()
                .any(|cluster| cluster.members.len() < MIN_CLUSTER_SIZE);
        let coverage_gap = previous
            .map(|controller| controller.metrics.cluster_coverage < COVERAGE_THRESHOLD)
            .unwrap_or(false);

        if !collapse_detected && !coverage_gap {
            return false;
        }

        let fallback = fallback_rule_based_clusters(candidates, beam_width.min(7).max(2));
        if fallback.len() <= 1 {
            return false;
        }
        *clusters = fallback;
        true
    }
}

impl StopController for DeterministicStopController {
    fn should_stop(metrics: &SearchMetrics, max_steps: usize) -> bool {
        metrics.step >= max_steps
            || metrics.stagnation_steps >= STOP_WINDOW
            || metrics.unchanged_steps >= STOP_WINDOW
    }
}

pub(crate) fn build_search_candidate(
    state: DesignState,
    objective: ObjectiveVector,
    rule_id: Uuid,
    normalized_objective: [f64; 4],
    simulation: Option<crate::runtime::world_model::SimulationDelta>,
) -> SearchCandidate {
    SearchCandidate {
        feature: feature_vector_for_state(&state),
        state,
        objective,
        rule_id,
        normalized_objective,
        simulation,
    }
}

#[allow(dead_code)]
pub(crate) fn select_structured_frontier(
    candidates: Vec<SearchCandidate>,
    beam_width: usize,
    depth: usize,
    max_steps: usize,
) -> StructuredSearchOutcome {
    select_controlled_frontier(candidates, None, beam_width, depth, max_steps)
}

pub(crate) fn select_controlled_frontier(
    candidates: Vec<SearchCandidate>,
    previous: Option<&SearchController>,
    beam_width: usize,
    depth: usize,
    max_steps: usize,
) -> StructuredSearchOutcome {
    if candidates.is_empty() && previous.is_none() {
        return empty_outcome(depth);
    }

    let beam_width = beam_width.max(1);
    let merged_candidates = merge_candidates(previous, candidates);
    if merged_candidates.is_empty() {
        return empty_outcome(depth);
    }

    let previous_metrics = previous
        .map(|controller| controller.metrics.clone())
        .unwrap_or_default();
    let beta = AdaptiveBetaScheduler::update(
        previous.map(|controller| controller.beta).unwrap_or(0.3),
        &previous_metrics,
    );

    let merged_candidates = {
        let filtered = pareto_filtered_candidates(merged_candidates.clone());
        if filtered.len() < beam_width.min(merged_candidates.len()) {
            merged_candidates
        } else {
            filtered
        }
    };
    let features = merged_candidates
        .iter()
        .map(|candidate| candidate.feature)
        .collect::<Vec<_>>();
    let mut clusters = DeterministicKMeansClusterer::cluster(&features, beam_width.min(7).max(1));
    let reconfigured =
        AdaptiveClusterManager::rebalance(&mut clusters, &merged_candidates, beam_width, previous);
    let cluster_collapsed = clusters.len() <= 1;

    let frontier_update = StableFrontierManager::update(
        previous,
        merged_candidates.clone(),
        &clusters,
        beta,
        beam_width.min(merged_candidates.len()),
    );

    let frontier = frontier_update.frontier;
    let raw_frontier_hv = crate::hv_4d_from_origin_normalized(
        &frontier
            .iter()
            .map(|candidate| candidate.normalized_objective)
            .collect::<Vec<_>>(),
    );
    let previous_hv = previous
        .map(|controller| controller.metrics.hv)
        .unwrap_or(0.0);
    let frontier_hv = raw_frontier_hv.max(previous_hv);
    let raw_hv_delta = frontier_hv - previous_hv;
    let hv_delta = if raw_hv_delta.abs() < HV_STOP_EPS {
        0.0
    } else {
        raw_hv_delta
    };
    let score_variance = objective_score_variance(&frontier);
    let selected_cluster_ids = selected_cluster_ids(&clusters, &merged_candidates, &frontier);
    let cluster_coverage = if clusters.is_empty() {
        0.0
    } else {
        selected_cluster_ids.len() as f64 / clusters.len() as f64
    };
    let stagnation_steps = if hv_delta > HV_GROWTH_THRESHOLD {
        0
    } else {
        previous_metrics.stagnation_steps + 1
    };
    let unchanged_steps = if frontier_update.frontier_change_ratio <= 0.2 {
        previous_metrics.unchanged_steps + 1
    } else {
        0
    };
    let metrics = SearchMetrics {
        hv: frontier_hv,
        hv_delta,
        score_variance,
        diversity: frontier_update.diversity_mean,
        stagnation_steps,
        cluster_coverage,
        frontier_change_ratio: frontier_update.frontier_change_ratio,
        unchanged_steps,
        step: depth,
    };
    let stop_triggered = DeterministicStopController::should_stop(&metrics, max_steps);

    StructuredSearchOutcome {
        controller: SearchController {
            clusters,
            frontier,
            beta,
            metrics,
            smoothed_scores: frontier_update.smoothed_scores,
        },
        frontier_hv,
        hv_delta,
        beta_used: beta,
        cluster_coverage,
        score_variance,
        diversity_mean: frontier_update.diversity_mean,
        frontier_change_ratio: frontier_update.frontier_change_ratio,
        cluster_collapsed: cluster_collapsed && !reconfigured,
        stop_triggered,
    }
}

fn empty_outcome(depth: usize) -> StructuredSearchOutcome {
    StructuredSearchOutcome {
        controller: SearchController {
            clusters: Vec::new(),
            frontier: Vec::new(),
            beta: BETA_MIN,
            metrics: SearchMetrics {
                step: depth,
                ..SearchMetrics::default()
            },
            smoothed_scores: BTreeMap::new(),
        },
        frontier_hv: 0.0,
        hv_delta: 0.0,
        beta_used: BETA_MIN,
        cluster_coverage: 0.0,
        score_variance: 0.0,
        diversity_mean: 0.0,
        frontier_change_ratio: 0.0,
        cluster_collapsed: true,
        stop_triggered: false,
    }
}

fn merge_candidates(
    previous: Option<&SearchController>,
    candidates: Vec<SearchCandidate>,
) -> Vec<SearchCandidate> {
    let mut ordered = BTreeMap::<u128, SearchCandidate>::new();
    if let Some(controller) = previous {
        for candidate in &controller.frontier {
            ordered.insert(candidate.state.id.as_u128(), candidate.clone());
        }
    }
    for candidate in candidates {
        ordered.insert(candidate.state.id.as_u128(), candidate);
    }
    ordered.into_values().collect()
}

fn pareto_filtered_candidates(candidates: Vec<SearchCandidate>) -> Vec<SearchCandidate> {
    let mut filtered = Vec::new();
    'candidate: for (idx, candidate) in candidates.iter().enumerate() {
        for (other_idx, other) in candidates.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            if crate::dominates(&other.objective, &candidate.objective) {
                continue 'candidate;
            }
        }
        filtered.push(candidate.clone());
    }
    if filtered.is_empty() {
        candidates
    } else {
        filtered.sort_by(|lhs, rhs| lhs.state.id.cmp(&rhs.state.id));
        filtered
    }
}

fn previous_cluster_ids(
    previous: Option<&SearchController>,
    clusters: &[Cluster],
    candidates: &[SearchCandidate],
) -> BTreeSet<usize> {
    let Some(previous) = previous else {
        return BTreeSet::new();
    };
    let previous_ids = previous
        .frontier
        .iter()
        .map(|candidate| candidate.state.id.as_u128())
        .collect::<BTreeSet<_>>();
    clusters
        .iter()
        .filter(|cluster| {
            cluster
                .members
                .iter()
                .any(|member| previous_ids.contains(&candidates[*member].state.id.as_u128()))
        })
        .map(|cluster| cluster.id)
        .collect()
}

fn cluster_priority(
    cluster: &Cluster,
    candidates: &[SearchCandidate],
    previous_cluster_ids: &BTreeSet<usize>,
) -> f64 {
    let variance = cluster_variance(cluster, candidates);
    let structural_spread = cluster
        .members
        .iter()
        .map(|member| feature_distance(candidates[*member].feature, cluster.centroid))
        .sum::<f64>()
        / cluster.members.len().max(1) as f64;
    let exploration_boost = if previous_cluster_ids.contains(&cluster.id) {
        0.0
    } else {
        0.25
    };
    variance + structural_spread + exploration_boost
}

fn pick_best_member(
    cluster: &Cluster,
    candidates: &[SearchCandidate],
    used: &BTreeSet<usize>,
    selected_indices: &[usize],
    beta: f64,
    global_centroid: SearchFeatureVector,
    previous_scores: &BTreeMap<u128, f64>,
    smoothed_scores: &mut BTreeMap<u128, f64>,
) -> Option<usize> {
    cluster
        .members
        .iter()
        .copied()
        .filter(|member| !used.contains(member))
        .max_by(|lhs, rhs| {
            smoothed_candidate_score(
                *lhs,
                candidates,
                selected_indices,
                beta,
                global_centroid,
                previous_scores,
            )
            .partial_cmp(&smoothed_candidate_score(
                *rhs,
                candidates,
                selected_indices,
                beta,
                global_centroid,
                previous_scores,
            ))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| candidates[*rhs].state.id.cmp(&candidates[*lhs].state.id))
        })
        .inspect(|idx| {
            let score = smoothed_candidate_score(
                *idx,
                candidates,
                selected_indices,
                beta,
                global_centroid,
                previous_scores,
            );
            smoothed_scores.insert(candidates[*idx].state.id.as_u128(), score);
        })
}

fn smoothed_candidate_score(
    idx: usize,
    candidates: &[SearchCandidate],
    selected_indices: &[usize],
    beta: f64,
    global_centroid: SearchFeatureVector,
    previous_scores: &BTreeMap<u128, f64>,
) -> f64 {
    let quality = crate::scalar_score(&candidates[idx].objective).clamp(0.0, 1.0);
    let diversity = diversity_score(idx, candidates, selected_indices, global_centroid);
    let raw_score = quality + beta * diversity;
    let previous = previous_scores
        .get(&candidates[idx].state.id.as_u128())
        .copied()
        .unwrap_or(raw_score);
    TEMPORAL_SMOOTHING_ALPHA * raw_score + (1.0 - TEMPORAL_SMOOTHING_ALPHA) * previous
}

fn diversity_score(
    idx: usize,
    candidates: &[SearchCandidate],
    selected_indices: &[usize],
    global_centroid: SearchFeatureVector,
) -> f64 {
    if selected_indices.is_empty() {
        return feature_distance(candidates[idx].feature, global_centroid);
    }
    selected_indices
        .iter()
        .map(|selected| feature_distance(candidates[idx].feature, candidates[*selected].feature))
        .fold(f64::INFINITY, f64::min)
        .clamp(0.0, 1.0)
}

fn mean_diversity(frontier: &[SearchCandidate]) -> f64 {
    if frontier.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    let mut count = 0usize;
    for idx in 0..frontier.len() {
        let candidate = &frontier[idx];
        let nearest = frontier
            .iter()
            .enumerate()
            .filter_map(|(other_idx, other)| {
                (idx != other_idx).then_some(feature_distance(candidate.feature, other.feature))
            })
            .fold(f64::INFINITY, f64::min);
        if nearest.is_finite() {
            total += nearest;
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

fn frontier_change_ratio(previous: Option<&SearchController>, frontier: &[SearchCandidate]) -> f64 {
    let Some(previous) = previous else {
        return 1.0;
    };
    let lhs = previous
        .frontier
        .iter()
        .map(|candidate| candidate.state.id.as_u128())
        .collect::<BTreeSet<_>>();
    let rhs = frontier
        .iter()
        .map(|candidate| candidate.state.id.as_u128())
        .collect::<BTreeSet<_>>();
    let union = lhs.union(&rhs).count();
    if union == 0 {
        return 0.0;
    }
    lhs.symmetric_difference(&rhs).count() as f64 / union as f64
}

fn selected_cluster_ids(
    clusters: &[Cluster],
    candidates: &[SearchCandidate],
    frontier: &[SearchCandidate],
) -> BTreeSet<usize> {
    let frontier_ids = frontier
        .iter()
        .map(|candidate| candidate.state.id.as_u128())
        .collect::<BTreeSet<_>>();
    clusters
        .iter()
        .filter(|cluster| {
            cluster
                .members
                .iter()
                .any(|member| frontier_ids.contains(&candidates[*member].state.id.as_u128()))
        })
        .map(|cluster| cluster.id)
        .collect()
}

fn objective_score_variance(frontier: &[SearchCandidate]) -> f64 {
    if frontier.len() < 2 {
        return 0.0;
    }
    let values = frontier
        .iter()
        .map(|candidate| crate::scalar_score(&candidate.objective))
        .collect::<Vec<_>>();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64
}

fn cluster_variance(cluster: &Cluster, candidates: &[SearchCandidate]) -> f64 {
    if cluster.members.len() < 2 {
        return 0.0;
    }
    let mean = cluster
        .members
        .iter()
        .map(|idx| crate::scalar_score(&candidates[*idx].objective))
        .sum::<f64>()
        / cluster.members.len() as f64;
    cluster
        .members
        .iter()
        .map(|idx| {
            let delta = crate::scalar_score(&candidates[*idx].objective) - mean;
            delta * delta
        })
        .sum::<f64>()
        / cluster.members.len() as f64
}

fn fallback_rule_based_clusters(
    candidates: &[SearchCandidate],
    max_clusters: usize,
) -> Vec<Cluster> {
    let mut buckets = BTreeMap::<usize, Vec<usize>>::new();
    let cycle_split = candidates
        .iter()
        .map(|candidate| candidate.feature.cycle_flag >= 0.5)
        .collect::<BTreeSet<_>>()
        .len()
        > 1;
    let coupling_mean = candidates
        .iter()
        .map(|candidate| candidate.feature.coupling)
        .sum::<f64>()
        / candidates.len() as f64;

    for (idx, candidate) in candidates.iter().enumerate() {
        let bucket = if cycle_split {
            usize::from(candidate.feature.cycle_flag >= 0.5)
        } else {
            usize::from(candidate.feature.coupling >= coupling_mean)
        };
        buckets.entry(bucket).or_default().push(idx);
    }

    if buckets.len() <= 1 {
        buckets.clear();
        for (idx, candidate) in candidates.iter().enumerate() {
            let bucket = usize::from(candidate.feature.coupling >= coupling_mean)
                + 2 * usize::from(candidate.feature.structural_variance >= 0.5);
            buckets.entry(bucket).or_default().push(idx);
        }
    }

    let features = candidates
        .iter()
        .map(|candidate| candidate.feature)
        .collect::<Vec<_>>();
    let mut clusters = buckets
        .into_values()
        .map(|mut members| {
            members.sort_unstable();
            Cluster {
                id: 0,
                centroid: centroid_of(&features, &members),
                members,
            }
        })
        .filter(|cluster| !cluster.members.is_empty())
        .collect::<Vec<_>>();
    clusters.sort_by(|lhs, rhs| lhs.members[0].cmp(&rhs.members[0]));
    merge_small_clusters(&mut clusters, &features);
    if clusters.len() > max_clusters {
        clusters.truncate(max_clusters);
    }
    for (idx, cluster) in clusters.iter_mut().enumerate() {
        cluster.id = idx;
        cluster.centroid = centroid_of(&features, &cluster.members);
    }
    clusters
}

fn desired_cluster_count(n: usize, max_clusters: usize) -> usize {
    if n <= 1 || max_clusters <= 1 {
        return 1;
    }
    let size_limited = (n / 2).max(1).min(max_clusters.max(1));
    size_limited.min(5)
}

fn initial_centroids(features: &[SearchFeatureVector], k: usize) -> Vec<SearchFeatureVector> {
    let mut ordered = features.iter().copied().enumerate().collect::<Vec<_>>();
    ordered.sort_by(|lhs, rhs| {
        feature_magnitude(lhs.1)
            .partial_cmp(&feature_magnitude(rhs.1))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| lhs.0.cmp(&rhs.0))
    });
    let last = ordered.len().saturating_sub(1);
    (0..k)
        .map(|slot| {
            let idx = if k <= 1 { 0 } else { slot * last / (k - 1) };
            ordered[idx.min(ordered.len() - 1)].1
        })
        .collect()
}

fn rebalance_empty_clusters(
    assignments: &mut [usize],
    features: &[SearchFeatureVector],
    k: usize,
    centroids: &[SearchFeatureVector],
) {
    for cluster_id in 0..k {
        if assignments.iter().any(|assigned| *assigned == cluster_id) {
            continue;
        }
        let mut donor = None::<usize>;
        let mut best_distance = f64::NEG_INFINITY;
        for (idx, feature) in features.iter().enumerate() {
            let current = assignments[idx];
            let cluster_size = assignments
                .iter()
                .filter(|assigned| **assigned == current)
                .count();
            if cluster_size <= MIN_CLUSTER_SIZE {
                continue;
            }
            let distance = feature_distance(*feature, centroids[cluster_id]);
            if distance > best_distance {
                best_distance = distance;
                donor = Some(idx);
            }
        }
        if let Some(donor_idx) = donor {
            assignments[donor_idx] = cluster_id;
        }
    }
}

fn merge_small_clusters(clusters: &mut Vec<Cluster>, features: &[SearchFeatureVector]) {
    loop {
        let Some((small_idx, _)) = clusters
            .iter()
            .enumerate()
            .find(|(_, cluster)| cluster.members.len() < MIN_CLUSTER_SIZE && clusters.len() > 1)
        else {
            break;
        };

        let member = clusters[small_idx].members[0];
        let mut best_target = None::<usize>;
        let mut best_distance = f64::INFINITY;
        for (idx, cluster) in clusters.iter().enumerate() {
            if idx == small_idx {
                continue;
            }
            let distance = feature_distance(features[member], cluster.centroid);
            if distance < best_distance {
                best_distance = distance;
                best_target = Some(idx);
            }
        }
        let Some(target_idx) = best_target else {
            break;
        };
        clusters[target_idx].members.push(member);
        clusters[target_idx].members.sort_unstable();
        clusters[target_idx].centroid = centroid_of(features, &clusters[target_idx].members);
        clusters.remove(small_idx);
    }
}

fn nearest_centroid(feature: SearchFeatureVector, centroids: &[SearchFeatureVector]) -> usize {
    centroids
        .iter()
        .enumerate()
        .min_by(|lhs, rhs| {
            feature_distance(feature, *lhs.1)
                .partial_cmp(&feature_distance(feature, *rhs.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn centroid_of(features: &[SearchFeatureVector], members: &[usize]) -> SearchFeatureVector {
    if members.is_empty() {
        return SearchFeatureVector {
            coupling: 0.0,
            propagation_score: 0.0,
            impact: 0.0,
            structural_variance: 0.0,
            cycle_flag: 0.0,
        };
    }
    let mut centroid = SearchFeatureVector {
        coupling: 0.0,
        propagation_score: 0.0,
        impact: 0.0,
        structural_variance: 0.0,
        cycle_flag: 0.0,
    };
    for member in members {
        let feature = features[*member];
        centroid.coupling += feature.coupling;
        centroid.propagation_score += feature.propagation_score;
        centroid.impact += feature.impact;
        centroid.structural_variance += feature.structural_variance;
        centroid.cycle_flag += feature.cycle_flag;
    }
    let denom = members.len() as f64;
    SearchFeatureVector {
        coupling: centroid.coupling / denom,
        propagation_score: centroid.propagation_score / denom,
        impact: centroid.impact / denom,
        structural_variance: centroid.structural_variance / denom,
        cycle_flag: centroid.cycle_flag / denom,
    }
}

fn feature_magnitude(feature: SearchFeatureVector) -> f64 {
    feature_distance(
        feature,
        SearchFeatureVector {
            coupling: 0.0,
            propagation_score: 0.0,
            impact: 0.0,
            structural_variance: 0.0,
            cycle_flag: 0.0,
        },
    )
}

fn feature_distance(lhs: SearchFeatureVector, rhs: SearchFeatureVector) -> f64 {
    let diffs = [
        lhs.coupling - rhs.coupling,
        lhs.propagation_score - rhs.propagation_score,
        lhs.impact - rhs.impact,
        lhs.structural_variance - rhs.structural_variance,
        lhs.cycle_flag - rhs.cycle_flag,
    ];
    let distance = diffs.iter().map(|diff| diff * diff).sum::<f64>().sqrt();
    (distance / 5.0f64.sqrt()).clamp(0.0, 1.0)
}

fn feature_vector_for_state(state: &DesignState) -> SearchFeatureVector {
    let graph = &state.graph;
    let nodes = graph.nodes().len();
    let edges = graph.edges().len();
    let max_possible_edges = nodes.saturating_mul(nodes.saturating_sub(1)) / 2;
    let edge_density = if max_possible_edges == 0 {
        0.0
    } else {
        edges as f64 / max_possible_edges as f64
    };
    let coupling = (0.55 * edge_density + 0.45 * graph.normalized_max_degree()).clamp(0.0, 1.0);
    let propagation_score = graph.average_reachable_ratio().clamp(0.0, 1.0);
    let impact = (graph.longest_path_depth_ratio() * coupling).clamp(0.0, 1.0);
    let structural_variance = graph.normalized_degree_variance().clamp(0.0, 1.0);
    let cycle_flag = if graph.is_dag() { 0.0 } else { 1.0 };

    SearchFeatureVector {
        coupling,
        propagation_score,
        impact,
        structural_variance,
        cycle_flag,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use memory_space::{DesignNode, StateId, StructuralGraph};

    use super::*;

    fn state_with_graph(id: u128, nodes: usize, edges: &[(u128, u128)]) -> DesignState {
        let mut graph = StructuralGraph::default();
        for node_id in 1..=nodes {
            graph = graph.with_node_added(DesignNode::new(
                Uuid::from_u128(node_id as u128 + id * 100),
                format!("N{node_id}"),
                BTreeMap::new(),
            ));
        }
        for (from, to) in edges {
            graph = graph.with_edge_added(
                Uuid::from_u128(from + id * 100),
                Uuid::from_u128(to + id * 100),
            );
        }
        DesignState::new(
            StateId::from_u128(id),
            Arc::new(graph),
            format!("history:{id}"),
        )
    }

    fn candidate(id: u128, edges: &[(u128, u128)], normalized: [f64; 4]) -> SearchCandidate {
        let state = state_with_graph(id, 5, edges);
        build_search_candidate(
            state,
            ObjectiveVector {
                f_struct: normalized[0],
                f_field: normalized[1],
                f_risk: normalized[2],
                f_shape: normalized[3],
            },
            Uuid::from_u128(id + 10_000),
            normalized,
            None,
        )
    }

    #[test]
    fn kmeans_clusters_structurally_distinct_candidates() {
        let features = vec![
            SearchFeatureVector {
                coupling: 0.9,
                propagation_score: 0.8,
                impact: 0.8,
                structural_variance: 0.7,
                cycle_flag: 0.0,
            },
            SearchFeatureVector {
                coupling: 0.85,
                propagation_score: 0.75,
                impact: 0.72,
                structural_variance: 0.65,
                cycle_flag: 0.0,
            },
            SearchFeatureVector {
                coupling: 0.15,
                propagation_score: 0.2,
                impact: 0.18,
                structural_variance: 0.12,
                cycle_flag: 0.0,
            },
            SearchFeatureVector {
                coupling: 0.2,
                propagation_score: 0.25,
                impact: 0.15,
                structural_variance: 0.1,
                cycle_flag: 0.0,
            },
            SearchFeatureVector {
                coupling: 0.55,
                propagation_score: 0.4,
                impact: 0.45,
                structural_variance: 0.8,
                cycle_flag: 0.0,
            },
            SearchFeatureVector {
                coupling: 0.5,
                propagation_score: 0.35,
                impact: 0.5,
                structural_variance: 0.85,
                cycle_flag: 0.0,
            },
        ];

        let clusters = DeterministicKMeansClusterer::cluster(&features, 5);
        assert!(clusters.len() >= 3);
        assert!(clusters.iter().all(|cluster| cluster.members.len() >= 2));
    }

    #[test]
    fn beta_scheduler_decays_on_growth_and_grows_on_stagnation() {
        let decayed = AdaptiveBetaScheduler::update(
            0.3,
            &SearchMetrics {
                hv_delta: 0.01,
                ..SearchMetrics::default()
            },
        );
        let grown = AdaptiveBetaScheduler::update(
            0.1,
            &SearchMetrics {
                stagnation_steps: 8,
                ..SearchMetrics::default()
            },
        );
        assert!(decayed < 0.3);
        assert!(grown > 0.1);
    }

    #[test]
    fn diversity_prefers_non_duplicate_candidate() {
        let candidates = vec![
            candidate(
                1,
                &[(1, 2), (2, 3), (3, 4), (4, 5)],
                [0.95, 0.95, 0.95, 0.95],
            ),
            candidate(
                2,
                &[(1, 2), (2, 3), (3, 4), (4, 5)],
                [0.94, 0.94, 0.94, 0.94],
            ),
            candidate(3, &[(1, 3), (1, 4), (1, 5)], [0.78, 0.82, 0.79, 0.77]),
            candidate(4, &[(1, 2)], [0.72, 0.74, 0.76, 0.71]),
        ];

        let outcome = select_structured_frontier(candidates, 3, 1, 10);
        assert!(outcome.controller.frontier.len() >= 3);
        let unique_feature_groups = outcome
            .controller
            .frontier
            .iter()
            .map(|candidate| candidate.state.id)
            .collect::<BTreeSet<_>>();
        assert!(unique_feature_groups.len() >= 3);
    }

    #[test]
    fn structured_search_is_deterministic() {
        let candidates = vec![
            candidate(1, &[(1, 2), (2, 3)], [0.8, 0.7, 0.6, 0.9]),
            candidate(2, &[(1, 3), (1, 4)], [0.7, 0.8, 0.7, 0.8]),
            candidate(3, &[(1, 2)], [0.6, 0.9, 0.75, 0.7]),
            candidate(4, &[(2, 3), (3, 4)], [0.9, 0.6, 0.8, 0.6]),
        ];
        let first = select_structured_frontier(candidates.clone(), 3, 1, 10);
        let second = select_structured_frontier(candidates, 3, 1, 10);
        assert_eq!(
            first
                .controller
                .frontier
                .iter()
                .map(|candidate| candidate.state.id)
                .collect::<Vec<_>>(),
            second
                .controller
                .frontier
                .iter()
                .map(|candidate| candidate.state.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cluster_collapse_is_detected_for_identical_features() {
        let candidates = vec![
            candidate(1, &[(1, 2), (2, 3)], [0.8, 0.8, 0.8, 0.8]),
            candidate(2, &[(1, 2), (2, 3)], [0.7, 0.7, 0.7, 0.7]),
            candidate(3, &[(1, 2), (2, 3)], [0.6, 0.6, 0.6, 0.6]),
        ];
        let outcome = select_structured_frontier(candidates, 2, 1, 10);
        assert!(outcome.cluster_collapsed);
    }

    #[test]
    fn cluster_manager_reconfigures_when_kmeans_collapses() {
        let candidates = vec![
            candidate(1, &[(1, 2), (2, 3), (3, 1)], [0.8, 0.6, 0.7, 0.8]),
            candidate(2, &[(1, 2), (2, 3), (3, 1)], [0.79, 0.59, 0.7, 0.79]),
            candidate(3, &[(1, 2)], [0.6, 0.8, 0.6, 0.7]),
            candidate(4, &[(1, 3)], [0.58, 0.78, 0.62, 0.68]),
        ];
        let features = candidates
            .iter()
            .map(|candidate| candidate.feature)
            .collect::<Vec<_>>();
        let mut clusters = vec![Cluster {
            id: 0,
            members: (0..candidates.len()).collect(),
            centroid: centroid_of(&features, &(0..candidates.len()).collect::<Vec<_>>()),
        }];
        let rebalanced = AdaptiveClusterManager::rebalance(&mut clusters, &candidates, 3, None);
        assert!(rebalanced);
        assert!(clusters.len() >= 2);
    }

    #[test]
    fn frontier_smoothing_limits_churn() {
        let candidates = vec![
            candidate(1, &[(1, 2), (2, 3), (3, 4)], [0.95, 0.35, 0.85, 0.25]),
            candidate(2, &[(1, 3), (1, 4), (1, 5)], [0.35, 0.95, 0.25, 0.85]),
            candidate(3, &[(1, 2)], [0.65, 0.55, 0.75, 0.7]),
            candidate(4, &[(2, 3), (3, 4)], [0.8, 0.7, 0.4, 0.8]),
            candidate(5, &[(1, 5)], [0.45, 0.8, 0.9, 0.55]),
            candidate(6, &[(1, 2), (1, 3)], [0.75, 0.45, 0.7, 0.9]),
        ];
        let first = select_structured_frontier(candidates.clone(), 5, 1, 20);
        let second = select_controlled_frontier(candidates, Some(&first.controller), 5, 2, 20);
        assert!(
            second.frontier_change_ratio <= 0.2,
            "change_ratio={}",
            second.frontier_change_ratio
        );
    }

    #[test]
    fn controlled_search_remains_stable_for_2000_steps() {
        let candidates = vec![
            candidate(1, &[(1, 2), (2, 3), (3, 4)], [0.95, 0.35, 0.85, 0.25]),
            candidate(2, &[(1, 3), (1, 4), (1, 5)], [0.35, 0.95, 0.25, 0.85]),
            candidate(3, &[(1, 2)], [0.65, 0.55, 0.75, 0.7]),
            candidate(4, &[(2, 3), (3, 4)], [0.8, 0.7, 0.4, 0.8]),
            candidate(5, &[(1, 5)], [0.45, 0.8, 0.9, 0.55]),
            candidate(6, &[(1, 2), (1, 3)], [0.75, 0.45, 0.7, 0.9]),
        ];
        let mut controller = None;
        let mut last_hv = 0.0;
        for step in 1..=2000 {
            let outcome =
                select_controlled_frontier(candidates.clone(), controller.as_ref(), 5, step, 2000);
            assert!(outcome.frontier_hv.is_finite());
            assert!(outcome.beta_used.is_finite());
            assert!(outcome.cluster_coverage.is_finite());
            assert!(outcome.frontier_hv + 1e-9 >= last_hv || outcome.hv_delta.abs() <= 1e-3);
            last_hv = outcome.frontier_hv.max(last_hv);
            controller = Some(outcome.controller);
        }
    }

    #[test]
    fn structured_frontier_hypervolume_exceeds_threshold() {
        let candidates = vec![
            candidate(1, &[(1, 2), (2, 3), (3, 4)], [0.95, 0.35, 0.85, 0.25]),
            candidate(2, &[(1, 3), (1, 4), (1, 5)], [0.35, 0.95, 0.25, 0.85]),
            candidate(3, &[(1, 2)], [0.65, 0.55, 0.75, 0.7]),
            candidate(4, &[(2, 3), (3, 4)], [0.8, 0.7, 0.4, 0.8]),
            candidate(5, &[(1, 5)], [0.45, 0.8, 0.9, 0.55]),
            candidate(6, &[(1, 2), (1, 3)], [0.75, 0.45, 0.7, 0.9]),
        ];
        let outcome = select_structured_frontier(candidates, 5, 1, 10);
        assert!(outcome.frontier_hv > 0.1, "hv={}", outcome.frontier_hv);
        assert!(
            outcome.cluster_coverage >= 0.8,
            "coverage={}",
            outcome.cluster_coverage
        );
        assert!(outcome.score_variance > 0.0);
    }
}
