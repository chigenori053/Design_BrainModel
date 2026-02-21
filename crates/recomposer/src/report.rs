use semantic_dhm::{ConceptUnit, ResonanceWeights};

use crate::Recomposer;
use crate::consistency::{ConsistencyReport, compute_consistency};
use crate::explain::round2;
use crate::recommend::{RecommendationInput, RecommendationReport};

#[derive(Clone, Debug, PartialEq)]
pub struct DesignReport {
    pub summary: String,
    pub abstraction_mean: f32,
    pub abstraction_variance: f32,
    pub consistency: ConsistencyReport,
    pub global_coherence: f32,
    pub recommendations: RecommendationReport,
}

impl Recomposer {
    pub fn generate_report(
        &self,
        concepts: &[ConceptUnit],
        weights: &ResonanceWeights,
        top_k: usize,
    ) -> DesignReport {
        let abstraction_mean = abstraction_mean(concepts);
        let abstraction_variance = abstraction_variance(concepts, abstraction_mean);

        let consistency = compute_consistency(concepts, weights);
        let recommendation_report = build_recommendations(self, concepts, weights, top_k);
        let summary = select_summary(&consistency.report);

        DesignReport {
            summary: summary.to_string(),
            abstraction_mean: round2(abstraction_mean),
            abstraction_variance: round2(abstraction_variance),
            consistency: ConsistencyReport {
                directional_conflicts: consistency.report.directional_conflicts,
                structural_conflicts: consistency.report.structural_conflicts,
                tradeoffs: consistency.report.tradeoffs,
                stability_score: round2(consistency.report.stability_score),
            },
            global_coherence: round2(consistency.global_coherence),
            recommendations: recommendation_report,
        }
    }
}

fn abstraction_mean(concepts: &[ConceptUnit]) -> f32 {
    if concepts.is_empty() {
        return 0.0;
    }
    concepts.iter().map(|c| c.a).sum::<f32>() / concepts.len() as f32
}

fn abstraction_variance(concepts: &[ConceptUnit], mean: f32) -> f32 {
    if concepts.is_empty() {
        return 0.0;
    }
    concepts
        .iter()
        .map(|c| {
            let d = c.a - mean;
            d * d
        })
        .sum::<f32>()
        / concepts.len() as f32
}

fn build_recommendations(
    recomposer: &Recomposer,
    concepts: &[ConceptUnit],
    weights: &ResonanceWeights,
    top_k: usize,
) -> RecommendationReport {
    if concepts.is_empty() {
        return RecommendationReport {
            summary: "No recommendation candidates available.".to_string(),
            recommendations: Vec::new(),
        };
    }

    let mut sorted = concepts.to_vec();
    sorted.sort_by(|l, r| l.id.cmp(&r.id));

    let query = sorted[0].clone();
    let candidates = sorted.into_iter().skip(1).collect::<Vec<_>>();
    if candidates.is_empty() {
        return RecommendationReport {
            summary: "No recommendation candidates available.".to_string(),
            recommendations: Vec::new(),
        };
    }

    let capped_top_k = top_k.max(1).min(candidates.len());
    recomposer.recommend(
        &RecommendationInput {
            query,
            candidates,
            top_k: capped_top_k,
        },
        weights,
    )
}

fn select_summary(consistency: &ConsistencyReport) -> &'static str {
    if consistency.structural_conflicts > 0 {
        "Design contains structural contradictions."
    } else if consistency.directional_conflicts > 0 {
        "Design contains directional conflicts."
    } else if consistency.stability_score < 0.60 {
        "Design stability is moderate."
    } else {
        "Design structure is stable."
    }
}

#[cfg(test)]
mod tests {
    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::Recomposer;

    fn query(v0: f32, v1: f32, a: f32, s0: f32, s1: f32) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = v0;
        v[1] = v1;
        s[0] = s0;
        s[1] = s1;
        ConceptQuery {
            v,
            a,
            s,
            polarity: 0,
        }
    }

    #[test]
    fn report_mean_and_variance_are_correct() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.2, 1.0, 0.0));
        let id2 = dhm.insert_query(&query(0.0, 1.0, 0.6, 0.0, 1.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let r = Recomposer;
        let out = r.generate_report(&concepts, &dhm.weights(), 2);
        assert_eq!(out.abstraction_mean, 0.40);
        assert_eq!(out.abstraction_variance, 0.04);
    }

    #[test]
    fn report_coherence_n1_is_one() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id = dhm.insert_query(&query(1.0, 0.0, 0.5, 1.0, 0.0));
        let concepts = vec![dhm.get(id).expect("c")];

        let r = Recomposer;
        let out = r.generate_report(&concepts, &dhm.weights(), 2);
        assert_eq!(out.global_coherence, 1.0);
        assert_eq!(out.consistency.stability_score, 1.0);
    }

    #[test]
    fn report_handles_n0() {
        let dhm = SemanticDhm::in_memory().expect("mem");
        let r = Recomposer;
        let out = r.generate_report(&[], &dhm.weights(), 2);
        assert_eq!(out.abstraction_mean, 0.0);
        assert_eq!(out.abstraction_variance, 0.0);
        assert_eq!(out.global_coherence, 1.0);
        assert!(out.recommendations.recommendations.is_empty());
    }

    #[test]
    fn summary_branch_structural_conflict_wins() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.1, 1.0, 0.0));
        let id2 = dhm.insert_query(&query(-1.0, 0.0, 0.9, -1.0, 0.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let r = Recomposer;
        let out = r.generate_report(&concepts, &dhm.weights(), 2);
        assert_eq!(out.summary, "Design contains structural contradictions.");
    }
}
