use super::rationality::blast_radius_inverse;
use super::{MutationCandidate, MutationSearchResult, TradeoffExplanation, TradeoffPoint};

pub fn explain_tradeoff(search_result: &MutationSearchResult) -> Option<TradeoffExplanation> {
    let selected = search_result.selected.as_ref()?;
    let rejected = search_result
        .candidates
        .iter()
        .filter(|candidate| candidate.id != selected.id)
        .collect::<Vec<_>>();
    let benchmark = rejected.first().copied().or_else(|| {
        search_result
            .candidates
            .iter()
            .find(|c| c.id != selected.id)
    })?;

    let tradeoff_points = vec![
        make_point("boundary integrity", selected, benchmark),
        make_point("maintainability", selected, benchmark),
        make_point("extensibility", selected, benchmark),
        make_point("rollback complexity", selected, benchmark),
        make_point("blast radius", selected, benchmark),
    ];

    Some(TradeoffExplanation {
        selected_id: selected.id.clone(),
        rejected_ids: search_result.rejected.clone(),
        rationale_summary: rationale_summary(selected, benchmark),
        tradeoff_points,
        adoption_reason: adoption_reason(selected, benchmark),
    })
}

pub fn render_pr_ready_block(explanation: &TradeoffExplanation) -> String {
    let mut lines = vec![
        format!("Selected mutation: {}", explanation.selected_id),
        String::new(),
        "Tradeoff summary:".to_string(),
    ];
    for point in &explanation.tradeoff_points {
        lines.push(format!("- {}", point.explanation));
    }
    lines.push(String::new());
    lines.push("Rejected:".to_string());
    if explanation.rejected_ids.is_empty() {
        lines.push("- none".to_string());
    } else {
        for rejected in &explanation.rejected_ids {
            lines.push(format!("- {rejected}"));
        }
    }
    lines.push(String::new());
    lines.push(format!("Adoption reason: {}", explanation.adoption_reason));
    lines.join("\n")
}

fn rationale_summary(selected: &MutationCandidate, benchmark: &MutationCandidate) -> String {
    format!(
        "{} outranked {} by improving boundary integrity and extensibility while preserving rollback isolation.",
        selected.id, benchmark.id
    )
}

fn adoption_reason(selected: &MutationCandidate, benchmark: &MutationCandidate) -> String {
    let selected_score = selected.expected_score.as_ref().expect("selected score");
    let benchmark_score = benchmark.expected_score.as_ref().expect("benchmark score");
    format!(
        "{} was selected because it improved boundary integrity ({:+.2}) and extensibility ({:+.2}) while keeping rollback complexity acceptable.",
        selected.id,
        selected_score.boundary_integrity - benchmark_score.boundary_integrity,
        selected_score.extensibility - benchmark_score.extensibility,
    )
}

fn make_point(
    dimension: &str,
    selected: &MutationCandidate,
    rejected: &MutationCandidate,
) -> TradeoffPoint {
    let selected_score = dimension_score(dimension, selected);
    let rejected_score = dimension_score(dimension, rejected);
    let delta = selected_score - rejected_score;
    let explanation = if delta >= 0.0 {
        format!("Better {dimension} ({:+.2}) versus {}", delta, rejected.id)
    } else {
        format!(
            "Slightly weaker {dimension} ({:+.2}) than {} but accepted for overall ranking",
            delta, rejected.id
        )
    };
    TradeoffPoint {
        dimension: dimension.to_string(),
        selected_score,
        rejected_score,
        explanation,
    }
}

fn dimension_score(dimension: &str, candidate: &MutationCandidate) -> f32 {
    let score = candidate.expected_score.as_ref().expect("candidate score");
    match dimension {
        "boundary integrity" => score.boundary_integrity,
        "maintainability" => score.maintainability,
        "extensibility" => score.extensibility,
        "rollback complexity" => score.rollback_complexity,
        "blast radius" => blast_radius_inverse(&candidate.plan),
        _ => score.total,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{
        DesignDelta, MutationPlan, MutationSearchResult, MutationStrategy, RationalityScore,
    };

    fn candidate(
        id: &str,
        strategy: MutationStrategy,
        boundary: f32,
        extensibility: f32,
    ) -> MutationCandidate {
        MutationCandidate {
            id: id.to_string(),
            strategy,
            plan: MutationPlan {
                delta: DesignDelta {
                    impacted_crates: vec!["design_cli".to_string()],
                    ..DesignDelta::default()
                },
                target_files: vec![PathBuf::from("apps/cli/Cargo.toml")],
                expected_tests: vec!["cargo test -p design_cli".to_string()],
                rollback_units: vec!["crate::design_cli".to_string()],
            },
            expected_score: Some(RationalityScore {
                maintainability: 0.90,
                extensibility,
                risk: 0.80,
                boundary_integrity: boundary,
                rollback_complexity: 0.87,
                total: 0.88,
            }),
        }
    }

    #[test]
    fn explanation_contains_five_tradeoff_dimensions() {
        let selected = candidate(
            "trait-extraction-1",
            MutationStrategy::TraitExtraction,
            0.92,
            0.91,
        );
        let rejected = candidate(
            "adapter-insertion-1",
            MutationStrategy::AdapterInsertion,
            0.81,
            0.80,
        );
        let explanation = explain_tradeoff(&MutationSearchResult {
            candidates: vec![selected.clone(), rejected.clone()],
            selected: Some(selected),
            rejected: vec![rejected.id.clone()],
        })
        .expect("explanation");
        assert_eq!(explanation.tradeoff_points.len(), 5);
        assert!(render_pr_ready_block(&explanation).contains("Selected mutation:"));
    }
}
