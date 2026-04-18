use super::planner;
use super::rationality;
use super::{
    DesignDelta, DesignGraph, MutationCandidate, MutationPlan, MutationSearchResult,
    MutationStrategy, SEARCH_RATIONALITY_THRESHOLD,
};

const MAX_DEPTH: usize = 3;
const BEAM_WIDTH: usize = 4;

pub fn search_mutations(
    graph: &DesignGraph,
    delta: &DesignDelta,
    spec: &str,
) -> MutationSearchResult {
    let generated = generate_candidates(graph, delta, spec);
    let mut frontier = generated.clone();
    let mut rejected = Vec::new();

    for _depth in 1..MAX_DEPTH {
        frontier = prune_dominated(frontier, &mut rejected);
        frontier.sort_by(compare_candidates);
        frontier = retain_diverse_candidates(frontier, BEAM_WIDTH);
    }

    let mut candidates = prune_dominated(frontier, &mut rejected);
    candidates.sort_by(compare_candidates);
    candidates = retain_diverse_candidates(candidates, BEAM_WIDTH.max(4));
    if candidates.len() < 4 {
        let mut fallback_candidates = generated;
        fallback_candidates.sort_by(compare_candidates);
        candidates = retain_diverse_candidates(fallback_candidates, 4);
    }
    let selected = candidates
        .iter()
        .find(|candidate| {
            candidate
                .expected_score
                .as_ref()
                .map(|score| score.total >= SEARCH_RATIONALITY_THRESHOLD)
                .unwrap_or(false)
        })
        .cloned()
        .or_else(|| candidates.first().cloned());

    MutationSearchResult {
        candidates,
        selected,
        rejected,
    }
}

pub fn generate_candidates(
    graph: &DesignGraph,
    delta: &DesignDelta,
    spec: &str,
) -> Vec<MutationCandidate> {
    let strategies = [
        MutationStrategy::Conservative,
        MutationStrategy::TraitExtraction,
        MutationStrategy::AdapterInsertion,
        MutationStrategy::DependencyInversion,
        MutationStrategy::CrateSplit,
        MutationStrategy::InterfacePromotion,
    ];

    strategies
        .into_iter()
        .map(|strategy| candidate_for_strategy(graph, delta, spec, strategy))
        .collect()
}

fn candidate_for_strategy(
    graph: &DesignGraph,
    delta: &DesignDelta,
    spec: &str,
    strategy: MutationStrategy,
) -> MutationCandidate {
    let mut plan = planner::build_mutation_plan(
        graph,
        delta,
        spec,
        matches!(strategy, MutationStrategy::Conservative),
    );
    tune_plan_for_strategy(&mut plan, strategy, spec);
    let expected_score = Some(rationality::score_mutation_plan(graph, &plan));
    MutationCandidate {
        id: format!("{}-{}", strategy_label(strategy), plan.target_files.len()),
        plan,
        expected_score,
        strategy,
    }
}

fn tune_plan_for_strategy(plan: &mut MutationPlan, strategy: MutationStrategy, spec: &str) {
    match strategy {
        MutationStrategy::Conservative => {
            if plan.delta.impacted_crates.len() > 1 {
                plan.delta.impacted_crates.truncate(1);
                plan.target_files.truncate(1);
                plan.rollback_units.truncate(1);
            }
        }
        MutationStrategy::TraitExtraction => {
            plan.delta.introduced_interfaces.extend(
                plan.delta
                    .impacted_crates
                    .iter()
                    .map(|krate| format!("{krate}::TraitFacade")),
            );
            plan.expected_tests
                .push("cargo check --workspace".to_string());
        }
        MutationStrategy::AdapterInsertion => {
            plan.delta.introduced_interfaces.extend(
                plan.delta
                    .impacted_crates
                    .iter()
                    .map(|krate| format!("{krate}::Adapter")),
            );
            if plan.delta.impacted_crates.len() > 1 {
                plan.delta.impacted_crates.truncate(2);
                plan.target_files.truncate(2);
            }
        }
        MutationStrategy::CrateSplit => {
            let base = plan
                .delta
                .impacted_crates
                .first()
                .cloned()
                .unwrap_or_default();
            if !base.is_empty() {
                plan.delta.api_changes.push(super::ApiChange {
                    crate_name: base.clone(),
                    surface: "module-boundary".to_string(),
                    change: "crate-split".to_string(),
                });
                plan.rollback_units.push(format!("split::{base}"));
            }
        }
        MutationStrategy::InterfacePromotion => {
            plan.delta.introduced_interfaces.extend(
                plan.delta
                    .impacted_crates
                    .iter()
                    .map(|krate| format!("{krate}::PromotedInterface")),
            );
        }
        MutationStrategy::DependencyInversion => {
            if plan.delta.dependency_moves.is_empty() && plan.delta.impacted_crates.len() >= 2 {
                plan.delta.dependency_moves = plan
                    .delta
                    .impacted_crates
                    .windows(2)
                    .map(|pair| (pair[1].clone(), pair[0].clone()))
                    .collect();
            }
            if spec.to_lowercase().contains("adapter") {
                plan.expected_tests
                    .push("cargo test --workspace".to_string());
            }
        }
    }

    plan.expected_tests.sort();
    plan.expected_tests.dedup();
    plan.rollback_units.sort();
    plan.rollback_units.dedup();
    plan.delta.introduced_interfaces.sort();
    plan.delta.introduced_interfaces.dedup();
}

