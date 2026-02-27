use std::cmp::Ordering;

use semantic_dhm::ConceptUnitV2 as SemanticUnitL2V2;

use super::coherence::{HumanCoherence, compute_human_coherence};

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
    pub human_coherence: HumanCoherence,
}

pub fn rank_frontier_by_human_coherence(frontier: Vec<ObjectiveCase>) -> Vec<RankedCase> {
    let mut ranked = frontier
        .into_iter()
        .map(|objective| RankedCase {
            human_coherence: compute_human_coherence(&objective.l2),
            objective,
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|a, b| {
        // Pareto rank is 1-based in this codebase. Apply HC for first+second layers.
        let hc_cmp = if a.objective.pareto_rank <= 2 && b.objective.pareto_rank <= 2 {
            cmp_desc_f64(a.human_coherence.score, b.human_coherence.score)
        } else {
            Ordering::Equal
        };
        a.objective
            .pareto_rank
            .cmp(&b.objective.pareto_rank)
            .then_with(|| cmp_desc_f64(a.objective.total_score, b.objective.total_score))
            .then_with(|| hc_cmp)
            .then_with(|| a.objective.case_id.cmp(&b.objective.case_id))
    });

    ranked
}

fn cmp_desc_f64(a: f64, b: f64) -> Ordering {
    if (a - b).abs() <= EPS {
        Ordering::Equal
    } else {
        b.total_cmp(&a)
    }
}
