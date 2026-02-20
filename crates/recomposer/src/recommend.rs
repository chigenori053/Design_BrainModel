use semantic_dhm::{ConceptId, ConceptQuery, ConceptUnit, ResonanceWeights, resonance};

use crate::Recomposer;
use crate::explain::round2;

#[derive(Clone, Debug, PartialEq)]
pub struct RecommendationInput {
    pub query: ConceptUnit,
    pub candidates: Vec<ConceptUnit>,
    pub top_k: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionType {
    Merge,
    Refine,
    ApplyPattern,
    Separate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub target: ConceptId,
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
            v: input.query.v.clone(),
            a: input.query.a,
            s: input.query.s.clone(),
        }
        .normalized();

        let mut candidates = input.candidates.clone();
        candidates.sort_by(|l, r| l.id.cmp(&r.id));

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

    let action = if r >= 0.60 && a_diff < 0.40 {
        ActionType::Merge
    } else if r >= 0.60 && a_diff >= 0.40 {
        ActionType::Refine
    } else if (0.10..0.60).contains(&r) && s_sim >= 0.60 {
        ActionType::ApplyPattern
    } else if r < -0.10 {
        ActionType::Separate
    } else if r >= 0.10 {
        ActionType::ApplyPattern
    } else {
        ActionType::Separate
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
        ActionType::Separate => format!(
            "Consider separating from Concept {}. Structural conflict detected (R={:.2}).",
            c.id.0,
            round2(r)
        ),
    };

    Recommendation {
        target: c.id,
        action,
        score: round2(r),
        rationale,
    }
}

pub(crate) fn recommendation_summary(recs: &[Recommendation]) -> String {
    let mut merge_refine = 0usize;
    let mut apply = 0usize;
    let mut separate = 0usize;

    for r in recs {
        match r.action {
            ActionType::Merge | ActionType::Refine => merge_refine += 1,
            ActionType::ApplyPattern => apply += 1,
            ActionType::Separate => separate += 1,
        }
    }

    if merge_refine > apply && merge_refine > separate {
        "Strong integration opportunities detected.".to_string()
    } else if apply > merge_refine && apply > separate {
        "Structural reuse opportunities detected.".to_string()
    } else if separate > merge_refine && separate > apply {
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
    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::{ActionType, RecommendationInput, Recomposer};

    fn sample_query(a: f32, seed: f32) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = seed;
        v[1] = 1.0 - seed;
        s[0] = 1.0 - seed;
        s[1] = seed;
        ConceptQuery { v, a, s }
    }

    #[test]
    fn recommend_merge_refine_apply_separate_and_priority() {
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
        });
        let c_refine_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.98;
                v[1] = 0.02;
                v
            },
            a: 0.85,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.96;
                s[1] = 0.04;
                s
            },
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
        });
        let c_sep_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = -0.9;
                v[1] = -0.1;
                v
            },
            a: 0.95,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = -0.9;
                s[1] = -0.1;
                s
            },
        });

        let r = Recomposer;
        let rec = r.recommend(
            &RecommendationInput {
                query,
                candidates: vec![
                    dhm.get(c_refine_id).expect("r"),
                    dhm.get(c_sep_id).expect("s"),
                    dhm.get(c_merge_id).expect("m"),
                    dhm.get(c_apply_id).expect("a"),
                ],
                top_k: 10,
            },
            &dhm.weights(),
        );

        let action_of = |id| {
            rec.recommendations
                .iter()
                .find(|x| x.target == id)
                .map(|x| x.action)
        };

        assert_eq!(action_of(c_merge_id), Some(ActionType::Merge));
        assert_eq!(action_of(c_refine_id), Some(ActionType::Refine));
        assert_eq!(action_of(c_apply_id), Some(ActionType::ApplyPattern));
        assert_eq!(action_of(c_sep_id), Some(ActionType::Separate));
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
