use std::collections::BTreeSet;

use architecture_domain::{ArchitectureState, ComponentRole};

use crate::{
    ranking::{RankedCandidate, rank_candidates},
    search_state::SearchState,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SearchNodeDiversityPruned {
    pub node_id: u64,
    pub similarity: f64,
    pub pruned_by: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PruneCandidatesOutcome {
    pub selected: Vec<SearchState>,
    pub diversity_pruned: Vec<SearchNodeDiversityPruned>,
}

/// Retain only the top `beam_width` candidates by Pareto-aware ranking plus diversity filtering.
pub fn prune_candidates(
    states: Vec<SearchState>,
    beam_width: usize,
    diversity_threshold: f64,
) -> Vec<SearchState> {
    prune_candidates_with_telemetry(states, beam_width, diversity_threshold).selected
}

pub fn prune_candidates_with_telemetry(
    states: Vec<SearchState>,
    beam_width: usize,
    diversity_threshold: f64,
) -> PruneCandidatesOutcome {
    let ranked = rank_candidates(states);
    select_diverse_nodes(ranked, beam_width, diversity_threshold)
}

pub fn select_diverse_nodes(
    ranked: Vec<RankedCandidate>,
    k: usize,
    threshold: f64,
) -> PruneCandidatesOutcome {
    let threshold = threshold.clamp(0.0, 1.0);
    let target = k.max(1);
    let mut selected: Vec<SearchState> = Vec::with_capacity(target);
    let mut deferred = Vec::new();

    for candidate in ranked {
        let mut similar = None;
        for existing in &selected {
            let similarity = architecture_similarity(
                &candidate.state.architecture_state,
                &existing.architecture_state,
            );
            if similarity > threshold {
                similar = Some((existing.state_id, similarity));
                break;
            }
        }

        if let Some((pruned_by, similarity)) = similar {
            let state = candidate.state;
            let node_id = state.state_id;
            deferred.push((
                state,
                SearchNodeDiversityPruned {
                    node_id,
                    similarity,
                    pruned_by,
                },
            ));
            continue;
        }

        selected.push(candidate.state);
        if selected.len() == target {
            break;
        }
    }

    while selected.len() < target && !deferred.is_empty() {
        let (state, _) = deferred.remove(0);
        selected.push(state);
    }

    let diversity_pruned = deferred
        .into_iter()
        .map(|(_, event)| SearchNodeDiversityPruned {
            node_id: event.node_id,
            similarity: event.similarity,
            pruned_by: event.pruned_by,
        })
        .collect();

    PruneCandidatesOutcome {
        selected,
        diversity_pruned,
    }
}

pub fn architecture_similarity(lhs: &ArchitectureState, rhs: &ArchitectureState) -> f64 {
    let lhs_vector = architecture_feature_vector(lhs);
    let rhs_vector = architecture_feature_vector(rhs);
    let cosine = cosine_similarity(&lhs_vector, &rhs_vector);
    let component_overlap =
        jaccard_similarity(component_signatures(lhs), component_signatures(rhs));
    let dependency_overlap =
        jaccard_similarity(dependency_signatures(lhs), dependency_signatures(rhs));
    let topology_match = if lhs.deployment.topology == rhs.deployment.topology {
        1.0
    } else {
        0.0
    };

    (0.45 * cosine + 0.25 * component_overlap + 0.20 * dependency_overlap + 0.10 * topology_match)
        .clamp(0.0, 1.0)
}

fn architecture_feature_vector(state: &ArchitectureState) -> Vec<f64> {
    let role_count = |role: ComponentRole| {
        state
            .components
            .iter()
            .filter(|component| component.role == role)
            .count() as f64
    };

    vec![
        state.metrics.component_count as f64,
        state.metrics.dependency_count as f64,
        state.metrics.layering_score,
        state.deployment.replicas as f64,
        state.constraints.len() as f64,
        role_count(ComponentRole::Controller),
        role_count(ComponentRole::Service),
        role_count(ComponentRole::Repository),
        role_count(ComponentRole::Database),
        role_count(ComponentRole::Gateway),
        state
            .components
            .iter()
            .filter(|component| matches!(component.role, ComponentRole::Unknown(_)))
            .count() as f64,
    ]
}

fn component_signatures(state: &ArchitectureState) -> BTreeSet<String> {
    state
        .components
        .iter()
        .map(|component| {
            format!(
                "{}:{:?}:{}:{}",
                component.id.0,
                component.role,
                component.inputs.len(),
                component.outputs.len()
            )
        })
        .collect()
}

fn dependency_signatures(state: &ArchitectureState) -> BTreeSet<String> {
    state
        .dependencies
        .iter()
        .map(|dependency| {
            format!(
                "{}->{}:{:?}",
                dependency.from.0, dependency.to.0, dependency.kind
            )
        })
        .collect()
}

fn cosine_similarity(lhs: &[f64], rhs: &[f64]) -> f64 {
    let dot = lhs.iter().zip(rhs).map(|(l, r)| l * r).sum::<f64>();
    let lhs_norm = lhs.iter().map(|value| value * value).sum::<f64>().sqrt();
    let rhs_norm = rhs.iter().map(|value| value * value).sum::<f64>().sqrt();
    if lhs_norm == 0.0 && rhs_norm == 0.0 {
        1.0
    } else if lhs_norm == 0.0 || rhs_norm == 0.0 {
        0.0
    } else {
        (dot / (lhs_norm * rhs_norm)).clamp(0.0, 1.0)
    }
}

fn jaccard_similarity(lhs: BTreeSet<String>, rhs: BTreeSet<String>) -> f64 {
    if lhs.is_empty() && rhs.is_empty() {
        return 1.0;
    }
    let intersection = lhs.intersection(&rhs).count() as f64;
    let union = lhs.union(&rhs).count() as f64;
    if union == 0.0 {
        1.0
    } else {
        (intersection / union).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use architecture_domain::ArchitectureState;
    use world_model_core::WorldState;

    use super::*;
    use crate::search_state::SearchState;

    fn search_state_with_architecture(
        state_id: u64,
        world_id: u64,
        features: Vec<f64>,
    ) -> SearchState {
        let world_state = WorldState::new(world_id, features);
        let architecture_state =
            ArchitectureState::from_architecture(&world_state.architecture, Vec::new());
        let mut state = SearchState::new(state_id, world_state);
        state.architecture_state = architecture_state;
        state.score = 0.9 - state_id as f64 * 0.01;
        state
    }

    #[test]
    fn architecture_similarity_is_high_for_identical_architectures() {
        let lhs = search_state_with_architecture(1, 10, vec![1.0, 0.5, 0.2]);
        let rhs = search_state_with_architecture(2, 11, vec![1.0, 0.5, 0.2]);

        let similarity = architecture_similarity(&lhs.architecture_state, &rhs.architecture_state);

        assert!(similarity > 0.99, "similarity was {similarity}");
    }

    #[test]
    fn select_diverse_nodes_prunes_near_duplicate_architectures() {
        let first = search_state_with_architecture(1, 10, vec![1.0, 0.5, 0.2]);
        let duplicate = search_state_with_architecture(2, 11, vec![1.0, 0.5, 0.2]);
        let distinct = search_state_with_architecture(3, 12, vec![3.0, 2.0, 1.0]);

        let ranked = rank_candidates(vec![first.clone(), duplicate, distinct.clone()]);
        let outcome = select_diverse_nodes(ranked, 2, 0.85);

        assert_eq!(outcome.selected.len(), 2);
        assert_eq!(outcome.selected[0].state_id, first.state_id);
        assert_eq!(outcome.selected[1].state_id, distinct.state_id);
        assert_eq!(outcome.diversity_pruned.len(), 1);
        assert_eq!(outcome.diversity_pruned[0].node_id, 2);
        assert_eq!(outcome.diversity_pruned[0].pruned_by, 1);
    }
}