pub fn prune_dominated(
    candidates: Vec<MutationCandidate>,
    rejected: &mut Vec<String>,
) -> Vec<MutationCandidate> {
    let mut kept = Vec::new();

    'candidate: for candidate in candidates {
        for existing in &kept {
            if dominates(existing, &candidate) {
                rejected.push(candidate.id.clone());
                continue 'candidate;
            }
        }
        kept.retain(|existing| {
            let dominated = dominates(&candidate, existing);
            if dominated {
                rejected.push(existing.id.clone());
            }
            !dominated
        });
        kept.push(candidate);
    }

    kept
}

fn retain_diverse_candidates(
    candidates: Vec<MutationCandidate>,
    limit: usize,
) -> Vec<MutationCandidate> {
    let mut selected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for candidate in &candidates {
        if seen.insert(candidate.strategy) {
            selected.push(candidate.clone());
        }
        if selected.len() >= limit {
            return selected;
        }
    }

    for candidate in candidates {
        if selected.iter().any(|existing| existing.id == candidate.id) {
            continue;
        }
        selected.push(candidate);
        if selected.len() >= limit {
            break;
        }
    }

    selected
}

fn dominates(left: &MutationCandidate, right: &MutationCandidate) -> bool {
    let same_impacted = left.plan.delta.impacted_crates == right.plan.delta.impacted_crates;
    let same_targets = left.plan.target_files == right.plan.target_files;
    let Some(left_score) = left.expected_score.as_ref() else {
        return false;
    };
    let Some(right_score) = right.expected_score.as_ref() else {
        return false;
    };

    same_impacted
        && same_targets
        && ranking_value(left) >= ranking_value(right)
        && left_score.rollback_complexity >= right_score.rollback_complexity
        && left.strategy != right.strategy
}

fn compare_candidates(left: &MutationCandidate, right: &MutationCandidate) -> std::cmp::Ordering {
    ranking_value(right)
        .partial_cmp(&ranking_value(left))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| left.strategy.cmp(&right.strategy))
        .then_with(|| left.id.cmp(&right.id))
}

fn ranking_value(candidate: &MutationCandidate) -> f32 {
    let Some(score) = candidate.expected_score.as_ref() else {
        return 0.0;
    };
    let blast_radius_inverse = rationality::blast_radius_inverse(&candidate.plan);
    (0.30 * score.boundary_integrity)
        + (0.20 * score.maintainability)
        + (0.20 * score.extensibility)
        + (0.15 * score.rollback_complexity)
        + (0.15 * blast_radius_inverse)
}

fn strategy_label(strategy: MutationStrategy) -> &'static str {
    match strategy {
        MutationStrategy::Conservative => "conservative",
        MutationStrategy::TraitExtraction => "trait-extraction",
        MutationStrategy::AdapterInsertion => "adapter-insertion",
        MutationStrategy::CrateSplit => "crate-split",
        MutationStrategy::InterfacePromotion => "interface-promotion",
        MutationStrategy::DependencyInversion => "dependency-inversion",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{CrateNode, DesignGraph};

    fn sample_graph() -> DesignGraph {
        DesignGraph {
            workspace_root: PathBuf::from("."),
            crates: vec![
                CrateNode {
                    name: "design_cli".to_string(),
                    manifest_path: PathBuf::from("apps/cli/Cargo.toml"),
                    internal_dependencies: vec!["agent_core".to_string()],
                },
                CrateNode {
                    name: "agent_core".to_string(),
                    manifest_path: PathBuf::from("crates/agent_core/Cargo.toml"),
                    internal_dependencies: Vec::new(),
                },
            ],
            dependency_edges: vec![("design_cli".to_string(), "agent_core".to_string())],
        }
    }

    #[test]
    fn generates_at_least_four_candidates() {
        let graph = sample_graph();
        let delta = planner::extract_design_delta(
            &graph,
            "trait分離案とadapter案を比較して crate split案を含めて最も保守性の高い案で進めて",
        );
        let result = search_mutations(&graph, &delta, "比較して");
        assert!(result.candidates.len() >= 4);
        assert!(result.selected.is_some());
    }

    #[test]
    fn ranking_is_stable_across_reruns() {
        let graph = sample_graph();
        let delta =
            planner::extract_design_delta(&graph, "複数の設計変更案を比較して最適案で実装して");
        let first = search_mutations(&graph, &delta, "比較して");
        let second = search_mutations(&graph, &delta, "比較して");
        assert_eq!(
            first.selected.as_ref().map(|c| c.id.clone()),
            second.selected.as_ref().map(|c| c.id.clone())
        );
    }
}
