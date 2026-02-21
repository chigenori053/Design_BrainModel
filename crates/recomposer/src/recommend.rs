use semantic_dhm::{ConceptId, ConceptQuery, ConceptUnit, ResonanceWeights, resonance};

use crate::Recomposer;
use crate::consistency::compute_consistency;
use crate::constants::STRUCTURAL_CONFLICT_THRESHOLD;
use crate::explain::round2;

#[derive(Clone, Debug, PartialEq)]
pub struct RecommendationInput {
    pub query: ConceptUnit,
    pub candidates: Vec<ConceptUnit>,
    pub top_k: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionType {
    ResolveStructuralConflict,
    HighlightTradeoff,
    Merge,
    Refine,
    ApplyPattern,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub target: Option<ConceptId>,
    pub target_pair: Option<(ConceptId, ConceptId)>,
    pub action: ActionType,
    pub score: f32,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecommendationReport {
    pub summary: String,
    pub recommendations: Vec<Recommendation>,
}

impl Recomposer {
    pub fn recommend(
        &self,
        input: &RecommendationInput,
        weights: &ResonanceWeights,
    ) -> RecommendationReport {
        let q = ConceptQuery {
            v: input.query.integrated_vector.clone(),
            a: input.query.a,
            s: input.query.s.clone(),
            polarity: input.query.polarity,
        }
        .normalized();

        let mut candidates = input.candidates.clone();
        candidates.sort_by(|l, r| l.id.cmp(&r.id));

        if candidates.is_empty() {
            return RecommendationReport {
                summary: "No recommendation candidates available.".to_string(),
                recommendations: Vec::new(),
            };
        }

        let mut all_concepts = Vec::with_capacity(candidates.len() + 1);
        all_concepts.push(input.query.clone());
        all_concepts.extend(candidates.clone());
        all_concepts.sort_by(|l, r| l.id.cmp(&r.id));

        if let Some((left, right, score)) = first_structural_conflict_pair(&all_concepts, weights) {
            return RecommendationReport {
                summary: "Conflict areas require attention.".to_string(),
                recommendations: vec![Recommendation {
                    target: None,
                    target_pair: Some((left, right)),
                    action: ActionType::ResolveStructuralConflict,
                    score: round2(score),
                    rationale: format!(
                        "Resolve structural conflict between Concept {} and Concept {}.",
                        left.0, right.0
                    ),
                }],
            };
        }

        let consistency = compute_consistency(&all_concepts, weights);
        if let Some(t) = consistency.report.tradeoffs.first() {
            return RecommendationReport {
                summary: "Mixed structural signals detected.".to_string(),
                recommendations: vec![Recommendation {
                    target: None,
                    target_pair: Some(t.pair),
                    action: ActionType::HighlightTradeoff,
                    score: round2(t.tension),
                    rationale: format!(
                        "Design tension detected between Concept {} and Concept {} (tension={:.2}). Consider prioritization.",
                        t.pair.0.0,
                        t.pair.1.0,
                        round2(t.tension)
                    ),
                }],
            };
        }

        let mut recommendations = candidates
            .into_iter()
            .map(|c| recommend_one(&q, &c, weights))
            .collect::<Vec<_>>();

        recommendations.sort_by(|l, r| {
            r.score
                .partial_cmp(&l.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| l.target.cmp(&r.target))
        });
        recommendations.truncate(input.top_k);

        let summary = recommendation_summary(&recommendations);
        RecommendationReport {
            summary,
            recommendations,
        }
    }
}

fn recommend_one(
    query: &ConceptQuery,
    c: &ConceptUnit,
    weights: &ResonanceWeights,
) -> Recommendation {
    let s_sim = dot_norm(&query.s, &c.s);
    let a_diff = (query.a - c.a).abs();
    let r = resonance(query, c, *weights);

    // Step3 fallback (no structural conflict/tradeoff in the set): Merge -> Refine -> ApplyPattern
    let action = if r >= 0.60 && a_diff < 0.40 {
        ActionType::Merge
    } else if r >= 0.60 && a_diff >= 0.40 {
        ActionType::Refine
    } else {
        ActionType::ApplyPattern
    };

    let rationale = match action {
        ActionType::Merge => format!(
            "Consider merging with Concept {}. Strong alignment detected (R={:.2}).",
            c.id.0,
            round2(r)
        ),
        ActionType::Refine => format!(
            "Consider refining abstraction alignment with Concept {}. High resonance but abstraction gap detected (Δa={:.2}).",
            c.id.0,
            round2(a_diff)
        ),
        ActionType::ApplyPattern => format!(
            "Consider applying structural pattern from Concept {}. Structural similarity detected (s_sim={:.2}).",
            c.id.0,
            round2(s_sim)
        ),
        ActionType::ResolveStructuralConflict | ActionType::HighlightTradeoff => {
            "See global recommendation context.".to_string()
        }
    };

    Recommendation {
        target: Some(c.id),
        target_pair: None,
        action,
        score: round2(r),
        rationale,
    }
}

fn first_structural_conflict_pair(
    concepts: &[ConceptUnit],
    weights: &ResonanceWeights,
) -> Option<(ConceptId, ConceptId, f32)> {
    for i in 0..concepts.len() {
        for j in (i + 1)..concepts.len() {
            let c1 = &concepts[i];
            let c2 = &concepts[j];
            let query = ConceptQuery {
                v: c1.integrated_vector.clone(),
                a: c1.a,
                s: c1.s.clone(),
                polarity: c1.polarity,
            }
            .normalized();
            let r = resonance(&query, c2, *weights);
            let polarity_conflict = (c1.polarity as i16) * (c2.polarity as i16) < 0;
            if polarity_conflict || r < STRUCTURAL_CONFLICT_THRESHOLD {
                return Some((c1.id, c2.id, r));
            }
        }
    }
    None
}

pub(crate) fn recommendation_summary(recs: &[Recommendation]) -> String {
    let mut merge_refine = 0usize;
    let mut apply = 0usize;
    let mut conflict_related = 0usize;

    for r in recs {
        match r.action {
            ActionType::ResolveStructuralConflict => conflict_related += 2,
            ActionType::HighlightTradeoff => conflict_related += 1,
            ActionType::Merge | ActionType::Refine => merge_refine += 1,
            ActionType::ApplyPattern => apply += 1,
        }
    }

    if merge_refine > apply && merge_refine > conflict_related {
        "Strong integration opportunities detected.".to_string()
    } else if apply > merge_refine && apply > conflict_related {
        "Structural reuse opportunities detected.".to_string()
    } else if conflict_related > merge_refine && conflict_related > apply {
        "Conflict areas require attention.".to_string()
    } else {
        "Mixed structural signals detected.".to_string()
    }
}

pub(crate) fn dot_norm(a: &[f32], b: &[f32]) -> f32 {
    let an = normalize(a);
    let bn = normalize(b);
    an.iter().zip(bn.iter()).map(|(x, y)| x * y).sum::<f32>()
}

fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = l2_norm(v);
    if norm <= f32::EPSILON {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use semantic_dhm::{ConceptId, ConceptQuery, SemanticDhm};

    use super::{ActionType, RecommendationInput, Recomposer};

    fn sample_query(a: f32, seed: f32) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = seed;
        v[1] = 1.0 - seed;
        s[0] = 1.0 - seed;
        s[1] = seed;
        ConceptQuery {
            v,
            a,
            s,
            polarity: 0,
        }
    }

    #[test]
    fn case_d_homogeneous_prefers_merge_or_apply() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.20,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 1.0;
                s
            },
            polarity: 0,
        });
        let query = dhm.get(q_id).expect("q");

        let c_merge_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.98;
                v[1] = 0.02;
                v
            },
            a: 0.25,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.97;
                s[1] = 0.03;
                s
            },
            polarity: 0,
        });
        let c_apply_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.20;
                v[1] = 0.98;
                v
            },
            a: 0.25,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.80;
                s[1] = 0.60;
                s
            },
            polarity: 0,
        });

        let r = Recomposer;
        let rec = r.recommend(
            &RecommendationInput {
                query,
                candidates: vec![
                    dhm.get(c_merge_id).expect("m"),
                    dhm.get(c_apply_id).expect("a"),
                ],
                top_k: 10,
            },
            &dhm.weights(),
        );

        let action_of = |id| -> Option<ActionType> {
            rec.recommendations
                .iter()
                .find(|x| x.target == Some(id))
                .map(|x| x.action)
        };

        assert_eq!(action_of(c_merge_id), Some(ActionType::Merge));
        assert_eq!(action_of(c_apply_id), Some(ActionType::ApplyPattern));
    }

    #[test]
    fn case_a_tradeoff_prefers_highlight_tradeoff() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.2,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 1.0;
                s
            },
            polarity: 0,
        });
        let c_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.60,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 1.0;
                s
            },
            polarity: 0,
        });

        let query = dhm.get(q_id).expect("q");
        let candidate = dhm.get(c_id).expect("c");
        let r = Recomposer;
        let rec = r.recommend(
            &RecommendationInput {
                query,
                candidates: vec![candidate],
                top_k: 1,
            },
            &dhm.weights(),
        );

        assert_eq!(rec.recommendations.len(), 1);
        assert_eq!(rec.recommendations[0].action, ActionType::HighlightTradeoff);
        assert_eq!(
            rec.recommendations[0].target_pair,
            Some((ConceptId(1), ConceptId(2)))
        );
    }

    #[test]
    fn case_b_structural_conflict_prefers_resolve_conflict() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.10,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 1.0;
                s
            },
            polarity: -1,
        });
        let query = dhm.get(q_id).expect("q");

        let conflict_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.90,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = -1.0;
                s
            },
            polarity: 1,
        });

        let r = Recomposer;
        let rec = r.recommend(
            &RecommendationInput {
                query,
                candidates: vec![dhm.get(conflict_id).expect("conflict")],
                top_k: 2,
            },
            &dhm.weights(),
        );

        assert_eq!(rec.recommendations.len(), 1);
        assert_eq!(
            rec.recommendations[0].action,
            ActionType::ResolveStructuralConflict
        );
        assert_eq!(
            rec.recommendations[0].target_pair,
            Some((ConceptId(1), ConceptId(2)))
        );
    }

    #[test]
    fn recommend_top_k_and_deterministic() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q_id = dhm.insert_query(&sample_query(0.40, 0.70));
        let query = dhm.get(q_id).expect("q");

        let mut candidates = Vec::new();
        for i in 0..5 {
            let id = dhm.insert_query(&sample_query(
                0.40 + 0.01 * i as f32,
                0.70 - 0.05 * i as f32,
            ));
            candidates.push(dhm.get(id).expect("c"));
        }

        let r = Recomposer;
        let a = r.recommend(
            &RecommendationInput {
                query: query.clone(),
                candidates: candidates.clone(),
                top_k: 3,
            },
            &dhm.weights(),
        );
        let b = r.recommend(
            &RecommendationInput {
                query,
                candidates,
                top_k: 3,
            },
            &dhm.weights(),
        );
        assert_eq!(a, b);
        assert_eq!(a.recommendations.len(), 3);
    }
}
