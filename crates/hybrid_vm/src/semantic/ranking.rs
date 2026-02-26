use std::cmp::Ordering;

use semantic_dhm::{ConceptUnitV2 as SemanticUnitL2V2, RequirementKind};

const EPS: f64 = 1e-12;

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveCase {
    pub case_id: String,
    pub pareto_rank: usize,
    pub total_score: f64,
    pub l2: SemanticUnitL2V2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RankedCase {
    pub objective: ObjectiveCase,
    pub coherence: SemanticCoherence,
}

pub fn rank_frontier_by_semantic(frontier: Vec<ObjectiveCase>) -> Vec<RankedCase> {
    let mut ranked = frontier
        .into_iter()
        .map(|objective| RankedCase {
            coherence: compute_coherence(&objective.l2),
            objective,
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|a, b| {
        a.objective
            .pareto_rank
            .cmp(&b.objective.pareto_rank)
            .then_with(|| cmp_desc_f64(a.objective.total_score, b.objective.total_score))
            .then_with(|| cmp_desc_f64(a.coherence.total_score, b.coherence.total_score))
            .then_with(|| a.objective.case_id.cmp(&b.objective.case_id))
    });

    ranked
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCoherence {
    pub dependency: f64,
    pub abstraction: f64,
    pub polarity: f64,
    pub contradiction: f64,
    pub coverage: f64,
    pub total_score: f64,
}

fn compute_coherence(l2: &SemanticUnitL2V2) -> SemanticCoherence {
    let edge_count = l2.causal_links.len() as f64;
    let negative_edges = l2.causal_links.iter().filter(|e| e.weight < 0.0).count() as f64;
    let dependency = if edge_count <= EPS {
        0.5
    } else {
        clamp01(1.0 - negative_edges / edge_count)
    };

    let strengths = l2
        .derived_requirements
        .iter()
        .map(|r| r.strength as f64)
        .collect::<Vec<_>>();
    let abstraction = if strengths.is_empty() {
        0.0
    } else {
        let mean = strengths.iter().sum::<f64>() / strengths.len() as f64;
        let var = strengths.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / strengths.len() as f64;
        clamp01(1.0 - (var / 1.0))
    };

    let polarity = if edge_count <= EPS {
        1.0
    } else {
        clamp01(1.0 - negative_edges / edge_count)
    };
    let contradiction = if edge_count <= EPS {
        1.0
    } else {
        clamp01((-(negative_edges / edge_count)).exp())
    };

    let has = |k| l2.derived_requirements.iter().any(|r| r.kind == k);
    let covered = [
        has(RequirementKind::Performance),
        has(RequirementKind::Memory),
        has(RequirementKind::Security),
        has(RequirementKind::Reliability),
        has(RequirementKind::NoCloud),
    ]
    .into_iter()
    .filter(|v| *v)
    .count() as f64;
    let coverage = clamp01(covered / 5.0);

    let total_score = clamp01((dependency + abstraction + polarity + contradiction + coverage) / 5.0);

    SemanticCoherence {
        dependency,
        abstraction,
        polarity,
        contradiction,
        coverage,
        total_score,
    }
}

fn cmp_desc_f64(a: f64, b: f64) -> Ordering {
    if (a - b).abs() <= EPS {
        Ordering::Equal
    } else {
        b.total_cmp(&a)
    }
}

fn clamp01(v: f64) -> f64 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}
